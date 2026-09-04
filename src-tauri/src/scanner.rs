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

    // ---- 1. 枚举目录 + 收集 .lrc + 检测视频文件 ----
    let mut files: Vec<(String, i64, i64)> = Vec::new();
    let mut lrc_map: HashMap<String, String> = HashMap::new(); // rel 去扩展名 → 本地 .lrc 绝对路径
    let mut video_stems: HashSet<String> = HashSet::new(); // 视频文件的 stem_key 集合
    let mut seen = 0usize; // 遍历的文件总数（含非音频文件，进度按此上报更平滑）
    for entry in WalkDir::new(&base).follow_links(false).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = rel_path(&base, entry.path());
        seen += 1;
        if seen.is_multiple_of(1000) {
            emit_enumerate(app, source_id, files.len(), file_name_of(&rel));
        }
        let Some(ext) = entry.path().extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase())
        else {
            continue;
        };
        if ext == "lrc" {
            lrc_map.insert(stem_key(&rel), base.join(&rel).to_string_lossy().to_string());
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
        .filter(|(p, mtime, size)| needs_parse(&existing, p, *mtime, *size, full_rescan, fast_import))
        .cloned()
        .collect();

    // ---- 3. 并发解析 ----
    let base_c = base.clone();
    let video_stems_c = video_stems.clone();
    let rx = run_concurrent(to_parse, move |(rel, mtime, size)| {
        let has_mv = video_stems_c.contains(&stem_key(&rel));
        if fast_import {
            return Some(fast_track(&rel, mtime, size, has_mv));
        }
        let full = base_c.join(&rel);
        match metadata::read(&full, false) {
            Ok(m) => Some(parsed_from_meta(&rel, m, mtime, size, has_mv)),
            Err(_) => Some(fallback_track(&rel, mtime, size, has_mv)),
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
    let auth = webdav::Auth::from_source(config.as_deref(), source_id);
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
                Ok(m) => Some(parsed_from_meta(&rel, m, 0, size, false)),
                Err(_) => Some(fallback_track(&rel, 0, size, false)),
            },
            Err(_) => Some(fallback_track(&rel, 0, size, false)),
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

// ================================================================ 共用写入管线

struct ScanCaches {
    artists: HashMap<String, i64>,
    /// key → 本地专辑 id
    albums: HashMap<String, i64>,
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
        let mut stmt = conn.prepare("SELECT id, key FROM albums").map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (id, key) = row.map_err(|e| e.to_string())?;
            albums.insert(key, id);
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
        // 多艺人拆分："A / B" → [A, B]，首个作为主艺人（tracks.artist_id，兼容旧查询/排序）
        let artist_ids = split_artists(&row.artist)
            .iter()
            .map(|n| get_or_create_artist(&tx, &mut caches.artists, n))
            .collect::<Result<Vec<i64>, String>>()?;
        let artist_id = artist_ids.first().copied().unwrap_or(0);
        // 专辑归属按合辑艺人（album_artist）入库，艺人/专辑归类才与标签语义一致
        let album_artist_id = get_or_create_artist(&tx, &mut caches.artists, &row.album_artist)?;

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
                                  has_embedded_lyrics, has_mv, mtime, file_size, format, added_at, meta_state)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)
             ON CONFLICT(source_id, path) DO UPDATE SET
                title=excluded.title, artist_id=excluded.artist_id, album_id=excluded.album_id,
                genre=excluded.genre, track_no=excluded.track_no, disc_no=excluded.disc_no,
                year=excluded.year, duration=excluded.duration, bitrate=excluded.bitrate,
                sample_rate=excluded.sample_rate, channels=excluded.channels, bit_depth=excluded.bit_depth,
                has_embedded_lyrics=excluded.has_embedded_lyrics, has_mv=excluded.has_mv,
                mtime=excluded.mtime, file_size=excluded.file_size, format=excluded.format, meta_state=excluded.meta_state",
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
            ],
        )
        .map_err(|e| e.to_string())?;
        let track_id = tx.last_insert_rowid();

        // 多艺人关联：先清后插保证重复扫描幂等（更新行会重建关联）
        tx.execute("DELETE FROM track_artists WHERE track_id = ?1", params![track_id])
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
    Ok(removed)
}

// ================================================================ 并发工作线程

/// 多线程并发执行 f：任务逐条从共享队列领取（避免静态均分导致大文件集中在
/// 单个分片时拖尾），结果经 channel 返回主线程
fn run_concurrent<TIn, TOut>(
    items: Vec<TIn>,
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
        .clamp(2, MAX_WORKERS);
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

/// 多艺人分隔符（含中文标点与常见合作标注；拆分前 feat./ft./featuring 会先归一为 ';'）
const ARTIST_SEPARATORS: &[char] = &[';', '；', '、', '&', '，', ',', '/'];

/// 把 "A / B"、"A & B"、"A feat. B" 这类多艺人字符串拆成独立艺人名。
/// 拆不出多个时原样返回（单元素）。
fn split_artists(name: &str) -> Vec<String> {
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
    for p in normalized.split(ARTIST_SEPARATORS) {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }
        let key = p.to_lowercase();
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
    use super::split_artists;

    #[test]
    fn keeps_single_artist() {
        assert_eq!(split_artists("周杰伦"), vec!["周杰伦"]);
        assert_eq!(split_artists("未知艺人"), vec!["未知艺人"]);
    }

    #[test]
    fn splits_common_separators() {
        assert_eq!(split_artists("周杰伦 / 费玉清"), vec!["周杰伦", "费玉清"]);
        assert_eq!(split_artists("A & B"), vec!["A", "B"]);
        assert_eq!(split_artists("A、B、C"), vec!["A", "B", "C"]);
        assert_eq!(split_artists("A；B"), vec!["A", "B"]);
        assert_eq!(split_artists("A，B"), vec!["A", "B"]);
    }

    #[test]
    fn splits_featuring_tokens() {
        assert_eq!(split_artists("A feat. B"), vec!["A", "B"]);
        assert_eq!(split_artists("A Feat. B"), vec!["A", "B"]);
        assert_eq!(split_artists("A featuring B"), vec!["A", "B"]);
        assert_eq!(split_artists("A ft. B"), vec!["A", "B"]);
        // 词中包含 feat 字样的艺人名不应被拆；结尾悬空的 "feat" 视为残留标注，拆掉后清理
        assert_eq!(split_artists("Feature"), vec!["Feature"]);
        assert_eq!(split_artists("Mo feat"), vec!["Mo"]);
    }

    #[test]
    fn dedupes_and_trims() {
        assert_eq!(split_artists("A / a"), vec!["A"]);
        assert_eq!(split_artists("  A  /  B  "), vec!["A", "B"]);
    }

    #[test]
    fn empty_falls_back_to_original() {
        assert_eq!(split_artists(""), vec![""]);
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
    }
}

/// 标签解析失败时的降级行（meta_state=1：标记已尝试，避免每次重扫重试；「完整解析」可再试）
fn fallback_track(rel: &str, mtime: i64, size: i64, has_mv: bool) -> ParsedTrack {
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
        meta_state: 1,
    }
}

fn parsed_from_meta(rel: &str, meta: TrackMeta, mtime: i64, size: i64, has_mv: bool) -> ParsedTrack {
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
        has_mv,
        format: ext_of(rel),
        mtime,
        size,
        meta_state: 1,
    }
}
