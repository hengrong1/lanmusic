//! 音乐库扫描。统一管线：枚举（实时进度）→ diff → 并发处理（不持锁）→ 独立连接批量入库。
//!
//! 三类来源共用入库与进度事件：
//! - local  ：walkdir 本地遍历 + lofty 标签解析 + .lrc 同名文件索引
//! - webdav ：PROPFIND 遍历 + 只拉文件头部 1MB 解析标签 + 外挂 lrc/封面记录
//! - lan    ：直接从对方设备 /api/tracks 拉取元数据（零逐文件 I/O）+ 远程封面下载
//!
//! 扫描线程自开 SQLite 连接（WAL），UI 查询不受阻塞。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use rusqlite::params;
use rusqlite::OptionalExtension;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use url::Url;
use walkdir::WalkDir;

use crate::covers::COVER_NAMES;
use crate::db;
use crate::metadata::{self, TrackMeta, HEAD_FETCH_SIZE};
use crate::network::webdav;
use crate::state::AppState;

const AUDIO_EXTS: &[&str] = &[
    "mp3", "flac", "m4a", "aac", "ogg", "oga", "opus", "wav", "aif", "aiff", "wma", "ape",
];

/// 视频扩展名（用于检测同名 MV 文件）
const VIDEO_EXTS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "ts",
];
/// 每这么多首提交一次事务并上报一次进度
const BATCH: usize = 100;
/// 并发解析线程上限（I/O 密集，8 线程已能掩盖网络延迟且不至于压垮网络）
const MAX_WORKERS: usize = 8;
/// WebDAV 扫描并发度。远端（尤其经 OpenList/中转网盘）对并发 Range 请求敏感，
/// 首轮 8 路突发容易整批失败，降到 4 更稳。
const WEBDAV_WORKERS: usize = 4;
/// WebDAV 单文件头部拉取的总尝试次数（首次 + 重试）
const WEBDAV_FETCH_ATTEMPTS: usize = 2;

/// 扫描时始终跳过的目录名（NAS 回收站 / 系统元数据等），与用户配置合并生效、
/// 不区分大小写。匹配的是目录名本身（不是路径），任何层级命中都会整棵剪掉。
pub const BUILTIN_SKIP_DIRS: &[&str] = &[
    "#recycle",                  // Synology 回收站
    "#snapshot",                 // Synology 快照
    "@eaDir",                    // Synology 缩略图/索引元数据
    ".@__thumb",                 // QNAP 缩略图
    "$RECYCLE.BIN",              // Windows 回收站
    "System Volume Information", // Windows 卷信息
    "lost+found",                // Linux 文件系统恢复目录
    ".Trash",                    // macOS 回收站（.Trash-uid 变体见 is_skipped_dir 的前缀规则）
    ".Trashes",                  // macOS U 盘回收站
    ".trash",                    // 桌面环境 / 网盘通用回收站
];

/// 用户自定义跳过目录在 app_settings 中的键（逗号/换行分隔的目录名）
pub const SKIP_DIRS_KEY: &str = "scan.skipDirs";

/// 读取生效的跳过目录集合（内置 + 用户配置，统一小写）
pub fn load_skip_dirs(conn: &rusqlite::Connection) -> HashSet<String> {
    let mut set: HashSet<String> = BUILTIN_SKIP_DIRS.iter().map(|s| s.to_lowercase()).collect();
    let raw = db::get_setting(conn, SKIP_DIRS_KEY).unwrap_or_default();
    for part in raw
        .split([',', '，', ';', '；', '\n', '\r'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        set.insert(part.to_lowercase());
    }
    set
}

/// 目录名是否应跳过：精确命中（不区分大小写），或 `.Trash-0` 这类带后缀的回收站
pub fn is_skipped_dir(name: &std::ffi::OsStr, skip: &HashSet<String>) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    let lower = name.to_lowercase();
    skip.contains(&lower) || lower.starts_with(".trash-")
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub source_id: i64,
    /// "enumerate" = 正在枚举（总数未知）；"parse" = 解析入库中
    pub phase: String,
    pub done: usize,
    pub total: usize,
    pub current: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanDone {
    pub source_id: i64,
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub ms: u128,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanError {
    pub source_id: i64,
    pub message: String,
}

/// 工作线程产出的行数据（不含任何 DB 引用，入库在主线程完成）
struct ParsedTrack {
    rel: String,
    title: String,
    artist: String,
    album_title: String,
    album_artist: String,
    genre: Option<String>,
    year: Option<i64>,
    track_no: Option<i64>,
    disc_no: Option<i64>,
    duration: Option<f64>,
    bitrate: Option<i64>,
    sample_rate: Option<i64>,
    channels: Option<i64>,
    bit_depth: Option<i64>,
    has_lyrics: bool,
    has_mv: bool,
    format: Option<String>,
    mtime: i64,
    size: i64,
    /// 0 = 快速导入（仅文件名，待补全解析），1 = 完整解析
    meta_state: i64,
    /// 内嵌歌词原文（用于歌词搜索索引；无内嵌歌词为 None）
    lyrics_text: Option<String>,
}

/// 在后台线程中调用（见 commands::add_local_source / rescan_source 等）。
/// 日志覆盖扫描完整生命周期（开始 / 数量与耗时 / 失败原因），
/// 「未知艺人」「曲库不动」类问题先看这里的记录。
pub fn scan_source(app: AppHandle, source_id: i64, full_rescan: bool) {
    let started = Instant::now();
    log::info!("扫描开始 source {source_id}（{}）", if full_rescan { "完整解析" } else { "增量" });
    let result = load_source(&app, source_id).and_then(|(kind, base_path, base_url, config)| {
        match kind.as_str() {
            "local" => run_local_scan(
                &app,
                source_id,
                PathBuf::from(base_path.unwrap_or_default()),
                full_rescan,
            ),
            "webdav" => run_webdav_scan(&app, source_id, base_url, config, full_rescan),
            other => Err(crate::error::err1(
                crate::error::codes::SOURCE_KIND_UNKNOWN,
                "kind",
                other,
            )),
        }
    });

    let state = app.state::<AppState>();
    state.scanning.lock().unwrap().remove(&source_id);

    match result {
        Ok((added, updated, removed)) => {
            let ms = started.elapsed().as_millis();
            log::info!("扫描完成 source {source_id}：新增 {added}，更新 {updated}，移除 {removed}，耗时 {ms} ms");
            let _ = app.emit(
                "scan:done",
                ScanDone { source_id, added, updated, removed, ms },
            );
            // 扫描历史：落一行供「音乐库体检」展示最近几次增删改
            let state = app.state::<AppState>();
            let conn = state.db.lock().ok();
            if let Some(conn) = conn {
                let _ = conn.execute(
                    "INSERT INTO scan_history (source_id, at, added, updated, removed, ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        source_id,
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64,
                        added as i64,
                        updated as i64,
                        removed as i64,
                        ms as i64
                    ],
                );
            }
        }
        Err(message) => {
            log::error!("扫描失败 source {source_id}：{message}");
            let _ = app.emit("scan:error", ScanError { source_id, message });
        }
    }

    // 封面缓存容量控制：扫描可能新增大量封面，顺带清理一次（低频、纯本地目录扫描，开销可忽略）
    crate::covers::enforce_limit_with_setting(&app);
}

fn load_source(
    app: &AppHandle,
    source_id: i64,
) -> Result<(String, Option<String>, Option<String>, Option<String>), String> {
    let state = app.state::<AppState>();
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT kind, base_path, base_url, config FROM sources WHERE id = ?1",
        [source_id],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        },
    )
    .map_err(|e| e.to_string())
}

