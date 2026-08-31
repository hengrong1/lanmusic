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
use crate::network::{self, webdav};
use crate::state::AppState;

const AUDIO_EXTS: &[&str] = &[
    "mp3", "flac", "m4a", "aac", "ogg", "oga", "opus", "wav", "aif", "aiff", "wma", "ape",
];
/// 每这么多首提交一次事务并上报一次进度
const BATCH: usize = 100;
/// 并发解析线程上限（I/O 密集，8 线程已能掩盖网络延迟且不至于压垮网络）
const MAX_WORKERS: usize = 8;

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
    format: Option<String>,
    mtime: i64,
    size: i64,
    /// 0 = 快速导入（仅文件名，待补全解析），1 = 完整解析
    meta_state: i64,
    /// LAN 共享源的远端曲目 id
    remote_id: Option<i64>,
    /// 远端确认存在歌词（lan）
    has_lrc: bool,
}

/// 在后台线程中调用（见 commands::add_local_source / rescan_source 等）
pub fn scan_source(app: AppHandle, source_id: i64, full_rescan: bool) {
    let started = Instant::now();
    let result = load_source(&app, source_id).and_then(|(kind, base_path, base_url, config)| {
        match kind.as_str() {
            "local" => run_local_scan(
                &app,
                source_id,
                PathBuf::from(base_path.unwrap_or_default()),
                full_rescan,
            ),
            "webdav" => run_webdav_scan(&app, source_id, base_url, config, full_rescan),
            "lan" => run_lan_scan(&app, source_id, base_url, config, full_rescan),
            other => Err(format!("未知来源类型：{other}")),
        }
    });

    let state = app.state::<AppState>();
    state.scanning.lock().unwrap().remove(&source_id);

    match result {
        Ok((added, updated, removed)) => {
            let _ = app.emit(
                "scan:done",
                ScanDone {
                    source_id,
                    added,
                    updated,
                    removed,
                    ms: started.elapsed().as_millis(),
                },
            );
        }
        Err(message) => {
            let _ = app.emit("scan:error", ScanError { source_id, message });
        }
    }
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
        ScanProgress { source_id, phase: "enumerate".into(), done, total: 0, current },
    );
}

