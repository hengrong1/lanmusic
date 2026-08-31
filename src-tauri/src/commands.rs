//! IPC 命令层：薄封装，参数校验后操作数据库 / 触发扫描。

use rusqlite::{params, params_from_iter, OptionalExtension, ToSql};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::db;
use crate::scanner;
use crate::state::AppState;

// ---------- DTO ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub base_path: Option<String>,
    pub base_url: Option<String>,
    pub enabled: bool,
    pub last_scan_at: Option<i64>,
    pub track_count: i64,
    pub fast_import: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: i64,
    pub title: String,
    pub artist: Option<String>,
    pub artist_id: Option<i64>,
    pub album: Option<String>,
    pub album_id: Option<i64>,
    pub track_no: Option<i64>,
    pub disc_no: Option<i64>,
    pub duration: Option<f64>,
    pub bitrate: Option<i64>,
    pub sample_rate: Option<i64>,
    pub bit_depth: Option<i64>,
    pub format: Option<String>,
    pub path: String,
    pub has_lyrics: bool,
    pub fav: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub total: i64,
    pub items: Vec<T>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AlbumItem {
    pub id: i64,
    pub title: String,
    pub artist: Option<String>,
    pub year: Option<i64>,
    pub has_cover: bool,
    pub track_count: i64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArtistItem {
    pub id: i64,
    pub name: String,
    pub track_count: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryStats {
    pub tracks: i64,
    pub albums: i64,
    pub artists: i64,
    pub favorites: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackQuery {
    pub view: Option<String>,
    pub ref_id: Option<i64>,
    pub search: Option<String>,
    pub sort: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

// ---------- 行映射 ----------

const TRACK_SELECT: &str = "SELECT t.id, t.title, a.name, t.artist_id, al.title, t.album_id, t.track_no, \
     t.disc_no, t.duration, t.bitrate, t.sample_rate, t.bit_depth, t.format, t.path, t.has_embedded_lyrics, t.fav \
     FROM tracks t \
     LEFT JOIN artists a ON a.id = t.artist_id \
     LEFT JOIN albums al ON al.id = t.album_id";

fn row_track(r: &rusqlite::Row) -> rusqlite::Result<Track> {
    Ok(Track {
        id: r.get(0)?,
        title: r.get(1)?,
        artist: r.get(2)?,
        artist_id: r.get(3)?,
        album: r.get(4)?,
        album_id: r.get(5)?,
        track_no: r.get(6)?,
        disc_no: r.get(7)?,
        duration: r.get(8)?,
        bitrate: r.get(9)?,
        sample_rate: r.get(10)?,
        bit_depth: r.get(11)?,
        format: r.get(12)?,
        path: r.get(13)?,
        has_lyrics: r.get::<_, i64>(14)? != 0,
        fav: r.get::<_, i64>(15)? != 0,
    })
}

const SOURCE_SELECT: &str = "SELECT s.id, s.kind, s.name, s.base_path, s.base_url, s.enabled, s.last_scan_at, s.fast_import, \
     (SELECT COUNT(*) FROM tracks t WHERE t.source_id = s.id) FROM sources s";

fn row_source(r: &rusqlite::Row) -> rusqlite::Result<Source> {
    Ok(Source {
        id: r.get(0)?,
        kind: r.get(1)?,
        name: r.get(2)?,
        base_path: r.get(3)?,
        base_url: r.get(4)?,
        enabled: r.get::<_, i64>(5)? != 0,
        last_scan_at: r.get(6)?,
        fast_import: r.get::<_, i64>(7)? != 0,
        track_count: r.get(8)?,
    })
}

// ---------- 来源管理 ----------

fn spawn_scan(app: &AppHandle, state: &AppState, id: i64, full_rescan: bool) -> Result<(), String> {
    let mut scanning = state.scanning.lock().map_err(|e| e.to_string())?;
    if !scanning.insert(id) {
        return Err("该来源正在扫描中".into());
    }
    drop(scanning);
    let app2 = app.clone();
    std::thread::spawn(move || scanner::scan_source(app2, id, full_rescan));
    Ok(())
}

#[tauri::command]
pub fn add_local_source(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<Source, String> {
    let p = std::path::PathBuf::from(&path);
    if !p.is_dir() {
        return Err("目录不存在".into());
    }
    let name = p
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.clone());

    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let exists: Option<i64> = conn
        .query_row(
            "SELECT id FROM sources WHERE kind = 'local' AND base_path = ?1",
            params![path],
            |r| r.get(0),
        )
        .ok();
    if exists.is_some() {
        return Err("该文件夹已在音乐库中".into());
    }
    conn.execute("INSERT INTO sources (kind, name, base_path) VALUES ('local', ?1, ?2)", params![name, path])
        .map_err(|e| e.to_string())?;
    let id = conn.last_insert_rowid();
    drop(conn);

    spawn_scan(&app, &state, id, false)?;

    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.query_row(&format!("{SOURCE_SELECT} WHERE s.id = ?1"), params![id], row_source)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_sources(state: State<'_, AppState>) -> Result<Vec<Source>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(&format!("{SOURCE_SELECT} ORDER BY s.id")).map_err(|e| e.to_string())?;
    let items = stmt
        .query_map([], row_source)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(items)
}

#[tauri::command]
pub fn remove_source(app: AppHandle, state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let state2 = app.state::<AppState>();
    if state2.scanning.lock().map_err(|e| e.to_string())?.contains(&id) {
        return Err("该来源正在扫描中，请稍后再移除".into());
    }
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    // 先收集将被删除的孤儿专辑 id，删除后同步清理封面缓存文件
    // （否则 SQLite rowid 复用会让新专辑命中旧封面 → 歌和封面对不上）
    let orphan_albums: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT id FROM albums WHERE id NOT IN (SELECT DISTINCT album_id FROM tracks)")
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |r| r.get::<_, i64>(0)).map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?
    };
    conn.execute("DELETE FROM sources WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM albums WHERE id NOT IN (SELECT DISTINCT album_id FROM tracks)", [])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM artists WHERE id NOT IN (SELECT DISTINCT artist_id FROM tracks)", [])
        .map_err(|e| e.to_string())?;
    drop(conn);
    crate::covers::purge(&state.covers_dir, &orphan_albums);
    Ok(())
}

/// mode: "auto" = 增量（新文件/变化文件/快速导入未解析的行）；"full" = 全部重新解析
#[tauri::command]
pub fn rescan_source(app: AppHandle, state: State<'_, AppState>, id: i64, mode: Option<String>) -> Result<(), String> {
    let full = mode.as_deref() == Some("full");
    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let exists: Option<i64> = conn
            .query_row("SELECT id FROM sources WHERE id = ?1", params![id], |r| r.get(0))
            .ok();
        if exists.is_none() {
            return Err("来源不存在".into());
        }
    }
    spawn_scan(&app, &state, id, full)
}

/// 开关快速导入：开启后扫描只按文件名/目录结构入库（不读文件内容），适合慢速网络目录
#[tauri::command]
pub fn set_source_fast_import(state: State<'_, AppState>, id: i64, enabled: bool) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE sources SET fast_import = ?1 WHERE id = ?2 AND kind = 'local'",
        params![enabled as i64, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- 库查询 ----------

/// 执行分页查询并收集行结果（可选 LIKE 过滤参数）
fn collect_rows<T>(
    conn: &rusqlite::Connection,
    sql: &str,
    like: Option<&String>,
    map: impl Fn(&rusqlite::Row) -> rusqlite::Result<T>,
) -> Result<Vec<T>, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let mut rows = match like {
        Some(l) => stmt.query_map(params![l], map).map_err(|e| e.to_string())?,
        None => stmt.query_map([], map).map_err(|e| e.to_string())?,
    };
    let mut out = Vec::new();
    while let Some(r) = rows.next() {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

#[tauri::command]
pub fn query_tracks(state: State<'_, AppState>, q: TrackQuery) -> Result<Page<Track>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    let mut wheres: Vec<String> = Vec::new();
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    match q.view.as_deref().unwrap_or("all") {
        "album" => {
            if let Some(id) = q.ref_id {
                wheres.push("t.album_id = ?".into());
                args.push(Box::new(id));
            }
        }
        "artist" => {
            if let Some(id) = q.ref_id {
                wheres.push("t.artist_id = ?".into());
                args.push(Box::new(id));
            }
        }
        "favorites" => {
            wheres.push("t.fav = 1".into());
        }
        _ => {}
    }
    if let Some(s) = q.search.as_deref().filter(|s| !s.trim().is_empty()) {
        let like = format!("%{}%", s.trim());
        wheres.push("(t.title LIKE ? OR IFNULL(a.name,'') LIKE ? OR IFNULL(al.title,'') LIKE ?)".into());
        args.push(Box::new(like.clone()));
        args.push(Box::new(like.clone()));
        args.push(Box::new(like));
    }
    let where_sql = if wheres.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", wheres.join(" AND "))
    };
    // 排序：支持 "-" 前缀表示降序（表头点击排序）
    let order_sql = match q.sort.as_deref() {
        Some("-title") => "ORDER BY t.title COLLATE NOCASE DESC",
        Some("album") => "ORDER BY IFNULL(al.title,'~') COLLATE NOCASE ASC, IFNULL(t.disc_no,0) ASC, IFNULL(t.track_no,0) ASC, t.title COLLATE NOCASE ASC",
        Some("-album") => "ORDER BY IFNULL(al.title,'~') COLLATE NOCASE DESC, IFNULL(t.disc_no,0) DESC, IFNULL(t.track_no,0) DESC, t.title COLLATE NOCASE DESC",
        Some("artist") => "ORDER BY IFNULL(a.name,'~') COLLATE NOCASE ASC, IFNULL(al.title,'~') COLLATE NOCASE ASC, IFNULL(t.track_no,0) ASC",
        Some("-artist") => "ORDER BY IFNULL(a.name,'~') COLLATE NOCASE DESC, IFNULL(al.title,'~') COLLATE NOCASE DESC, IFNULL(t.track_no,0) DESC",
        Some("added") => "ORDER BY t.id DESC",
        Some("none") => "ORDER BY t.id ASC",
        Some("duration") => "ORDER BY IFNULL(t.duration,0) ASC",
        Some("-duration") => "ORDER BY IFNULL(t.duration,0) DESC",
        Some("recent") => "ORDER BY CASE WHEN t.last_played_at IS NULL THEN 1 ELSE 0 END, t.last_played_at DESC",
        _ => "ORDER BY t.title COLLATE NOCASE ASC",
    };
    let page = q.page.unwrap_or(0) as i64;
    let page_size = q.page_size.unwrap_or(200).clamp(1, 5000) as i64;

    let count_sql = format!(
        "SELECT COUNT(*) FROM tracks t LEFT JOIN artists a ON a.id = t.artist_id LEFT JOIN albums al ON al.id = t.album_id {where_sql}"
    );
    let total: i64 = conn
        .query_row(&count_sql, params_from_iter(args.iter().map(|b| b.as_ref())), |r| r.get(0))
        .map_err(|e| e.to_string())?;

    let sql = format!("{TRACK_SELECT} {where_sql} {order_sql} LIMIT {page_size} OFFSET {}", page * page_size);
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let items = stmt
        .query_map(params_from_iter(args.iter().map(|b| b.as_ref())), row_track)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(Page { total, items })
}

#[tauri::command]
pub fn query_albums(
    state: State<'_, AppState>,
    search: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
) -> Result<Page<AlbumItem>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let page = page.unwrap_or(0) as i64;
    let page_size = page_size.unwrap_or(200).clamp(1, 500) as i64;
    let offset = page * page_size;

    let like = search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{s}%"));
    let where_sql = if like.is_some() {
        "WHERE al.title LIKE ?1 OR IFNULL(a.name,'') LIKE ?1"
    } else {
        ""
    };

    let total: i64 = if like.is_some() {
        conn.query_row(
            &format!("SELECT COUNT(*) FROM albums al LEFT JOIN artists a ON a.id = al.artist_id {where_sql}"),
            params![like],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?
    } else {
        conn.query_row(
            &format!("SELECT COUNT(*) FROM albums al LEFT JOIN artists a ON a.id = al.artist_id"),
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?
    };

    let sql = format!(
        "SELECT al.id, al.title, a.name, al.year, al.has_cover, \
                (SELECT COUNT(*) FROM tracks t WHERE t.album_id = al.id) \
         FROM albums al LEFT JOIN artists a ON a.id = al.artist_id \
         {where_sql} ORDER BY al.title COLLATE NOCASE LIMIT {page_size} OFFSET {offset}"
    );
    let items: Vec<AlbumItem> = collect_rows(
        &conn,
        &sql,
        like.as_ref(),
        |r| {
            Ok(AlbumItem {
                id: r.get(0)?,
                title: r.get(1)?,
                artist: r.get(2)?,
                year: r.get(3)?,
                has_cover: r.get::<_, i64>(4)? != 0,
                track_count: r.get(5)?,
            })
        },
    )?;
    Ok(Page { total, items })
}

#[tauri::command]
pub fn query_artists(
    state: State<'_, AppState>,
    search: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
) -> Result<Page<ArtistItem>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let page = page.unwrap_or(0) as i64;
    let page_size = page_size.unwrap_or(300).clamp(1, 1000) as i64;
    let offset = page * page_size;

    let like = search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{s}%"));
    let where_sql = if like.is_some() { "WHERE ar.name LIKE ?1" } else { "" };

    let total: i64 = if like.is_some() {
        conn.query_row(&format!("SELECT COUNT(*) FROM artists ar {where_sql}"), params![like], |r| r.get(0))
            .map_err(|e| e.to_string())?
    } else {
        conn.query_row("SELECT COUNT(*) FROM artists ar", [], |r| r.get(0))
            .map_err(|e| e.to_string())?
    };

    let sql = format!(
        "SELECT ar.id, ar.name, (SELECT COUNT(*) FROM tracks t WHERE t.artist_id = ar.id) \
         FROM artists ar {where_sql} ORDER BY ar.name COLLATE NOCASE LIMIT {page_size} OFFSET {offset}"
    );
    let items: Vec<ArtistItem> = collect_rows(&conn, &sql, like.as_ref(), |r| {
        Ok(ArtistItem { id: r.get(0)?, name: r.get(1)?, track_count: r.get(2)? })
    })?;
    Ok(Page { total, items })
}

#[tauri::command]
pub fn get_track(state: State<'_, AppState>, id: i64) -> Result<Option<Track>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.query_row(&format!("{TRACK_SELECT} WHERE t.id = ?1"), params![id], row_track)
        .optional()
        .map_err(|e| e.to_string())
}

/// 批量按 id 取曲目（播放队列快照还原用）
#[tauri::command]
pub fn get_tracks_by_ids(state: State<'_, AppState>, ids: Vec<i64>) -> Result<Vec<Track>, String> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("{TRACK_SELECT} WHERE t.id IN ({placeholders})");
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let items = stmt
        .query_map(params_from_iter(ids.iter()), row_track)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(items)
}

#[tauri::command]
pub fn get_stream_url(state: State<'_, AppState>, id: i64) -> Result<String, String> {
    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let exists: Option<i64> = conn
            .query_row("SELECT id FROM tracks WHERE id = ?1", params![id], |r| r.get(0))
            .ok();
        if exists.is_none() {
            return Err("曲目不存在".into());
        }
    }
    // Windows 上自定义协议以 http://{scheme}.localhost 形式访问
    #[cfg(target_os = "windows")]
    let url = format!("http://music.localhost/track/{id}");
    #[cfg(not(target_os = "windows"))]
    let url = format!("music://track/{id}");
    Ok(url)
}

#[tauri::command]
pub fn library_stats(state: State<'_, AppState>) -> Result<LibraryStats, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let tracks: i64 = conn.query_row("SELECT COUNT(*) FROM tracks", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    let albums: i64 = conn.query_row("SELECT COUNT(*) FROM albums", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    let artists: i64 = conn.query_row("SELECT COUNT(*) FROM artists", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    let favorites: i64 = conn
        .query_row("SELECT COUNT(*) FROM tracks WHERE fav = 1", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    Ok(LibraryStats { tracks, albums, artists, favorites })
}

// ---------- 喜欢（M2.5） ----------

#[tauri::command]
pub fn favorite_toggle(state: State<'_, AppState>, id: i64, fav: bool) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("UPDATE tracks SET fav = ?1 WHERE id = ?2", params![fav as i64, id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- 其他 ----------

/// 在系统文件管理器中显示曲目文件
#[tauri::command]
pub fn reveal_track(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let (rel, base) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT t.path, s.base_path FROM tracks t JOIN sources s ON s.id = t.source_id WHERE t.id = ?1",
            params![id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
        )
        .map_err(|e| e.to_string())?
    };
    let Some(base) = base else { return Err("仅本地曲目支持此操作".into()) };
    let full = std::path::PathBuf::from(base).join(rel);
    if !full.exists() {
        return Err("文件不存在".into());
    }
    tauri_plugin_opener::reveal_item_in_dir(&full).map_err(|e| e.to_string())
}

// ================================================================ 歌单（M2）

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub track_count: i64,
    pub created_at: Option<i64>,
}

const PLAYLIST_SELECT: &str = "SELECT p.id, p.name, (SELECT COUNT(*) FROM playlist_items i WHERE i.playlist_id = p.id), p.created_at FROM playlists p";

fn row_playlist(r: &rusqlite::Row) -> rusqlite::Result<Playlist> {
    Ok(Playlist { id: r.get(0)?, name: r.get(1)?, track_count: r.get(2)?, created_at: r.get(3)? })
}

#[tauri::command]
pub fn playlist_list(state: State<'_, AppState>) -> Result<Vec<Playlist>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(&format!("{PLAYLIST_SELECT} ORDER BY p.sort, p.id"))
        .map_err(|e| e.to_string())?;
    let items = stmt
        .query_map([], row_playlist)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(items)
}

#[tauri::command]
pub fn playlist_create(state: State<'_, AppState>, name: String) -> Result<Playlist, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("歌单名不能为空".into());
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("INSERT INTO playlists (name, created_at) VALUES (?1, ?2)", params![name, now])
        .map_err(|e| e.to_string())?;
    let id = conn.last_insert_rowid();
    Ok(Playlist { id, name, track_count: 0, created_at: Some(now) })
}

#[tauri::command]
pub fn playlist_rename(state: State<'_, AppState>, id: i64, name: String) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("歌单名不能为空".into());
    }
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("UPDATE playlists SET name = ?1 WHERE id = ?2", params![name, id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn playlist_delete(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM playlists WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn playlist_get_items(state: State<'_, AppState>, id: i64) -> Result<Vec<Track>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let sql = format!(
        "{TRACK_SELECT} JOIN playlist_items i ON i.track_id = t.id WHERE i.playlist_id = ?1 ORDER BY i.position, i.id"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let items = stmt
        .query_map([id], row_track)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(items)
}

#[tauri::command]
pub fn playlist_add_tracks(state: State<'_, AppState>, id: i64, track_ids: Vec<i64>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let max_pos: i64 = conn
        .query_row(
            "SELECT IFNULL(MAX(position), -1) FROM playlist_items WHERE playlist_id = ?1",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    for (i, tid) in track_ids.iter().enumerate() {
        conn.execute(
            "INSERT INTO playlist_items (playlist_id, track_id, position) VALUES (?1, ?2, ?3)",
            params![id, tid, max_pos + 1 + i as i64],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn playlist_remove_track(state: State<'_, AppState>, id: i64, track_id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM playlist_items WHERE playlist_id = ?1 AND track_id = ?2",
        params![id, track_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn playlist_reorder(state: State<'_, AppState>, id: i64, track_ids: Vec<i64>) -> Result<(), String> {
    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM playlist_items WHERE playlist_id = ?1", [id]).map_err(|e| e.to_string())?;
    for (i, tid) in track_ids.iter().enumerate() {
        tx.execute(
            "INSERT INTO playlist_items (playlist_id, track_id, position) VALUES (?1, ?2, ?3)",
            params![id, tid, i as i64],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}

// ================================================================ 播放统计与歌词（M2）

#[tauri::command]
pub fn report_play(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE tracks SET play_count = play_count + 1, last_played_at = ?1 WHERE id = ?2",
        params![now, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_lyrics(app: AppHandle, id: i64) -> Result<Option<String>, String> {
    crate::lyrics::fetch(&app, id)
}

// ================================================================ 应用设置（M2/M3）

#[tauri::command]
pub fn get_setting(state: State<'_, AppState>, key: String) -> Result<Option<String>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    Ok(db::get_setting(&conn, &key))
}

#[tauri::command]
pub fn set_setting(state: State<'_, AppState>, key: String, value: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::set_setting(&conn, &key, &value);
    Ok(())
}

// ================================================================ 局域网共享与发现（M3）

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareStatus {
    pub running: bool,
    pub port: u16,
    pub token: Option<String>,
    pub name: String,
}

#[tauri::command]
pub fn share_get_status(app: AppHandle, state: State<'_, AppState>) -> Result<ShareStatus, String> {
    let running = crate::network::share::is_running(&app);
    let port = crate::network::share::running_port(&app).unwrap_or(
        crate::network::get_setting(&app, "share_port")
            .and_then(|v| v.parse().ok())
            .unwrap_or(crate::network::share_default_port()),
    );
    let token = crate::network::get_setting(&app, "share_token");
    let name = crate::network::get_setting(&app, "share_name").unwrap_or_else(|| "LanMusic".into());
    let _ = state;
    Ok(ShareStatus { running, port, token, name })
}

#[tauri::command]
pub fn share_set_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        crate::network::share::start(&app)
    } else {
        crate::network::share::stop(&app);
        Ok(())
    }
}

#[tauri::command]
pub fn net_discover_start(app: AppHandle) -> Result<(), String> {
    crate::network::mdns::browse_start(&app)
}

#[tauri::command]
pub fn net_discover_stop(app: AppHandle) -> Result<(), String> {
    crate::network::mdns::browse_stop(&app);
    Ok(())
}

/// 连接发现的设备（或手动输入地址），验证配对码后创建 LAN 来源并扫描
#[tauri::command]
pub fn lan_add_source(
    app: AppHandle,
    state: State<'_, AppState>,
    addr: String,
    token: String,
    name: Option<String>,
) -> Result<Source, String> {
    let addr = addr.trim().to_string();
    let (dev_name, _) = crate::network::lan::hello(&addr, &token)?;
    let base = if addr.starts_with("http") { addr.trim_end_matches('/').to_string() } else { format!("http://{}", addr.trim_end_matches('/')) };
    let config = serde_json::json!({ "token": token }).to_string();

    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let exists: Option<i64> = conn
        .query_row("SELECT id FROM sources WHERE kind = 'lan' AND base_url = ?1", params![base], |r| r.get(0))
        .ok();
    if exists.is_some() {
        return Err("该设备已添加过".into());
    }
    conn.execute(
        "INSERT INTO sources (kind, name, base_url, config) VALUES ('lan', ?1, ?2, ?3)",
        params![name.unwrap_or(dev_name), base, config],
    )
    .map_err(|e| e.to_string())?;
    let id = conn.last_insert_rowid();
    drop(conn);

    spawn_scan(&app, &state, id, false)?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.query_row(&format!("{SOURCE_SELECT} WHERE s.id = ?1"), params![id], row_source)
        .map_err(|e| e.to_string())
}

/// 添加 WebDAV 来源（NAS），验证连通性后扫描
#[tauri::command]
pub fn webdav_add_source(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
    username: String,
    password: String,
    name: Option<String>,
) -> Result<Source, String> {
    let base = crate::network::webdav::normalize_base(&url)?;
    let host = base.host_str().map(|h| h.to_string()).unwrap_or_else(|| url.clone());
    let display_name = name.unwrap_or_else(|| format!("WebDAV ({host})"));
    let auth = crate::network::webdav::Auth { username, password };
    // 连通性验证：列根目录
    crate::network::webdav::list_dir(&base, Some(&auth))?;
    let config = serde_json::json!({ "username": auth.username, "password": auth.password }).to_string();

    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let exists: Option<i64> = conn
        .query_row("SELECT id FROM sources WHERE kind = 'webdav' AND base_url = ?1", params![base.as_str()], |r| r.get(0))
        .ok();
    if exists.is_some() {
        return Err("该地址已添加过".into());
    }
    conn.execute(
        "INSERT INTO sources (kind, name, base_url, config) VALUES ('webdav', ?1, ?2, ?3)",
        params![display_name, base.as_str(), config],
    )
    .map_err(|e| e.to_string())?;
    let id = conn.last_insert_rowid();
    drop(conn);

    spawn_scan(&app, &state, id, false)?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.query_row(&format!("{SOURCE_SELECT} WHERE s.id = ?1"), params![id], row_source)
        .map_err(|e| e.to_string())
}