fn emit_enumerate(app: &AppHandle, source_id: i64, done: usize, current: String) {
    let _ = app.emit(
        "scan:progress",
        ScanProgress {
            source_id,
            phase: "enumerate".into(),
            done,
            total: 0,
            current,
        },
    );
}

fn emit_parse(app: &AppHandle, source_id: i64, done: usize, total: usize) {
    let _ = app.emit(
        "scan:progress",
        ScanProgress {
            source_id,
            phase: "parse".into(),
            done,
            total,
            current: String::new(),
        },
    );
}

// ================================================================ 本地扫描

fn run_local_scan(
    app: &AppHandle,
    source_id: i64,
    base: PathBuf,
    full_rescan: bool,
) -> Result<(usize, usize, usize), String> {
    let state = app.state::<AppState>();
    let mut conn = db::open_conn(&state.db_path, false).map_err(|e| e.to_string())?;
    let (fast_import, scan_subdirs): (bool, bool) = conn
        .query_row(
            "SELECT fast_import, scan_subdirs FROM sources WHERE id = ?1",
            [source_id],
            |r| Ok((r.get::<_, i64>(0)? != 0, r.get::<_, i64>(1)? != 0)),
        )
        .map_err(|e| e.to_string())?;
    let skip_dirs = load_skip_dirs(&conn);

    // ---- 1. 枚举目录 + 收集 .lrc + 检测视频文件 ----
    let enum_started = std::time::Instant::now();
    let mut files: Vec<(String, i64, i64)> = Vec::new();
    let mut lrc_map: HashMap<String, String> = HashMap::new(); // rel 去扩展名 → 本地 .lrc 绝对路径
    let mut video_stems: HashSet<String> = HashSet::new(); // 视频文件的 stem_key 集合
    let mut seen = 0usize; // 遍历的文件总数（含非音频文件，进度按此上报更平滑）
                           // 跳过目录 + 子目录开关：
                           // - 命中跳过名单的目录整个剪掉（filter_entry 不再下降），回收站/系统目录不进曲库
                           // - scan_subdirs = false 时 max_depth(1) 只看根目录的直接文件
    let walk = WalkDir::new(&base).follow_links(false);
    let walker: Box<dyn Iterator<Item = walkdir::Result<walkdir::DirEntry>>> =
        if scan_subdirs {
            Box::new(walk.into_iter().filter_entry(move |e| {
                e.depth() == 0 || !is_skipped_dir(e.file_name(), &skip_dirs)
            }))
        } else {
            Box::new(walk.max_depth(1).into_iter())
        };
    for entry in walker.filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = rel_path(&base, entry.path());
        seen += 1;
        if seen.is_multiple_of(1000) {
            emit_enumerate(app, source_id, files.len(), file_name_of(&rel));
        }
        let Some(ext) = entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
        else {
            continue;
        };
        if ext == "lrc" || ext == "qrc" {
            lrc_map.insert(
                stem_key(&rel),
                base.join(&rel).to_string_lossy().to_string(),
            );
            continue;
        }
        // 检测视频文件
        if VIDEO_EXTS.contains(&ext.as_str()) {
            video_stems.insert(stem_key(&rel));
            continue;
        }
        if !AUDIO_EXTS.contains(&ext.as_str()) {
            continue;
        }
        let md = entry.metadata().ok();
        let mtime = md
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let size = md.map(|m| m.len() as i64).unwrap_or(0);
        files.push((rel, mtime, size));
    }
    let total = files.len();
    emit_parse(app, source_id, 0, total);

    // ---- 2. diff ----
    let existing = load_existing(&conn, source_id)?;
    let to_parse: Vec<(String, i64, i64)> = files
        .iter()
        .filter(|(p, mtime, size)| {
            needs_parse(&existing, p, *mtime, *size, full_rescan, fast_import)
        })
        .cloned()
        .collect();

    // 诊断：枚举阶段耗时（到 diff 开始为止）
    crate::diagnostics::LAST_SCAN_ENUM_MS.store(enum_started.elapsed().as_millis() as u64, std::sync::atomic::Ordering::Relaxed);
    let parse_started = std::time::Instant::now();

    // ---- 3. 并发解析 ----
    let base_c = base.clone();
    let video_stems_c = video_stems.clone();
    // 「完整解析」要无视快速导入（见设置页 fullParseTip：含快速导入的歌曲也要重新解析），
    // 否则开了快速导入后点「完整解析」会被这里的 early-return 吃掉、什么都不做。
    let use_fast = fast_import && !full_rescan;
    let rx = run_concurrent(to_parse, MAX_WORKERS, move |(rel, mtime, size)| {
        let has_mv = video_stems_c.contains(&stem_key(&rel));
        if use_fast {
            return Some(fast_track(&rel, mtime, size, has_mv));
        }
        let full = base_c.join(&rel);
        match metadata::read(&full, false) {
            Ok(m) => Some(parsed_from_meta(&rel, m, mtime, size, has_mv)),
            // 本地文件读不到标签属于文件本身的问题，标 1 不再重试
            Err(_) => Some(fallback_track(&rel, mtime, size, has_mv, 1)),
        }
    });

    // ---- 4. 批量入库 ----
    let (added, updated) = consume_and_write(
        &mut conn,
        app,
        source_id,
        rx,
        &existing,
        Some(&lrc_map),
        None,
        total,
    )?;

    // ---- 4.5 歌词关联补全：覆盖未被重新解析的旧曲目 ----
    if !lrc_map.is_empty() {
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        for (rel, _, _) in &files {
            let Some(target) = lrc_map.get(&stem_key(rel)) else {
                continue;
            };
            let tid: Option<i64> = tx
                .query_row(
                    "SELECT id FROM tracks WHERE source_id = ?1 AND path = ?2",
                    params![source_id, rel],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if let Some(tid) = tid {
                tx.execute(
                    "INSERT INTO lrc_files (track_id, path) VALUES (?1, ?2)
                     ON CONFLICT(track_id) DO UPDATE SET path = excluded.path",
                    params![tid, target],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())?;
    }

    // ---- 5. 删除已消失的文件 + 更新扫描时间 ----
    let walked: HashSet<String> = files.iter().map(|(p, _, _)| p.clone()).collect();
    let removed = delete_missing(app, &mut conn, source_id, &existing, &walked)?;
    let now = now_secs();
    conn.execute(
        "UPDATE sources SET last_scan_at = ?1 WHERE id = ?2",
        params![now, source_id],
    )
    .map_err(|e| e.to_string())?;

    // 诊断：解析+diff 阶段耗时（最近一次）
    crate::diagnostics::LAST_SCAN_PARSE_MS.store(parse_started.elapsed().as_millis() as u64, std::sync::atomic::Ordering::Relaxed);

    Ok((added, updated, removed))
}

// ================================================================ WebDAV 扫描

fn run_webdav_scan(
    app: &AppHandle,
    source_id: i64,
    base_url: Option<String>,
    config: Option<String>,
    full_rescan: bool,
) -> Result<(usize, usize, usize), String> {
    let Some(base_str) = base_url else {
        return Err(crate::error::err(crate::error::codes::SOURCE_URL_MISSING));
    };
    let base = webdav::normalize_base(&base_str)?;
    let auth = webdav::Auth::from_source(config.as_deref(), source_id);
    let base_path = decoded_url_path(&base).trim_end_matches('/').to_string();
    // 跳过目录 / 子目录开关与本地扫描同一套口径
    let (skip_dirs, scan_subdirs) = {
        let state = app.state::<AppState>();
        let conn = db::open_conn(&state.db_path, false).map_err(|e| e.to_string())?;
        let subdirs: bool = conn
            .query_row(
                "SELECT scan_subdirs FROM sources WHERE id = ?1",
                [source_id],
                |r| r.get::<_, i64>(0).map(|v| v != 0),
            )
            .unwrap_or(true);
        (load_skip_dirs(&conn), subdirs)
    };

    // ---- 1. PROPFIND 遍历 ----
    let enum_started = std::time::Instant::now();
    let mut files: Vec<(String, i64)> = Vec::new(); // (rel, size)
    let mut lrc_map: HashMap<String, String> = HashMap::new(); // rel 去扩展名 → 完整 URL
    let mut cover_map: HashMap<String, String> = HashMap::new(); // 目录 rel → 封面 URL
    let mut queue = vec![base.clone()];
    let mut visited: HashSet<String> = HashSet::new();
    while let Some(dir) = queue.pop() {
        let dpath = decoded_url_path(&dir);
        if !visited.insert(dpath) {
            continue;
        }
        let items = match webdav::list_dir(&dir, auth.as_ref()) {
            Ok(items) => items,
            Err(e) => {
                // 根目录失败直接报错；子目录失败跳过继续
                if visited.len() == 1 {
                    return Err(e);
                }
                continue;
            }
        };
        for item in items {
            // 相对源根的路径（用于曲库展示与 diff）
            let rel = item
                .abs
                .strip_prefix(&base_path)
                .unwrap_or(&item.abs)
                .trim_start_matches('/')
                .to_string();
            if rel.is_empty() {
                continue;
            }
            if item.is_dir {
                // 命中跳过名单的目录不进入（其内容不会被枚举）；
                // 关闭子目录扫描时只列根目录，不再下钻
                let dir_name = file_name_of(&rel);
                if is_skipped_dir(std::path::Path::new(&dir_name).as_os_str(), &skip_dirs) {
                    continue;
                }
                if scan_subdirs {
                    let child = base
                        .join(&format!("{rel}/"))
                        .unwrap_or_else(|_| base.clone());
                    queue.push(child);
                }
            } else {
                let ext = ext_of(&rel);
                let name = file_name_of(&rel).to_ascii_lowercase();
                if matches!(ext.as_deref(), Some("lrc") | Some("qrc")) {
                    lrc_map.insert(stem_key(&rel), webdav::file_url(&base, &rel).to_string());
                } else if COVER_NAMES.contains(&name.as_str()) {
                    cover_map.insert(parent_dir(&rel), webdav::file_url(&base, &rel).to_string());
                } else if AUDIO_EXTS.contains(&ext.as_deref().unwrap_or("")) {
                    files.push((rel, item.size));
                }
            }
        }
        emit_enumerate(
            app,
            source_id,
            files.len(),
            file_name_of(&decoded_url_path(&dir)),
        );
    }
    let total = files.len();
    emit_parse(app, source_id, 0, total);
    // 诊断：PROPFIND 枚举阶段耗时
    crate::diagnostics::LAST_SCAN_ENUM_MS.store(enum_started.elapsed().as_millis() as u64, std::sync::atomic::Ordering::Relaxed);
    let parse_started = std::time::Instant::now();

    // ---- 2. diff（mtime 不可靠，仅按 size + meta_state）----
    // 快速导入与本地扫描共用同一个来源开关：开启后只按文件名/目录结构入库，
    // 完全不发网络请求（对经 OpenList 中转的云端库，能省掉「每文件拉 1MB」的开销）。
    let (existing, fast_import) = {
        let state = app.state::<AppState>();
        let conn = db::open_conn(&state.db_path, false).map_err(|e| e.to_string())?;
        let fast: bool = conn
            .query_row(
                "SELECT fast_import FROM sources WHERE id = ?1",
                [source_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        (load_existing(&conn, source_id)?, fast)
    };
    let to_parse: Vec<(String, i64)> = files
        .iter()
        .filter(|(p, size)| needs_parse(&existing, p, 0, *size, full_rescan, fast_import))
        .cloned()
        .collect();

    // ---- 3. 并发拉取头部字节并解析标签 ----
    let base_c = base.clone();
    let auth_c = auth.clone();
    // 同本地扫描：「完整解析」无视快速导入，否则点了也不会真的解析
    let use_fast = fast_import && !full_rescan;
    let rx = run_concurrent(to_parse, WEBDAV_WORKERS, move |(rel, size)| {
        // 快速导入：只按文件名/目录结构入库（meta_state=0，待补全），一次网络请求都不发。
        // 云端曲库开这个能直接跳过「每文件拉 1MB」，也顺带避开远端限流。
        if use_fast {
            return Some(fast_track(&rel, 0, size, false));
        }
        let url = webdav::file_url(&base_c, &rel);
        // 远端失败多是瞬时的（上游限流、连接复用被掐、冷启动超时）：重试一次再放弃。
        // 不重试的话，首轮并发突发失败的那批会直接落成「未知艺人/未知专辑」，
        // 而它们 meta_state 已是 1，增量扫描不会再碰，等于永久损坏。
        let mut bytes = None;
        let mut last_err = String::new();
        for _ in 0..WEBDAV_FETCH_ATTEMPTS {
            match webdav::download(&url, auth_c.as_ref(), Some((0, HEAD_FETCH_SIZE - 1))) {
                Ok(b) => {
                    bytes = Some(b);
                    break;
                }
                Err(e) => {
                    // 429/401/403 不重试：OpenList 对 WebDAV 认证失败按 IP 计次封锁，
                    // 且每个被挡的请求都会续期封锁窗口，重试只会让锁定持续更久。
                    let retryable = crate::network::is_retryable(&e);
                    last_err = e;
                    if !retryable {
                        break;
                    }
                }
            }
        }
        let Some(bytes) = bytes else {
            // 关键日志：WebDAV「未知艺人」的直接根因（重试后仍拉不到头部字节）。
            // meta_state=0 会留给下次扫描重试；若同一路径反复出现即上游持续限流/断网。
            log::warn!("[webdav] 头部拉取失败 {rel}: {last_err}");
            // 一个字节都没拿到：标 0 待补全，下次扫描或「完整解析」会重试
            return Some(fallback_track(&rel, 0, size, false, 0));
        };
        match metadata::read_bytes(&bytes, false) {
            Ok(m) => Some(parsed_from_meta(&rel, m, 0, size, false)),
            // 拿到字节却解析失败：文件本身的问题，标 1 不再重试
            Err(_) => Some(fallback_track(&rel, 0, size, false, 1)),
        }
    });

    // ---- 4. 批量入库（含 lrc / cover 关联）----
    let state = app.state::<AppState>();
    let mut conn = db::open_conn(&state.db_path, false).map_err(|e| e.to_string())?;
    let (added, updated) = consume_and_write(
        &mut conn,
        app,
        source_id,
        rx,
        &existing,
        Some(&lrc_map),
        Some(&cover_map),
        total,
    )?;

    // ---- 5. 删除 + 扫描时间 ----
    let walked: HashSet<String> = files.iter().map(|(p, _)| p.clone()).collect();
    let removed = delete_missing(app, &mut conn, source_id, &existing, &walked)?;
    let now = now_secs();
    conn.execute(
        "UPDATE sources SET last_scan_at = ?1 WHERE id = ?2",
        params![now, source_id],
    )
    .map_err(|e| e.to_string())?;

    // 诊断：解析+diff 阶段耗时（最近一次）
    crate::diagnostics::LAST_SCAN_PARSE_MS.store(parse_started.elapsed().as_millis() as u64, std::sync::atomic::Ordering::Relaxed);

    Ok((added, updated, removed))
}

// ================================================================ 共用写入管线

pub(crate) struct ScanCaches {
    artists: HashMap<String, i64>,
    /// key → 本地专辑 id
    albums: HashMap<String, i64>,
    /// 艺人别名（小写）→ 主艺人展示名：规整/自定义合并的「合并记忆」，
    /// 扫描再遇到旧名时仍归到主艺人名下（否则旧名会被重新建成独立艺人）
    aliases: HashMap<String, String>,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub(crate) fn load_caches(conn: &rusqlite::Connection) -> Result<ScanCaches, String> {
    let mut artists = HashMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT id, name FROM artists")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (id, name) = row.map_err(|e| e.to_string())?;
            artists.insert(name.to_lowercase(), id);
        }
    }
    let mut albums = HashMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT id, key FROM albums")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (id, key) = row.map_err(|e| e.to_string())?;
            albums.insert(key, id);
        }
    }
    let mut aliases = HashMap::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT aa.alias, a.name FROM artist_aliases aa \
                 JOIN artists a ON a.id = aa.artist_id",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (alias, name) = row.map_err(|e| e.to_string())?;
            aliases.insert(alias.to_lowercase(), name);
        }
    }
    Ok(ScanCaches {
        artists,
        albums,
        aliases,
    })
}