fn emit_parse(app: &AppHandle, source_id: i64, done: usize, total: usize) {
    let _ = app.emit(
        "scan:progress",
        ScanProgress { source_id, phase: "parse".into(), done, total, current: String::new() },
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
    let fast_import: bool = conn
        .query_row("SELECT fast_import FROM sources WHERE id = ?1", [source_id], |r| r.get(0))
        .map_err(|e| e.to_string())?;

    // ---- 1. 枚举目录 + 收集 .lrc ----
    let mut files: Vec<(String, i64, i64)> = Vec::new();
    let mut lrc_map: HashMap<String, String> = HashMap::new(); // rel 去扩展名 → 本地 .lrc 绝对路径
    for entry in WalkDir::new(&base).follow_links(false).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let Some(ext) = entry.path().extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase())
        else {
            continue;
        };
        let rel = rel_path(&base, entry.path());
        if ext == "lrc" {
            lrc_map.insert(stem_key(&rel), base.join(&rel).to_string_lossy().to_string());
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
        if files.len() % 500 == 0 && files.len() > 0 {
            emit_enumerate(app, source_id, files.len(), file_name_of(&rel));
        }
        files.push((rel, mtime, size));
    }
    let total = files.len();
    emit_parse(app, source_id, 0, total);

    // ---- 2. diff ----
    let existing = load_existing(&conn, source_id)?;
    let to_parse: Vec<(String, i64, i64)> = files
        .iter()
        .filter(|(p, mtime, size)| needs_parse(&existing, p, *mtime, *size, full_rescan, fast_import))
        .cloned()
        .collect();

    // ---- 3. 并发解析 ----
    let base_c = base.clone();
    let rx = run_concurrent(to_parse, move |(rel, mtime, size)| {
        if fast_import {
            return Some(fast_track(&rel, mtime, size));
        }
        let full = base_c.join(&rel);
        match metadata::read(&full, false) {
            Ok(m) => Some(parsed_from_meta(&rel, m, mtime, size)),
            Err(_) => Some(fallback_track(&rel, mtime, size)),
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
            let Some(target) = lrc_map.get(&stem_key(rel)) else { continue };
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
    conn.execute("UPDATE sources SET last_scan_at = ?1 WHERE id = ?2", params![now, source_id])
        .map_err(|e| e.to_string())?;

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
    let Some(base_str) = base_url else { return Err("WebDAV 地址缺失".into()) };
    let base = webdav::normalize_base(&base_str)?;
    let auth = webdav::Auth::from_config(config.as_deref());
    let base_path = decoded_url_path(&base).trim_end_matches('/').to_string();

    // ---- 1. PROPFIND 广度遍历 ----
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
                let child = base.join(&format!("{rel}/")).unwrap_or_else(|_| base.clone());
                queue.push(child);
            } else {
                let ext = ext_of(&rel);
                let name = file_name_of(&rel).to_ascii_lowercase();
                if ext.as_deref() == Some("lrc") {
                    lrc_map.insert(stem_key(&rel), webdav::file_url(&base, &rel).to_string());
                } else if COVER_NAMES.contains(&name.as_str()) {
                    cover_map.insert(parent_dir(&rel), webdav::file_url(&base, &rel).to_string());
                } else if AUDIO_EXTS.contains(&ext.as_deref().unwrap_or("")) {
                    files.push((rel, item.size));
                }
            }
        }
        emit_enumerate(app, source_id, files.len(), file_name_of(&decoded_url_path(&dir)));
    }
    let total = files.len();
    emit_parse(app, source_id, 0, total);

    // ---- 2. diff（mtime 不可靠，仅按 size + meta_state）----
    let existing = {
        let state = app.state::<AppState>();
        let conn = db::open_conn(&state.db_path, false).map_err(|e| e.to_string())?;
        load_existing(&conn, source_id)?
    };
    let to_parse: Vec<(String, i64)> = files
        .iter()
        .filter(|(p, size)| needs_parse(&existing, p, 0, *size, full_rescan, false))
        .cloned()
        .collect();

    // ---- 3. 并发拉取头部字节并解析标签 ----
    let base_c = base.clone();
    let auth_c = auth.clone();
    let rx = run_concurrent(to_parse, move |(rel, size)| {
        let url = webdav::file_url(&base_c, &rel);
        match webdav::download(&url, auth_c.as_ref(), Some((0, HEAD_FETCH_SIZE - 1))) {
            Ok(bytes) => match metadata::read_bytes(&bytes, false) {
                Ok(m) => Some(parsed_from_meta(&rel, m, 0, size)),
                Err(_) => Some(fallback_track(&rel, 0, size)),
            },
            Err(_) => Some(fallback_track(&rel, 0, size)),
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
    conn.execute("UPDATE sources SET last_scan_at = ?1 WHERE id = ?2", params![now, source_id])
        .map_err(|e| e.to_string())?;

    Ok((added, updated, removed))
}

// ================================================================ LAN 扫描

fn run_lan_scan(
    app: &AppHandle,
    source_id: i64,
    base_url: Option<String>,
    config: Option<String>,
    _full_rescan: bool,
) -> Result<(usize, usize, usize), String> {
    let Some(base) = base_url else { return Err("设备地址缺失".into()) };
    let base = base.trim_end_matches('/').to_string();
    let token = network::config_field(config.as_deref(), "token").unwrap_or_default();

    // ---- 1. 分页拉取全部元数据 ----
    let mut all: Vec<network::RemoteTrack> = Vec::new();
    let mut offset = 0i64;
    loop {
        let page = network::lan::tracks_page(&base, &token, offset, 500)
            .map_err(|e| format!("拉取设备曲库失败：{e}"))?;
        let n = page.len() as i64;
        all.extend(page);
        emit_enumerate(app, source_id, all.len(), String::new());
        if n < 500 || all.len() > 500_000 {
            break;
        }
        offset += n;
    }
    let total = all.len();
    emit_parse(app, source_id, 0, total);

    // ---- 2. 转 ParsedTrack（path = rt/{remote_id}），全量 upsert ----
    let rows: Vec<ParsedTrack> = all
        .iter()
        .map(|rt| ParsedTrack {
            rel: format!("rt/{}", rt.id),
            title: rt.title.clone(),
            artist: rt.artist.clone().unwrap_or_else(|| "未知艺人".into()),
            album_title: rt.album.clone().unwrap_or_else(|| "未知专辑".into()),
            album_artist: rt
                .album_artist
                .clone()
                .or_else(|| rt.artist.clone())
                .unwrap_or_else(|| "未知艺人".into()),
            genre: None,
            year: rt.year,
            track_no: rt.track_no,
            disc_no: None,
            duration: rt.duration,
            bitrate: None,
            sample_rate: None,
            channels: None,
            bit_depth: None,
            has_lyrics: false,
            format: rt.format.clone(),
            mtime: 0,
            size: 0,
            meta_state: 1,
            remote_id: Some(rt.id),
            has_lrc: rt.has_lrc,
        })
        .collect();

    let existing = {
        let state = app.state::<AppState>();
        let conn = db::open_conn(&state.db_path, false).map_err(|e| e.to_string())?;
        load_existing(&conn, source_id)?
    };
    let state = app.state::<AppState>();
    let mut conn = db::open_conn(&state.db_path, false).map_err(|e| e.to_string())?;
    let (added, updated) = consume_and_write(&mut conn, app, source_id, rows.into_iter(), &existing, None, None, total)?;

    // ---- 3. 删除 + 扫描时间 ----
    let walked: HashSet<String> = all.iter().map(|rt| format!("rt/{}", rt.id)).collect();
    let removed = delete_missing(app, &mut conn, source_id, &existing, &walked)?;
    let now = now_secs();
    conn.execute("UPDATE sources SET last_scan_at = ?1 WHERE id = ?2", params![now, source_id])
        .map_err(|e| e.to_string())?;

    // ---- 4. 下载缺失的远程封面 ----
    let state = app.state::<AppState>();
    let covers_dir = state.covers_dir.clone();
    let mut cover_count = 0usize;
    {
        let mut stmt = conn
            .prepare(
                "SELECT al.id, al.remote_id FROM albums al
                 WHERE al.remote_id IS NOT NULL
                   AND al.id IN (SELECT DISTINCT album_id FROM tracks WHERE source_id = ?1)",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([source_id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
            .map_err(|e| e.to_string())?;
        for row in rows.flatten() {
            let (album_id, remote_id) = row;
            let jpg = covers_dir.join(format!("{album_id}.jpg"));
            let none = covers_dir.join(format!("{album_id}.none"));
            if jpg.is_file() || none.is_file() {
                continue;
            }
            if let Ok(bytes) = network::lan::download_cover(&base, &token, remote_id) {
                if crate::covers::save_cover(&covers_dir, album_id, &bytes).is_ok() {
                    let _ = conn.execute("UPDATE albums SET has_cover = 1 WHERE id = ?1", [album_id]);
                    cover_count += 1;
                }
            }
        }
    }

    Ok((added, updated, removed + cover_count))
}

// ================================================================ 共用写入管线

struct ScanCaches {
    artists: HashMap<String, i64>,
    /// key → (本地专辑 id, 远端专辑 id)
    albums: HashMap<String, (i64, Option<i64>)>,
}

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64
}

fn load_caches(conn: &rusqlite::Connection) -> Result<ScanCaches, String> {
    let mut artists = HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT id, name FROM artists").map_err(|e| e.to_string())?;
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
        let mut stmt = conn.prepare("SELECT id, key, remote_id FROM albums").map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<i64>>(2)?))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (id, key, rid) = row.map_err(|e| e.to_string())?;
            albums.insert(key, (id, rid));
        }
    }
    Ok(ScanCaches { artists, albums })
}

