//! IPC 命令层：薄封装，参数校验后操作数据库 / 触发扫描。

use rusqlite::{params, params_from_iter, OptionalExtension};
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
    pub genres: i64,
    pub favorites: i64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GenreItem {
    pub name: String,
    pub track_count: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackQuery {
    pub view: Option<String>,
    pub ref_id: Option<i64>,
    pub genre: Option<String>,
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

    // 目录监听：本地文件变化后自动增量扫描
    crate::watcher::watch_source(&app, id, &path);

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
    let kind: Option<String> = conn
        .query_row("SELECT kind FROM sources WHERE id = ?1", params![id], |r| r.get(0))
        .ok();
    conn.execute("DELETE FROM sources WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM albums WHERE id NOT IN (SELECT DISTINCT album_id FROM tracks)", [])
        .map_err(|e| e.to_string())?;
    // 专辑归属艺人（albums.artist_id）可能没有直接归属的曲目，删除时需一并排除
    conn.execute(
        "DELETE FROM artists WHERE id NOT IN (SELECT DISTINCT artist_id FROM tracks)
         AND id NOT IN (SELECT DISTINCT artist_id FROM albums)",
        [],
    )
    .map_err(|e| e.to_string())?;
    drop(conn);
    crate::covers::purge(&state.covers_dir, &orphan_albums);

    // 清理收尾：webdav 来源移除钥匙串凭证；本地来源停止目录监听
    match kind.as_deref() {
        Some("webdav") => crate::keyring::delete_password(id),
        Some("local") => crate::watcher::unwatch_source(&app, id),
        _ => {}
    }
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
        "genre" => {
            if let Some(g) = q.genre.as_deref().filter(|s| !s.trim().is_empty()) {
                wheres.push("t.genre = ?".into());
                args.push(Box::new(g.trim().to_string()));
            }
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
    // 只展示有曲目的艺人：albums.artist_id 现在可指向纯合辑/专辑归属艺人（无直接曲目），不进列表
    let base_where = "ar.id IN (SELECT DISTINCT artist_id FROM tracks)";
    let where_sql = if like.is_some() {
        format!("WHERE {base_where} AND ar.name LIKE ?1")
    } else {
        format!("WHERE {base_where}")
    };

    let total: i64 = if like.is_some() {
        conn.query_row(&format!("SELECT COUNT(*) FROM artists ar {where_sql}"), params![like], |r| r.get(0))
            .map_err(|e| e.to_string())?
    } else {
        conn.query_row(&format!("SELECT COUNT(*) FROM artists ar {where_sql}"), [], |r| r.get(0))
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
    // 分批 IN 查询：SQLite 绑定变量有上限，超大队列快照一次性展开会报错
    const CHUNK_SIZE: usize = 900;
    let mut items = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(CHUNK_SIZE) {
        let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!("{TRACK_SELECT} WHERE t.id IN ({placeholders})");
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params_from_iter(chunk.iter()), row_track)
            .map_err(|e| e.to_string())?;
        for r in rows {
            items.push(r.map_err(|e| e.to_string())?);
        }
    }
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
    let genres: i64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT genre) FROM tracks WHERE genre IS NOT NULL AND genre != ''",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let favorites: i64 = conn
        .query_row("SELECT COUNT(*) FROM tracks WHERE fav = 1", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    Ok(LibraryStats { tracks, albums, artists, genres, favorites })
}

// ---------- 风格 ----------

/// 风格（genre）列表：来自曲目标签，仅展示非空风格及各自曲目数
#[tauri::command]
pub fn query_genres(
    state: State<'_, AppState>,
    search: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
) -> Result<Page<GenreItem>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let page = page.unwrap_or(0) as i64;
    let page_size = page_size.unwrap_or(300).clamp(1, 1000) as i64;
    let offset = page * page_size;

    let like = search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{s}%"));
    let base_where = "t.genre IS NOT NULL AND t.genre != ''";
    let where_sql = if like.is_some() {
        format!("WHERE {base_where} AND t.genre LIKE ?1")
    } else {
        format!("WHERE {base_where}")
    };

    let count_sql = format!("SELECT COUNT(DISTINCT t.genre) FROM tracks t {where_sql}");
    let total: i64 = match like.as_ref() {
        Some(l) => conn.query_row(&count_sql, params![l], |r| r.get(0)),
        None => conn.query_row(&count_sql, [], |r| r.get(0)),
    }
    .map_err(|e| e.to_string())?;

    let sql = format!(
        "SELECT t.genre, COUNT(*) FROM tracks t {where_sql} \
         GROUP BY t.genre ORDER BY t.genre COLLATE NOCASE LIMIT {page_size} OFFSET {offset}"
    );
    let items: Vec<GenreItem> = collect_rows(&conn, &sql, like.as_ref(), |r| {
        Ok(GenreItem { name: r.get(0)?, track_count: r.get(1)? })
    })?;
    Ok(Page { total, items })
}

// ---------- 喜欢（M2.5） ----------

#[tauri::command]
pub fn favorite_toggle(state: State<'_, AppState>, id: i64, fav: bool) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("UPDATE tracks SET fav = ?1 WHERE id = ?2", params![fav as i64, id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- 任务栏缩略图控制（Windows） ----------

/// 前端播放状态变化时同步任务栏缩略图按钮的播放/暂停图标（非 Windows 上为空操作）
#[tauri::command]
pub fn set_thumbbar_playing(playing: bool) -> Result<(), String> {
    #[cfg(windows)]
    crate::thumbbar::set_playing(playing);
    #[cfg(not(windows))]
    let _ = playing;
    Ok(())
}

// ---------- 系统字体 ----------

/// 枚举系统已安装字体（DirectWrite 字体集合的字族名），供全局字体设置选择；
/// 非 Windows 平台返回空列表（前端隐藏字体下拉的字体项）。
#[tauri::command]
pub fn list_system_fonts() -> Result<Vec<String>, String> {
    #[cfg(windows)]
    return Ok(crate::fonts::system_fonts());
    #[cfg(not(windows))]
    return Ok(Vec::new());
}

// ---------- 桌面歌词 ----------

/// 开启/关闭桌面歌词浮窗（置顶、无边框、可拖动），返回最终状态。
/// 浮窗与主窗口共用前端资源，前端按窗口 label（lyrics）渲染桌面歌词 UI。
///
/// 注意：必须为 async 命令。Windows 上 WebView2 窗口的创建会阻塞等待主线程消息，
/// 同步命令在主线程执行会导致消息循环死锁（应用卡死），async 命令在工作线程执行、
/// 由 Tauri 内部代理到主线程完成创建。
#[tauri::command]
pub async fn desktop_lyrics_set(app: AppHandle, enabled: bool) -> Result<bool, String> {
    if !enabled {
        if let Some(w) = app.get_webview_window("lyrics") {
            w.close().map_err(|e| e.to_string())?;
        }
        return Ok(false);
    }
    if app.get_webview_window("lyrics").is_some() {
        return Ok(true);
    }
    let builder = tauri::webview::WebviewWindowBuilder::new(
        &app,
        "lyrics",
        tauri::WebviewUrl::App("index.html".into()),
    )
    .title("LanMusic 桌面歌词")
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .shadow(false)
    .focused(false)
    .inner_size(760.0, 170.0);
    // 透明背景：Windows/Linux 支持；macOS 需 macos-private-api feature，v1 暂不启用
    #[cfg(any(windows, target_os = "linux"))]
    let builder = builder.transparent(true);
    let win = builder.build().map_err(|e| e.to_string())?;

    // 主显示器底部居中（上方留出约 120 逻辑像素，避开任务栏区域）
    if let Ok(Some(monitor)) = win.primary_monitor() {
        let scale = monitor.scale_factor();
        let (logical_w, logical_h) = (760.0_f64, 170.0_f64);
        let screen = monitor.size();
        let pos = monitor.position();
        let x = pos.x + ((screen.width as f64 - logical_w * scale) / 2.0) as i32;
        let y = pos.y + (screen.height as f64 - logical_h * scale) as i32 - (120.0 * scale) as i32;
        let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
    }
    Ok(true)
}

// ---------- 其他 ----------

/// 播放时阻止系统休眠/锁屏。
/// Windows 通过 SetThreadExecutionState 请求系统保持运行与屏幕常亮；
/// 在其他平台为空操作（前端可回退到 Web Wake Lock API）。
#[tauri::command]
#[cfg_attr(not(windows), allow(unused_variables))]
pub fn set_prevent_sleep(prevent: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows::Win32::System::Power::{
            SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED,
        };
        let flags = if prevent {
            ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED
        } else {
            ES_CONTINUOUS
        };
        // SAFETY: 在应用主线程调用，API 本身无内存安全前置条件
        let prev = unsafe { SetThreadExecutionState(flags) };
        if prev.0 == 0 {
            return Err("SetThreadExecutionState 调用失败".into());
        }
    }
    Ok(())
}

/// 退出应用（托盘菜单「退出」）
#[tauri::command]
pub fn exit_app(app: AppHandle) {
    app.exit(0);
}

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
    /// 歌单封面：最新加入歌曲的专辑 id（空歌单为 None）
    pub cover_album_id: Option<i64>,
    pub description: Option<String>,
}