fn load_existing(
    conn: &rusqlite::Connection,
    source_id: i64,
) -> Result<HashMap<String, (i64, i64, i64)>, String> {
    let mut map = HashMap::new();
    let mut stmt = conn
        .prepare("SELECT path, IFNULL(mtime,0), IFNULL(file_size,0), meta_state FROM tracks WHERE source_id = ?1")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([source_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                (
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                ),
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (p, v) = row.map_err(|e| e.to_string())?;
        map.insert(p, v);
    }
    Ok(map)
}

fn needs_parse(
    existing: &HashMap<String, (i64, i64, i64)>,
    p: &str,
    mtime: i64,
    size: i64,
    full_rescan: bool,
    fast_import: bool,
) -> bool {
    if full_rescan {
        return true;
    }
    match existing.get(p) {
        None => true,
        Some((em, es, st)) => *em != mtime || *es != size || (*st == 0 && !fast_import),
    }
}

/// 消费工作线程产出并批量入库；返回 (added, updated)
fn consume_and_write(
    conn: &mut rusqlite::Connection,
    app: &AppHandle,
    source_id: i64,
    rx: impl IntoIterator<Item = ParsedTrack>,
    existing: &HashMap<String, (i64, i64, i64)>,
    lrc_map: Option<&HashMap<String, String>>,
    cover_map: Option<&HashMap<String, String>>,
    total: usize,
) -> Result<(usize, usize), String> {
    // 艺人分隔符设置：扫描开始时读取一次，本批全部沿用
    let seps = {
        let s = db::get_setting(conn, ARTIST_SEPARATORS_KEY).unwrap_or_default();
        parse_separators(&s)
    };
    let mut caches = load_caches(conn)?;
    let covers_dir = app.state::<AppState>().covers_dir.clone();
    let now = now_secs();
    let mut added = 0usize;
    let mut updated = 0usize;
    let mut done = 0usize;
    let mut batch: Vec<ParsedTrack> = Vec::with_capacity(BATCH);

    for row in rx {
        if existing.contains_key(&row.rel) {
            updated += 1;
        } else {
            added += 1;
        }
        batch.push(row);
        done += 1;
        if batch.len() >= BATCH {
            write_batch(
                conn,
                &mut caches,
                source_id,
                now,
                &mut batch,
                existing,
                &covers_dir,
                lrc_map,
                cover_map,
                &seps,
            )?;
            emit_parse(app, source_id, done, total);
        }
    }
    write_batch(
        conn,
        &mut caches,
        source_id,
        now,
        &mut batch,
        existing,
        &covers_dir,
        lrc_map,
        cover_map,
        &seps,
    )?;
    emit_parse(app, source_id, done, total);
    Ok((added, updated))
}

fn write_batch(
    conn: &mut rusqlite::Connection,
    caches: &mut ScanCaches,
    source_id: i64,
    now: i64,
    batch: &mut Vec<ParsedTrack>,
    existing: &HashMap<String, (i64, i64, i64)>,
    covers_dir: &Path,
    lrc_map: Option<&HashMap<String, String>>,
    cover_map: Option<&HashMap<String, String>>,
    seps: &[char],
) -> Result<(), String> {
    if batch.is_empty() {
        return Ok(());
    }
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    for row in batch.drain(..) {
        // 多艺人拆分："A / B" → [A, B]，首个作为主艺人（tracks.artist_id，兼容旧查询/排序）
        let artist_ids = split_artists(&row.artist, seps)
            .iter()
            .map(|n| get_or_create_artist(&tx, caches, n))
            .collect::<Result<Vec<i64>, String>>()?;
        let artist_id = artist_ids.first().copied().unwrap_or(0);
        // 专辑归属按合辑艺人（album_artist）入库，艺人/专辑归类才与标签语义一致
        let album_artist_id = get_or_create_artist(&tx, caches, &row.album_artist)?;

        let album_key = format!(
            "{}|{}|{}",
            row.album_title.to_lowercase(),
            row.album_artist.to_lowercase(),
            row.year.unwrap_or(0)
        );
        let album_id = match caches.albums.get(&album_key) {
            Some(id) => *id,
            None => {
                tx.execute(
                    "INSERT INTO albums (title, artist_id, year, key) VALUES (?1, ?2, ?3, ?4)",
                    params![row.album_title, album_artist_id, row.year, album_key],
                )
                .map_err(|e| e.to_string())?;
                let id = tx.last_insert_rowid();
                caches.albums.insert(album_key, id);
                id
            }
        };

        tx.execute(
            "INSERT INTO tracks (source_id, path, title, artist_id, album_id, genre, track_no, disc_no,
                                  year, duration, bitrate, sample_rate, channels, bit_depth,
                                  has_embedded_lyrics, has_mv, mtime, file_size, format, added_at, meta_state, raw_artist)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22)
             ON CONFLICT(source_id, path) DO UPDATE SET
                title=excluded.title, artist_id=excluded.artist_id, album_id=excluded.album_id,
                genre=excluded.genre, track_no=excluded.track_no, disc_no=excluded.disc_no,
                year=excluded.year, duration=excluded.duration, bitrate=excluded.bitrate,
                sample_rate=excluded.sample_rate, channels=excluded.channels, bit_depth=excluded.bit_depth,
                has_embedded_lyrics=excluded.has_embedded_lyrics, has_mv=excluded.has_mv,
                mtime=excluded.mtime, file_size=excluded.file_size, format=excluded.format, meta_state=excluded.meta_state,
                raw_artist=excluded.raw_artist",
            params![
                source_id,
                row.rel,
                row.title,
                artist_id,
                album_id,
                row.genre,
                row.track_no,
                row.disc_no,
                row.year,
                row.duration,
                row.bitrate,
                row.sample_rate,
                row.channels,
                row.bit_depth,
                row.has_lyrics as i64,
                row.has_mv as i64,
                row.mtime,
                row.size,
                row.format,
                now,
                row.meta_state,
                // 原始艺人标签：仅完整解析行记录（快速导入行取自目录名，重拆无意义，留空待解析）
                if row.meta_state == 1 { Some(row.artist.as_str()) } else { None },
            ],
        )
        .map_err(|e| e.to_string())?;
        // 通过查询获取正确的 track_id（ON CONFLICT DO UPDATE 时 last_insert_rowid 不更新）
        let track_id: i64 = tx
            .query_row(
                "SELECT id FROM tracks WHERE source_id = ?1 AND path = ?2",
                params![source_id, row.rel],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;

        // 多艺人关联：先清后插保证重复扫描幂等（更新行会重建关联）
        tx.execute(
            "DELETE FROM track_artists WHERE track_id = ?1",
            params![track_id],
        )
        .map_err(|e| e.to_string())?;
        for (i, aid) in artist_ids.iter().enumerate() {
            tx.execute(
                "INSERT OR IGNORE INTO track_artists (track_id, artist_id, ord) VALUES (?1, ?2, ?3)",
                params![track_id, aid, i as i64],
            )
            .map_err(|e| e.to_string())?;
        }

        // 歌词关联：local/webdav 存同名 .lrc 的本地路径或完整 URL
        let lrc_target = lrc_map.and_then(|m| m.get(&stem_key(&row.rel)).cloned());
        if let Some(target) = lrc_target.as_ref() {
            let p = if target.is_empty() {
                None
            } else {
                Some(target.as_str())
            };
            tx.execute(
                "INSERT INTO lrc_files (track_id, path) VALUES (?1, ?2)
                 ON CONFLICT(track_id) DO UPDATE SET path = excluded.path",
                params![track_id, p],
            )
            .map_err(|e| e.to_string())?;
        }

        // 歌词搜索索引：优先内嵌歌词原文，其次读外挂 .lrc 文件内容（小文件，批次内读取开销可忽略）；
        // 完整解析后仍无歌词则清除旧索引（歌词被移除的情况）
        let lyrics_text = row.lyrics_text.clone().or_else(|| {
            lrc_target
                .as_ref()
                .filter(|p| !p.is_empty())
                .and_then(|p| std::fs::read(p).ok())
                .map(|b| crate::lyrics::decode_lyric_bytes(&b))
        });
        match lyrics_text {
            Some(text) if !text.trim().is_empty() => {
                tx.execute(
                    "INSERT INTO lyrics_index (track_id, text) VALUES (?1, ?2)
                     ON CONFLICT(track_id) DO UPDATE SET text = excluded.text",
                    params![track_id, text],
                )
                .map_err(|e| e.to_string())?;
            }
            _ if row.meta_state == 1 => {
                tx.execute(
                    "DELETE FROM lyrics_index WHERE track_id = ?1",
                    params![track_id],
                )
                .map_err(|e| e.to_string())?;
            }
            _ => {}
        }

        // WebDAV 封面 URL（首个出现的曲目决定，条件更新保证幂等）
        if let Some(cm) = cover_map {
            if let Some(u) = cm.get(&parent_dir(&row.rel)) {
                tx.execute(
                    "UPDATE albums SET cover_url = ?1 WHERE id = ?2 AND cover_url IS NULL",
                    params![u, album_id],
                )
                .map_err(|e| e.to_string())?;
            }
        }

        // 封面缓存失效：曲目是重新解析的更新行 → 文件内容可能已变化（如改了内嵌封面），
        // 作废其专辑的缓存，待下次展示时重新提取；新插入行 → 若专辑此前被判定"无封面"
        // （{id}.none 哨兵），解除哨兵以便用新文件重试
        if existing.contains_key(&row.rel) {
            crate::covers::purge(covers_dir, &[album_id]);
        } else if covers_dir.join(format!("{album_id}.none")).is_file() {
            let _ = std::fs::remove_file(covers_dir.join(format!("{album_id}.none")));
        }
    }
    tx.commit().map_err(|e| e.to_string())
}