fn load_existing(conn: &rusqlite::Connection, source_id: i64) -> Result<HashMap<String, (i64, i64, i64)>, String> {
    let mut map = HashMap::new();
    let mut stmt = conn
        .prepare("SELECT path, IFNULL(mtime,0), IFNULL(file_size,0), meta_state FROM tracks WHERE source_id = ?1")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([source_id], |r| {
            Ok((r.get::<_, String>(0)?, (r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?)))
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
            write_batch(conn, &mut caches, source_id, now, &mut batch, existing, &covers_dir, lrc_map, cover_map)?;
            emit_parse(app, source_id, done, total);
        }
    }
    write_batch(conn, &mut caches, source_id, now, &mut batch, existing, &covers_dir, lrc_map, cover_map)?;
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
) -> Result<(), String> {
    if batch.is_empty() {
        return Ok(());
    }
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    for row in batch.drain(..) {
        let artist_id = get_or_create_artist(&tx, &mut caches.artists, &row.artist)?;

        let album_key = format!(
            "{}|{}|{}",
            row.album_title.to_lowercase(),
            row.album_artist.to_lowercase(),
            row.year.unwrap_or(0)
        );
        let album_id = match caches.albums.get(&album_key) {
            Some((id, _)) => *id,
            None => {
                tx.execute(
                    "INSERT INTO albums (title, artist_id, year, key, remote_id) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![row.album_title, artist_id, row.year, album_key, row.remote_id],
                )
                .map_err(|e| e.to_string())?;
                let id = tx.last_insert_rowid();
                caches.albums.insert(album_key, (id, row.remote_id));
                id
            }
        };

        tx.execute(
            "INSERT INTO tracks (source_id, path, title, artist_id, album_id, genre, track_no, disc_no,
                                  year, duration, bitrate, sample_rate, channels, bit_depth,
                                  has_embedded_lyrics, mtime, file_size, format, added_at, meta_state, remote_id)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)
             ON CONFLICT(source_id, path) DO UPDATE SET
                title=excluded.title, artist_id=excluded.artist_id, album_id=excluded.album_id,
                genre=excluded.genre, track_no=excluded.track_no, disc_no=excluded.disc_no,
                year=excluded.year, duration=excluded.duration, bitrate=excluded.bitrate,
                sample_rate=excluded.sample_rate, channels=excluded.channels, bit_depth=excluded.bit_depth,
                has_embedded_lyrics=excluded.has_embedded_lyrics, mtime=excluded.mtime,
                file_size=excluded.file_size, format=excluded.format, meta_state=excluded.meta_state,
                remote_id=excluded.remote_id",
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
                row.mtime,
                row.size,
                row.format,
                now,
                row.meta_state,
                row.remote_id,
            ],
        )
        .map_err(|e| e.to_string())?;
        let track_id = tx.last_insert_rowid();

        // 歌词关联：lan 用 NULL 标记（播放时走远程接口）；local/webdav 存路径或 URL
        let lrc_target: Option<String> = if row.has_lrc {
            Some(String::new())
        } else {
            lrc_map.and_then(|m| m.get(&stem_key(&row.rel)).cloned())
        };
        if let Some(target) = lrc_target {
            let p = if target.is_empty() { None } else { Some(target) };
            tx.execute(
                "INSERT INTO lrc_files (track_id, path) VALUES (?1, ?2)
                 ON CONFLICT(track_id) DO UPDATE SET path = excluded.path",
                params![track_id, p],
            )
            .map_err(|e| e.to_string())?;
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

fn get_or_create_artist(
    tx: &rusqlite::Transaction,
    cache: &mut HashMap<String, i64>,
    name: &str,
) -> Result<i64, String> {
    let key = name.to_lowercase();
    if let Some(id) = cache.get(&key) {
        return Ok(*id);
    }
    tx.execute("INSERT OR IGNORE INTO artists (name) VALUES (?1)", [name]).map_err(|e| e.to_string())?;
    let id: i64 = tx
        .query_row("SELECT id FROM artists WHERE name = ?1 COLLATE NOCASE", [name], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    cache.insert(key, id);
    Ok(id)
}

fn delete_missing(
    app: &AppHandle,
    conn: &mut rusqlite::Connection,
    source_id: i64,
    existing: &HashMap<String, (i64, i64, i64)>,
    walked: &HashSet<String>,
) -> Result<usize, String> {
    let to_remove: Vec<String> = existing.keys().filter(|p| !walked.contains(*p)).cloned().collect();
    let mut removed = 0usize;
    for chunk in to_remove.chunks(500) {
        let tx = conn.transaction().map_err(|e| e.to_string())?;
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
                .prepare("SELECT id FROM albums WHERE id NOT IN (SELECT DISTINCT album_id FROM tracks)")
                .map_err(|e| e.to_string())?;
            let rows = stmt.query_map([], |r| r.get::<_, i64>(0)).map_err(|e| e.to_string())?;
            rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?
        };
        tx.execute("DELETE FROM albums WHERE id NOT IN (SELECT DISTINCT album_id FROM tracks)", [])
            .map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM artists WHERE id NOT IN (SELECT DISTINCT artist_id FROM tracks)", [])
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        if !orphan_albums.is_empty() {
            crate::covers::purge(&app.state::<AppState>().covers_dir, &orphan_albums);
        }
    }
    Ok(removed)
}

// ================================================================ 并发工作线程

/// 将 items 均分后在多个线程中并发执行 f，结果经 channel 返回主线程
fn run_concurrent<TIn, TOut>(
    mut items: Vec<TIn>,
    f: impl Fn(TIn) -> Option<TOut> + Send + Sync + Clone + 'static,
) -> mpsc::Receiver<TOut>
where
    TIn: Send + 'static,
    TOut: Send + 'static,
{
    let (tx, rx) = mpsc::channel::<TOut>();
    let workers = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(2, MAX_WORKERS);
    let chunk_size = items.len().div_ceil(workers).max(1);
    for _ in 0..workers {
        let take = chunk_size.min(items.len());
        if take == 0 {
            break;
        }
        let part: Vec<TIn> = items.drain(..take).collect();
        let tx = tx.clone();
        let f = f.clone();
        thread::spawn(move || {
            for item in part {
                if let Some(out) = f(item) {
                    if tx.send(out).is_err() {
                        break;
                    }
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
    percent_encoding::percent_decode_str(u.path()).decode_utf8_lossy().into_owned()
}

fn ext_of(rel: &str) -> Option<String> {
    Path::new(rel).extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase())
}

fn stem_of(rel: &str) -> String {
    Path::new(rel)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| rel.to_string())
}

/// "A/B/song.flac" → "A/B/song"
fn stem_key(rel: &str) -> String {
    Path::new(rel).with_extension("").to_string_lossy().replace('\\', "/")
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

/// 快速导入：不读文件内容。按常见目录布局（艺人/专辑/曲名）猜测。
fn fast_track(rel: &str, mtime: i64, size: i64) -> ParsedTrack {
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
        format: ext_of(rel),
        mtime,
        size,
        meta_state: 0,
        remote_id: None,
        has_lrc: false,
    }
}

/// 标签解析失败时的降级行（meta_state=1：标记已尝试，避免每次重扫重试；「完整解析」可再试）
fn fallback_track(rel: &str, mtime: i64, size: i64) -> ParsedTrack {
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
        format: ext_of(rel),
        mtime,
        size,
        meta_state: 1,
        remote_id: None,
        has_lrc: false,
    }
}

fn parsed_from_meta(rel: &str, meta: TrackMeta, mtime: i64, size: i64) -> ParsedTrack {
    let artist = meta
        .artist
        .clone()
        .or_else(|| meta.album_artist.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "未知艺人".into());
    let album_title = meta.album.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| "未知专辑".into());
    let album_artist = meta.album_artist.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| artist.clone());
    ParsedTrack {
        rel: rel.to_string(),
        title: meta.title.filter(|s| !s.is_empty()).unwrap_or_else(|| stem_of(rel)),
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
        format: ext_of(rel),
        mtime,
        size,
        meta_state: 1,
        remote_id: None,
        has_lrc: false,
    }
}