const PLAYLIST_SELECT: &str = "SELECT p.id, p.name, \
     (SELECT COUNT(*) FROM playlist_items i WHERE i.playlist_id = p.id), p.created_at, \
     (SELECT t.album_id FROM playlist_items i JOIN tracks t ON t.id = i.track_id \
      WHERE i.playlist_id = p.id AND t.album_id IS NOT NULL \
      ORDER BY i.added_at DESC, i.id DESC LIMIT 1), \
     p.description \
     FROM playlists p";

fn row_playlist(r: &rusqlite::Row) -> rusqlite::Result<Playlist> {
    Ok(Playlist {
        id: r.get(0)?,
        name: r.get(1)?,
        track_count: r.get(2)?,
        created_at: r.get(3)?,
        cover_album_id: r.get(4)?,
        description: r.get(5)?,
    })
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
    Ok(Playlist { id, name, track_count: 0, created_at: Some(now), cover_album_id: None, description: None })
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

/// 设置歌单简介（空字符串视为清除简介）
#[tauri::command]
pub fn playlist_set_description(state: State<'_, AppState>, id: i64, description: String) -> Result<(), String> {
    let trimmed = description.trim().to_string();
    let value: Option<String> = if trimmed.is_empty() { None } else { Some(trimmed) };
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("UPDATE playlists SET description = ?1 WHERE id = ?2", params![value, id])
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
        // 按加入时间倒序：新添加的歌曲排在最前；同时加入的按插入顺序（id）倒序
        "{TRACK_SELECT} JOIN playlist_items i ON i.track_id = t.id WHERE i.playlist_id = ?1 ORDER BY i.added_at DESC, i.id DESC"
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
pub fn playlist_add_tracks(state: State<'_, AppState>, id: i64, track_ids: Vec<i64>) -> Result<usize, String> {
    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let now: i64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let max_pos: i64 = tx
        .query_row(
            "SELECT IFNULL(MAX(position), -1) FROM playlist_items WHERE playlist_id = ?1",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    // 同一歌单内去重：已存在的曲目跳过，返回实际新增数量
    let mut added = 0usize;
    for tid in track_ids {
        let exists: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM playlist_items WHERE playlist_id = ?1 AND track_id = ?2",
                params![id, tid],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if exists > 0 {
            continue;
        }
        added += 1;
        tx.execute(
            "INSERT INTO playlist_items (playlist_id, track_id, position, added_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, tid, max_pos + added as i64, now],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(added)
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

/// 批量移除歌单中的多首歌曲
#[tauri::command]
pub fn playlist_remove_tracks(state: State<'_, AppState>, id: i64, track_ids: Vec<i64>) -> Result<(), String> {
    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    for tid in track_ids {
        tx.execute(
            "DELETE FROM playlist_items WHERE playlist_id = ?1 AND track_id = ?2",
            params![id, tid],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn playlist_reorder(state: State<'_, AppState>, id: i64, track_ids: Vec<i64>) -> Result<(), String> {
    use std::collections::HashMap;
    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    // 保留原加入时间：重排只改变 position，不影响“按加入时间倒序”的展示
    let old_added_at: HashMap<i64, i64> = {
        let mut stmt = tx
            .prepare("SELECT track_id, added_at FROM playlist_items WHERE playlist_id = ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([id], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?.unwrap_or(0)))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows.into_iter().collect()
    };
    tx.execute("DELETE FROM playlist_items WHERE playlist_id = ?1", [id])
        .map_err(|e| e.to_string())?;
    let now: i64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    for (i, tid) in track_ids.iter().enumerate() {
        let added_at = old_added_at.get(tid).copied().unwrap_or(now);
        tx.execute(
            "INSERT INTO playlist_items (playlist_id, track_id, position, added_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, tid, i as i64, added_at],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}

/// 歌单封面：最新加入歌曲的专辑 id（前端经 cover:// 协议惰性加载封面；空歌单返回 None）
#[tauri::command]
pub fn playlist_cover(state: State<'_, AppState>, id: i64) -> Result<Option<i64>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT t.album_id FROM playlist_items i JOIN tracks t ON t.id = i.track_id
         WHERE i.playlist_id = ?1 AND t.album_id IS NOT NULL
         ORDER BY i.added_at DESC, i.id DESC LIMIT 1",
        [id],
        |r| r.get::<_, i64>(0),
    )
    .optional()
    .map_err(|e| e.to_string())
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

// ================================================================ WebDAV 来源（M3）

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

    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let exists: Option<i64> = conn
        .query_row("SELECT id FROM sources WHERE kind = 'webdav' AND base_url = ?1", params![base.as_str()], |r| r.get(0))
        .ok();
    if exists.is_some() {
        return Err("该地址已添加过".into());
    }
    // 密码不入库：config 只写 username，插入后把密码写入系统钥匙串；
    // 钥匙串不可用时回退明文（保证功能可用）
    let config = serde_json::json!({ "username": auth.username }).to_string();
    conn.execute(
        "INSERT INTO sources (kind, name, base_url, config) VALUES ('webdav', ?1, ?2, ?3)",
        params![display_name, base.as_str(), config],
    )
    .map_err(|e| e.to_string())?;
    let id = conn.last_insert_rowid();
    if crate::keyring::set_password(id, &auth.password).is_err() {
        let fallback = serde_json::json!({ "username": auth.username, "password": auth.password }).to_string();
        let _ = conn.execute("UPDATE sources SET config = ?1 WHERE id = ?2", params![fallback, id]);
    }
    drop(conn);

    spawn_scan(&app, &state, id, false)?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.query_row(&format!("{SOURCE_SELECT} WHERE s.id = ?1"), params![id], row_source)
        .map_err(|e| e.to_string())
}