/// 解析艺人名：先规整（剥离尾部括号注释），命中别名时替换为主艺人展示名。
/// 专辑 key 与艺人入库都以解析结果为准，保证同一人的不同写法归到同一处。
fn resolve_artist(caches: &ScanCaches, name: &str) -> String {
    let base = canonical_artist(name.trim());
    match caches.aliases.get(&base.to_lowercase()) {
        Some(target) => target.clone(),
        None => base,
    }
}

pub(crate) fn get_or_create_artist(
    tx: &rusqlite::Transaction,
    caches: &mut ScanCaches,
    name: &str,
) -> Result<i64, String> {
    // 规整名入库与匹配：「陈奕迅（Eason Chan）」与「陈奕迅」命中同一位艺人，展示名取规整名；
    // 别名（合并记忆）优先级在规整之后——旧名直接落到主艺人名下，不再重建独立艺人
    let base = resolve_artist(caches, name);
    let key = base.to_lowercase();
    if let Some(id) = caches.artists.get(&key) {
        return Ok(*id);
    }
    tx.execute("INSERT OR IGNORE INTO artists (name) VALUES (?1)", [&base])
        .map_err(|e| e.to_string())?;
    let id: i64 = tx
        .query_row(
            "SELECT id FROM artists WHERE name = ?1 COLLATE NOCASE",
            [&base],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    caches.artists.insert(key, id);
    Ok(id)
}

fn delete_missing(
    app: &AppHandle,
    conn: &mut rusqlite::Connection,
    source_id: i64,
    existing: &HashMap<String, (i64, i64, i64)>,
    walked: &HashSet<String>,
) -> Result<usize, String> {
    let to_remove: Vec<String> = existing
        .keys()
        .filter(|p| !walked.contains(*p))
        .cloned()
        .collect();
    let mut removed = 0usize;
    for chunk in to_remove.chunks(500) {
        let tx = conn.transaction().map_err(|e| e.to_string())?;

        // 先收集要删除的 track_ids，并显式清理子表引用
        //（兼容未配置 ON DELETE CASCADE 的旧数据库，避免 FOREIGN KEY constraint failed）
        // 取待删曲目的展示信息（写移除记录用）+ id
        struct RemovedInfo {
            id: i64,
            title: String,
            artist: Option<String>,
            album: Option<String>,
            path: String,
        }
        let mut removed_infos: Vec<RemovedInfo> = Vec::with_capacity(chunk.len());
        for rel in chunk {
            let info = tx
                .query_row(
                    "SELECT t.id, t.title, a.name, al.title, t.path FROM tracks t \
                     LEFT JOIN artists a ON a.id = t.artist_id \
                     LEFT JOIN albums al ON al.id = t.album_id \
                     WHERE t.source_id = ?1 AND t.path = ?2",
                    params![source_id, rel],
                    |r| {
                        Ok(RemovedInfo {
                            id: r.get(0)?,
                            title: r.get(1)?,
                            artist: r.get(2)?,
                            album: r.get(3)?,
                            path: r.get(4)?,
                        })
                    },
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if let Some(info) = info {
                removed_infos.push(info);
            }
        }
        let track_ids: Vec<i64> = removed_infos.iter().map(|i| i.id).collect();
        // 写移除记录（设置 → 已移除歌曲 可查看）：文件消失属于「扫描移除」
        let removed_at = now_secs();
        for info in &removed_infos {
            tx.execute(
                "INSERT INTO removed_tracks (title, artist, album, path, source_id, reason, removed_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, 'scan', ?6)",
                params![info.title, info.artist, info.album, info.path, source_id, removed_at],
            )
            .map_err(|e| e.to_string())?;
        }
        // 显式删除子表引用（track_artists, playlist_items, lrc_files, lyrics_index）
        for tid in &track_ids {
            tx.execute(
                "DELETE FROM track_artists WHERE track_id = ?1",
                params![tid],
            )
            .map_err(|e| e.to_string())?;
            tx.execute(
                "DELETE FROM playlist_items WHERE track_id = ?1",
                params![tid],
            )
            .map_err(|e| e.to_string())?;
            tx.execute("DELETE FROM lrc_files WHERE track_id = ?1", params![tid])
                .map_err(|e| e.to_string())?;
            tx.execute("DELETE FROM lyrics_index WHERE track_id = ?1", params![tid])
                .map_err(|e| e.to_string())?;
        }
        // 然后删除 tracks
        for rel in chunk {
            tx.execute(
                "DELETE FROM tracks WHERE source_id = ?1 AND path = ?2",
                params![source_id, rel],
            )
            .map_err(|e| e.to_string())?;
            removed += 1;
        }
        // 先收集本次变孤儿的专辑 id（在删除 tracks 之后、删除 albums 之前，此时判定准确），
        // 提交后同步清理封面缓存，防止 rowid 复用后新专辑命中旧封面
        let orphan_albums: Vec<i64> = {
            let mut stmt = tx
                .prepare(
                    "SELECT id FROM albums WHERE id NOT IN (SELECT DISTINCT album_id FROM tracks)",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |r| r.get::<_, i64>(0))
                .map_err(|e| e.to_string())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?
        };
        tx.execute(
            "DELETE FROM albums WHERE id NOT IN (SELECT DISTINCT album_id FROM tracks)",
            [],
        )
        .map_err(|e| e.to_string())?;
        // 保留仍被专辑引用的艺人（专辑归属艺人可能没有直接归属的曲目）
        tx.execute(
            "DELETE FROM artists WHERE id NOT IN (SELECT DISTINCT artist_id FROM tracks)
             AND id NOT IN (SELECT DISTINCT artist_id FROM track_artists)
             AND id NOT IN (SELECT DISTINCT artist_id FROM albums)",
            [],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        if !orphan_albums.is_empty() {
            crate::covers::purge(&app.state::<AppState>().covers_dir, &orphan_albums);
        }
    }
    // 移除记录裁剪到上限（旧记录让位给新记录）
    db::cap_removed_log(conn).map_err(|e| e.to_string())?;
    Ok(removed)
}

// ================================================================ 并发工作线程

/// 多线程并发执行 f：任务逐条从共享队列领取（避免静态均分导致大文件集中在
/// 单个分片时拖尾），结果经 channel 返回主线程
fn run_concurrent<TIn, TOut>(
    items: Vec<TIn>,
    max_workers: usize,
    f: impl Fn(TIn) -> Option<TOut> + Send + Sync + Clone + 'static,
) -> mpsc::Receiver<TOut>
where
    TIn: Send + 'static,
    TOut: Send + 'static,
{
    use std::sync::{Arc, Mutex};

    let (tx, rx) = mpsc::channel::<TOut>();
    let workers = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(2, max_workers.max(2));
    let queue = Arc::new(Mutex::new(items.into_iter()));
    for _ in 0..workers {
        let tx = tx.clone();
        let f = f.clone();
        let queue = queue.clone();
        thread::spawn(move || loop {
            let item = {
                let mut q = match queue.lock() {
                    Ok(q) => q,
                    Err(_) => return, // 锁中毒：其他线程 panic，直接退出
                };
                q.next()
            };
            let Some(item) = item else { break };
            if let Some(out) = f(item) {
                if tx.send(out).is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);
    rx
}

// ================================================================ 行构造

fn rel_path(base: &Path, full: &Path) -> String {
    full.strip_prefix(base)
        .unwrap_or(full)
        .to_string_lossy()
        .replace('\\', "/")
}

fn decoded_url_path(u: &Url) -> String {
    percent_encoding::percent_decode_str(u.path())
        .decode_utf8_lossy()
        .into_owned()
}

fn ext_of(rel: &str) -> Option<String> {
    Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
}

fn stem_of(rel: &str) -> String {
    Path::new(rel)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| rel.to_string())
}

/// "A/B/song.flac" → "A/B/song"
fn stem_key(rel: &str) -> String {
    Path::new(rel)
        .with_extension("")
        .to_string_lossy()
        .replace('\\', "/")
}

fn parent_dir(rel: &str) -> String {
    match rel.rfind('/') {
        Some(i) => rel[..i].to_string(),
        None => String::new(),
    }
}

fn file_name_of(rel: &str) -> String {
    Path::new(rel)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// 多艺人分隔符默认集（含中文标点与常见合作标注；拆分前 feat./ft./featuring 会先归一为 ';'）。
/// 用户可在设置里按候选集开关，无设置时使用全集。
pub const ARTIST_SEPARATORS: &[char] = &[';', '；', '、', '&', '，', ',', '/'];

/// 设置页可选的分隔符候选集（与默认集一致）
pub const SEPARATOR_CANDIDATES: &[char] = ARTIST_SEPARATORS;

/// 艺人分隔符设置在 app_settings 中的键
pub const ARTIST_SEPARATORS_KEY: &str = "lm.artistSeparators";

/// 解析分隔符设置串：只保留候选字符并去重。
/// ';'（含 feat. 归一目标）恒定保留；无任何候选字符（未设置）时回退默认全集。
pub fn parse_separators(s: &str) -> Vec<char> {
    let mut seps: Vec<char> = Vec::new();
    for c in s.chars() {
        if SEPARATOR_CANDIDATES.contains(&c) && !seps.contains(&c) {
            seps.push(c);
        }
    }
    if seps.is_empty() {
        return ARTIST_SEPARATORS.to_vec();
    }
    if !seps.contains(&';') {
        seps.insert(0, ';');
    }
    seps
}

/// 艺人名规整：剥离尾部括号注释（全角（）/半角()），如「陈奕迅（Eason Chan）」→「陈奕迅」。
/// 规则：取字符串中最后一个开括号，若它不在串首、整串以对应闭括号结尾、且括号内
/// 含非空白内容，则其之前的内容（去首尾空白）即为规整名；否则保留原名。
/// 这样「陈奕迅」「陈奕迅（Eason Chan）」会命中同一位艺人，展示名统一为规整名。
pub fn canonical_artist(name: &str) -> String {
    let t = name.trim();
    if t.is_empty() {
        return name.to_string();
    }
    // 最后一个开括号位置（字节偏移）
    let mut open: Option<(char, usize)> = None;
    for (pos, c) in t.char_indices() {
        if c == '（' || c == '(' {
            open = Some((c, pos));
        }
    }
    if let Some((open_c, pos)) = open {
        // 开括号必须不在串首（前面还有艺人名），且串以对应闭括号结束
        let (open_len, close_len, closes): (usize, usize, char) = if open_c == '（' {
            (3, 3, '）')
        } else {
            (1, 1, ')')
        };
        if pos > 0 && t.ends_with(closes) {
            let inner = t[pos + open_len..t.len() - close_len].trim();
            if !inner.is_empty() {
                let base = t[..pos].trim();
                if !base.is_empty() {
                    return base.to_string();
                }
            }
        }
    }
    t.to_string()
}

/// 把 "A / B"、"A & B"、"A feat. B" 这类多艺人字符串拆成独立艺人名。
/// 拆不出多个时原样返回（单元素）。
pub fn split_artists(name: &str, seps: &[char]) -> Vec<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return vec![name.to_string()];
    }
    // 合作标注（feat. / ft. / featuring，大小写不敏感）统一替换为 ';' 分隔符
    let lower = trimmed.to_lowercase();
    let feat_tokens = ["feat.", "featuring", "feat", "ft."]; // 长词优先，避免 "feat" 抢先命中 "feat."/"featuring" 前缀
    let mut normalized = String::with_capacity(trimmed.len());
    let mut chars = trimmed.char_indices().peekable();
    while let Some(&(pos, _)) = chars.peek() {
        // 仅在词边界处匹配（前一个字符不是字母/数字）；is_char_boundary 防御个别
        // 字符大小写转换后字节长度变化（如 'İ'）导致的切分错位
        let boundary = pos == 0 || !lower[..pos].ends_with(|c: char| c.is_alphanumeric());
        let mut token_chars = 0usize;
        if boundary && lower.is_char_boundary(pos) {
            for tok in feat_tokens {
                if lower[pos..].starts_with(tok) {
                    let after = &lower[pos + tok.len()..];
                    if after.is_empty() || after.starts_with(|c: char| c.is_whitespace()) {
                        token_chars = tok.chars().count();
                        break;
                    }
                }
            }
        }
        if token_chars > 0 {
            normalized.push(';');
            for _ in 0..token_chars {
                chars.next();
            }
        } else {
            let (_, c) = chars.next().unwrap();
            normalized.push(c);
        }
    }

    // 按分隔符拆分 + 清洗 + 去重（大小写不敏感，保持原始顺序）
    let mut parts: Vec<String> = Vec::new();
    for p in normalized.split(seps) {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }
        let key = canonical_artist(&p).to_lowercase();
        if parts.iter().any(|x| x.to_lowercase() == key) {
            continue;
        }
        parts.push(p.to_string());
    }
    if parts.is_empty() {
        vec![trimmed.to_string()]
    } else if parts.len() == 1 {
        // 单元素：可能是拆分前原样（无分隔符），也可能是去重塌缩（"A / a"）——都返回拆分结果
        vec![parts.into_iter().next().unwrap()]
    } else {
        parts
    }
}

#[cfg(test)]
mod split_artists_tests {
    use super::{canonical_artist, parse_separators, split_artists, ARTIST_SEPARATORS};

    #[test]
    fn canonical_strips_trailing_zh_parenthesis() {
        assert_eq!(canonical_artist("陈奕迅（Eason Chan）"), "陈奕迅");
        assert_eq!(
            canonical_artist("陈奕迅（Eason Chan）"),
            canonical_artist("陈奕迅")
        );
    }

    #[test]
    fn canonical_strips_trailing_en_parenthesis() {
        assert_eq!(canonical_artist("Tom (2024)"), "Tom");
        assert_eq!(canonical_artist("Tom(2024)"), "Tom");
    }

    #[test]
    fn canonical_keeps_plain_or_other() {
        assert_eq!(canonical_artist("陈奕迅"), "陈奕迅");
        assert_eq!(canonical_artist("（未知）"), "（未知）");
        assert_eq!(canonical_artist("xxx（feat.）yyy"), "xxx（feat.）yyy");
        // 括号内空白不剥：避免把 "A（ ）" 当成注释
        assert_eq!(canonical_artist("A（ ）"), "A（ ）");
    }

    #[test]
    fn split_dedupes_by_canonical_name() {
        // split_artists 只做拆分与精确去重；规整（canonical_artist）在 get_or_create_artist 阶段生效
        assert_eq!(
            split_artists("A（x） / a", ARTIST_SEPARATORS),
            vec!["A（x）", "a"]
        );
        // 完全相同的名字才会去重
        assert_eq!(split_artists("A / A", ARTIST_SEPARATORS), vec!["A"]);
    }

    #[test]
    fn keeps_single_artist() {
        assert_eq!(split_artists("周杰伦", ARTIST_SEPARATORS), vec!["周杰伦"]);
        assert_eq!(
            split_artists("未知艺人", ARTIST_SEPARATORS),
            vec!["未知艺人"]
        );
    }

    #[test]
    fn splits_common_separators() {
        assert_eq!(
            split_artists("周杰伦 / 费玉清", ARTIST_SEPARATORS),
            vec!["周杰伦", "费玉清"]
        );
        assert_eq!(split_artists("A & B", ARTIST_SEPARATORS), vec!["A", "B"]);
        assert_eq!(
            split_artists("A、B、C", ARTIST_SEPARATORS),
            vec!["A", "B", "C"]
        );
        assert_eq!(split_artists("A；B", ARTIST_SEPARATORS), vec!["A", "B"]);
        assert_eq!(split_artists("A，B", ARTIST_SEPARATORS), vec!["A", "B"]);
    }

    #[test]
    fn splits_featuring_tokens() {
        assert_eq!(
            split_artists("A feat. B", ARTIST_SEPARATORS),
            vec!["A", "B"]
        );
        assert_eq!(
            split_artists("A Feat. B", ARTIST_SEPARATORS),
            vec!["A", "B"]
        );
        assert_eq!(
            split_artists("A featuring B", ARTIST_SEPARATORS),
            vec!["A", "B"]
        );
        assert_eq!(split_artists("A ft. B", ARTIST_SEPARATORS), vec!["A", "B"]);
        // 词中包含 feat 字样的艺人名不应被拆；结尾悬空的 "feat" 视为残留标注，拆掉后清理
        assert_eq!(split_artists("Feature", ARTIST_SEPARATORS), vec!["Feature"]);
        assert_eq!(split_artists("Mo feat", ARTIST_SEPARATORS), vec!["Mo"]);
    }

    #[test]
    fn dedupes_and_trims() {
        assert_eq!(split_artists("A / a", ARTIST_SEPARATORS), vec!["A"]);
        assert_eq!(
            split_artists("  A  /  B  ", ARTIST_SEPARATORS),
            vec!["A", "B"]
        );
    }

    #[test]
    fn empty_falls_back_to_original() {
        assert_eq!(split_artists("", ARTIST_SEPARATORS), vec![""]);
    }

    #[test]
    fn respects_custom_separators() {
        // 关闭 '/'：AC/DC 这类含斜杠的艺人名保持完整
        let seps = parse_separators("&");
        assert_eq!(split_artists("AC/DC & A", &seps), vec!["AC/DC", "A"]);
        assert_eq!(split_artists("AC/DC", &seps), vec!["AC/DC"]);
        // 关闭 '/' 但启用 '、'：仅按启用的分隔符拆
        let seps = parse_separators("、&");
        assert_eq!(split_artists("A、B / C", &seps), vec!["A", "B / C"]);
        // feat. 归一目标 ';' 恒定生效
        assert_eq!(split_artists("A feat. B", &seps), vec!["A", "B"]);
    }

    #[test]
    fn parses_separator_setting() {
        // 只保留候选字符、去重
        assert_eq!(parse_separators("a&/x&,"), vec![';', '&', '/', ',']);
        // 空串回退默认全集
        assert_eq!(parse_separators(""), ARTIST_SEPARATORS.to_vec());
        // 全部停用也至少保留 ';'（feat. 归一目标）
        assert_eq!(parse_separators("、&"), vec![';', '、', '&']);
    }
}

#[cfg(test)]
mod skip_dirs_tests {
    use super::{is_skipped_dir, load_skip_dirs, BUILTIN_SKIP_DIRS, SKIP_DIRS_KEY};
    use crate::db;
    use std::collections::HashSet;

    fn builtin_set() -> HashSet<String> {
        BUILTIN_SKIP_DIRS.iter().map(|s| s.to_lowercase()).collect()
    }

    #[test]
    fn builtin_dirs_are_skipped_case_insensitively() {
        let skip = builtin_set();
        assert!(is_skipped_dir(std::ffi::OsStr::new("#recycle"), &skip));
        assert!(is_skipped_dir(std::ffi::OsStr::new("#RECYCLE"), &skip));
        assert!(is_skipped_dir(std::ffi::OsStr::new("@eaDir"), &skip));
        assert!(is_skipped_dir(std::ffi::OsStr::new("$RECYCLE.BIN"), &skip));
        // 目录名只是包含关键字不算命中（匹配的是整个目录名）
        assert!(!is_skipped_dir(std::ffi::OsStr::new("recycle"), &skip));
        assert!(!is_skipped_dir(std::ffi::OsStr::new("我的音乐"), &skip));
    }

    #[test]
    fn trash_prefix_variants_are_skipped() {
        let skip = builtin_set();
        assert!(is_skipped_dir(std::ffi::OsStr::new(".Trash-0"), &skip));
        assert!(is_skipped_dir(std::ffi::OsStr::new(".Trash-1000"), &skip));
        // 纯前缀匹配只放行 .Trash 系列回收站，不影响普通目录
        assert!(!is_skipped_dir(std::ffi::OsStr::new(".Trashy"), &skip));
    }

    #[test]
    fn user_config_merges_with_builtin() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE app_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
            .unwrap();
        db::set_setting(&conn, SKIP_DIRS_KEY, "私人收藏，#tmp\nMy Folder").unwrap();
        let skip = load_skip_dirs(&conn);
        // 逗号 / 换行都可作为分隔符
        assert!(skip.contains("私人收藏"));
        assert!(skip.contains("#tmp"));
        // 目录名可含空格，整段保留并小写化
        assert!(skip.contains("my folder"));
        assert!(is_skipped_dir(std::ffi::OsStr::new("My Folder"), &skip));
        // 内置名单始终生效
        assert!(skip.contains("#recycle"));
    }
}

/// 快速导入：不读文件内容。按常见目录布局（艺人/专辑/曲名）猜测。
fn fast_track(rel: &str, mtime: i64, size: i64, has_mv: bool) -> ParsedTrack {
    let parts: Vec<&str> = rel.split('/').filter(|s| !s.is_empty()).collect();
    let (artist, album_title) = match parts.len() {
        n if n >= 3 => (parts[n - 3].to_string(), parts[n - 2].to_string()),
        2 => ("未知艺人".to_string(), parts[0].to_string()),
        _ => ("未知艺人".to_string(), "未知专辑".to_string()),
    };
    ParsedTrack {
        rel: rel.to_string(),
        title: stem_of(rel),
        artist,
        album_title,
        album_artist: "未知艺人".into(),
        genre: None,
        year: None,
        track_no: None,
        disc_no: None,
        duration: None,
        bitrate: None,
        sample_rate: None,
        channels: None,
        bit_depth: None,
        has_lyrics: false,
        has_mv,
        format: ext_of(rel),
        mtime,
        size,
        meta_state: 0,
        lyrics_text: None,
    }
}

/// 降级行：按文件名入库（艺人/专辑落成「未知」）。`meta_state` 由调用方决定：
/// - 1 = 文件本身读不出标签（本地解析失败、远端拿到字节但解析失败）→ 不再重试
/// - 0 = 远端一个字节都没拿到（限流/超时等瞬时故障）→ 留给后续扫描或「完整解析」重试
fn fallback_track(rel: &str, mtime: i64, size: i64, has_mv: bool, meta_state: i64) -> ParsedTrack {
    ParsedTrack {
        rel: rel.to_string(),
        title: stem_of(rel),
        artist: "未知艺人".into(),
        album_title: "未知专辑".into(),
        album_artist: "未知艺人".into(),
        genre: None,
        year: None,
        track_no: None,
        disc_no: None,
        duration: None,
        bitrate: None,
        sample_rate: None,
        channels: None,
        bit_depth: None,
        has_lyrics: false,
        has_mv,
        format: ext_of(rel),
        mtime,
        size,
        meta_state,
        lyrics_text: None,
    }
}

fn parsed_from_meta(
    rel: &str,
    meta: TrackMeta,
    mtime: i64,
    size: i64,
    has_mv: bool,
) -> ParsedTrack {
    let artist = meta
        .artist
        .clone()
        .or_else(|| meta.album_artist.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "未知艺人".into());
    let album_title = meta
        .album
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "未知专辑".into());
    let album_artist = meta
        .album_artist
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| artist.clone());
    ParsedTrack {
        rel: rel.to_string(),
        title: meta
            .title
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| stem_of(rel)),
        artist,
        album_title,
        album_artist,
        genre: meta.genre,
        year: meta.year,
        track_no: meta.track_no,
        disc_no: meta.disc_no,
        duration: meta.duration,
        bitrate: meta.bitrate,
        sample_rate: meta.sample_rate,
        channels: meta.channels,
        bit_depth: meta.bit_depth,
        has_lyrics: meta.has_lyrics,
        has_mv,
        format: ext_of(rel),
        mtime,
        size,
        meta_state: 1,
        lyrics_text: meta.lyrics,
    }
}
