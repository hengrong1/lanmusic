//! IPC 命令层：薄封装，参数校验后操作数据库 / 触发扫描。

use std::collections::HashMap;

use rusqlite::{params, params_from_iter, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::db;
use crate::error::{codes, err, err1};
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
    /// 是否扫描来源内子目录（false = 仅扫描根目录下的文件）
    pub scan_subdirs: bool,
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
    pub has_mv: bool,
    pub fav: bool,
    /// 完整艺人列表（含合作艺人，按标签顺序）；单艺人曲目同样返回
    pub artists: Vec<TrackArtistRef>,
    /// 命中的搜索字段（title/artist/album/lyrics/filename），仅搜索时非空
    pub matched_fields: Vec<String>,
}

/// 曲目关联艺人（track_artists）
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TrackArtistRef {
    pub id: i64,
    pub name: String,
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
    /// 搜索范围（title/artist/album/lyrics/filename）；缺省 = 标题/艺人/专辑
    pub fields: Option<Vec<String>>,
    /// 是否启用拼音匹配（默认前端设置控制）
    pub pinyin: Option<bool>,
}

// ---------- 行映射 ----------

const TRACK_SELECT: &str = "SELECT t.id, t.title, a.name, t.artist_id, al.title, t.album_id, t.track_no, \
     t.disc_no, t.duration, t.bitrate, t.sample_rate, t.bit_depth, t.format, t.path, t.has_embedded_lyrics, t.has_mv, t.fav \
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
        has_mv: r.get::<_, i64>(15)? != 0,
        fav: r.get::<_, i64>(16)? != 0,
        artists: Vec::new(),
        matched_fields: Vec::new(),
    })
}

/// 批量附加每首曲目的完整艺人列表（track_artists 关联，按 ord 排序）。
/// 多艺人时用 " / " 连接覆盖 artist 显示串（首个为主艺人）。
pub(crate) fn attach_artists(
    conn: &rusqlite::Connection,
    tracks: &mut [Track],
) -> Result<(), String> {
    if tracks.is_empty() {
        return Ok(());
    }
    use std::collections::HashMap;
    let mut map: HashMap<i64, Vec<TrackArtistRef>> = HashMap::new();
    const CHUNK_SIZE: usize = 900;
    let ids: Vec<i64> = tracks.iter().map(|t| t.id).collect();
    for chunk in ids.chunks(CHUNK_SIZE) {
        let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT ta.track_id, a.id, a.name FROM track_artists ta \
             JOIN artists a ON a.id = ta.artist_id \
             WHERE ta.track_id IN ({placeholders}) ORDER BY ta.track_id, ta.ord"
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params_from_iter(chunk.iter()), |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    TrackArtistRef {
                        id: r.get(1)?,
                        name: r.get(2)?,
                    },
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (tid, ar) = row.map_err(|e| e.to_string())?;
            map.entry(tid).or_default().push(ar);
        }
    }
    for t in tracks.iter_mut() {
        if let Some(list) = map.remove(&t.id) {
            if list.len() > 1 {
                t.artist = Some(
                    list.iter()
                        .map(|a| a.name.as_str())
                        .collect::<Vec<_>>()
                        .join(" / "),
                );
                t.artist_id = Some(list[0].id);
            }
            t.artists = list;
        }
    }
    Ok(())
}

const SOURCE_SELECT: &str = "SELECT s.id, s.kind, s.name, s.base_path, s.base_url, s.enabled, s.last_scan_at, s.fast_import, s.scan_subdirs, \
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
        scan_subdirs: r.get::<_, i64>(8)? != 0,
        track_count: r.get(9)?,
    })
}

// ---------- 来源管理 ----------

fn spawn_scan(app: &AppHandle, state: &AppState, id: i64, full_rescan: bool) -> Result<(), String> {
    let mut scanning = state.scanning.lock().map_err(|e| e.to_string())?;
    if !scanning.insert(id) {
        return Err(err(codes::SOURCE_SCANNING));
    }
    drop(scanning);
    let app2 = app.clone();
    std::thread::spawn(move || scanner::scan_source(app2, id, full_rescan));
    Ok(())
}

#[tauri::command]
pub fn add_local_source(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<Source, String> {
    let p = std::path::PathBuf::from(&path);
    if !p.is_dir() {
        return Err(err(codes::SOURCE_DIR_MISSING));
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
        return Err(err(codes::SOURCE_DUPLICATE));
    }
    conn.execute(
        "INSERT INTO sources (kind, name, base_path) VALUES ('local', ?1, ?2)",
        params![name, path],
    )
    .map_err(|e| e.to_string())?;
    let id = conn.last_insert_rowid();
    drop(conn);
    log::info!("已添加本地来源 {id}: {path}");

    spawn_scan(&app, &state, id, false)?;

    // 目录监听：本地文件变化后自动增量扫描（新来源默认递归扫描子目录）
    crate::watcher::watch_source(&app, id, &path, true);

    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.query_row(
        &format!("{SOURCE_SELECT} WHERE s.id = ?1"),
        params![id],
        row_source,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_sources(state: State<'_, AppState>) -> Result<Vec<Source>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(&format!("{SOURCE_SELECT} ORDER BY s.id"))
        .map_err(|e| e.to_string())?;
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
    if state2
        .scanning
        .lock()
        .map_err(|e| e.to_string())?
        .contains(&id)
    {
        return Err(err(codes::SOURCE_SCANNING_BUSY));
    }
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let kind: Option<String> = conn
        .query_row("SELECT kind FROM sources WHERE id = ?1", params![id], |r| {
            r.get(0)
        })
        .ok();
    // 先删曲目再删来源。现库 schema 的 tracks.source_id 带 ON DELETE CASCADE（连接均开启
    // 外键），删来源本可级联清曲目；这里改为显式删除，让子表清理真实执行——既兼容外键
    // 未生效的异常连接，也让每条 DELETE 都作用于真实存在的行，而不是恒为空的子查询。
    conn.execute(
        "DELETE FROM track_artists WHERE track_id IN (SELECT id FROM tracks WHERE source_id = ?1)",
        params![id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM playlist_items WHERE track_id IN (SELECT id FROM tracks WHERE source_id = ?1)",
        params![id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM lrc_files WHERE track_id IN (SELECT id FROM tracks WHERE source_id = ?1)",
        params![id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM lyrics_index WHERE track_id IN (SELECT id FROM tracks WHERE source_id = ?1)",
        params![id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM tracks WHERE source_id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM sources WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    // 曲目删除后再查真正的孤儿专辑
    let orphan_albums: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT id FROM albums WHERE id NOT IN (SELECT DISTINCT album_id FROM tracks)")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| r.get::<_, i64>(0))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    // 先删封面文件再删 DB 行：如果先删 DB 行后 SQLite rowid 被复用，后续 purge 可能误删新专辑同名缓存
    let covers_dir = state.covers_dir.clone();
    drop(conn);
    crate::covers::purge(&covers_dir, &orphan_albums);
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    // 按已收集的 id 精确删除（与传给 purge 的集合完全一致）
    if !orphan_albums.is_empty() {
        let placeholders = orphan_albums
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!("DELETE FROM albums WHERE id IN ({placeholders})");
        let _ = conn.execute(&sql, rusqlite::params_from_iter(orphan_albums.iter()));
    }
    // 专辑归属艺人（albums.artist_id）可能没有直接归属的曲目，删除时需一并排除
    conn.execute(
        "DELETE FROM artists WHERE id NOT IN (SELECT DISTINCT artist_id FROM tracks)
         AND id NOT IN (SELECT DISTINCT artist_id FROM track_artists)
         AND id NOT IN (SELECT DISTINCT artist_id FROM albums)",
        [],
    )
    .map_err(|e| e.to_string())?;
    drop(conn);

    // 清理收尾：webdav 来源移除钥匙串凭证；本地来源停止目录监听
    match kind.as_deref() {
        Some("webdav") => crate::keyring::delete_password(id),
        Some("local") => crate::watcher::unwatch_source(&app, id),
        _ => {}
    }
    // 破坏性操作留痕：日志里能看到来源何时被删（曲目、歌单引用随之清除）
    log::info!("已删除来源 {id}（{}）", kind.as_deref().unwrap_or("未知类型"));
    Ok(())
}

/// mode: "auto" = 增量（新文件/变化文件/快速导入未解析的行）；"full" = 全部重新解析
#[tauri::command]
pub fn rescan_source(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    mode: Option<String>,
) -> Result<(), String> {
    let full = mode.as_deref() == Some("full");
    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let exists: Option<i64> = conn
            .query_row("SELECT id FROM sources WHERE id = ?1", params![id], |r| {
                r.get(0)
            })
            .ok();
        if exists.is_none() {
            return Err(err(codes::SOURCE_NOT_FOUND));
        }
    }
    spawn_scan(&app, &state, id, full)
}

/// 开关快速导入：开启后扫描只按文件名/目录结构入库（不读文件内容），适合慢速网络目录
#[tauri::command]
pub fn set_source_fast_import(
    state: State<'_, AppState>,
    id: i64,
    enabled: bool,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE sources SET fast_import = ?1 WHERE id = ?2 AND kind = 'local'",
        params![enabled as i64, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 开关子目录扫描：开启后递归扫描来源内所有子目录；关闭后仅扫描根目录下的文件
/// （本地与 WebDAV 来源通用）。本地来源同时按新模式重建目录监听。
#[tauri::command]
pub fn set_source_scan_subdirs(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    enabled: bool,
) -> Result<(), String> {
    let local_base: Option<String> = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE sources SET scan_subdirs = ?1 WHERE id = ?2",
            params![enabled as i64, id],
        )
        .map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT base_path FROM sources WHERE id = ?1 AND kind = 'local' AND base_path IS NOT NULL",
            params![id],
            |r| r.get(0),
        )
        .ok()
    };
    // 监听模式跟随扫描模式：仅根目录时用 NonRecursive，子目录变化不再触发无谓重扫
    if let Some(base) = local_base {
        crate::watcher::unwatch_source(&app, id);
        crate::watcher::watch_source(&app, id, &base, enabled);
    }
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

/// 视图过滤条件（专辑/艺人/喜欢），供普通查询与搜索路径共用
pub(crate) fn view_filter(q: &TrackQuery) -> (String, Vec<Box<dyn rusqlite::ToSql>>) {
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
                // 主艺人或 track_artists 关联的合作艺人都命中
                wheres.push(
                    "(t.artist_id = ? OR EXISTS (SELECT 1 FROM track_artists ta WHERE ta.track_id = t.id AND ta.artist_id = ?))"
                        .into(),
                );
                args.push(Box::new(id));
                args.push(Box::new(id));
            }
        }
        "favorites" => {
            wheres.push("t.fav = 1".into());
        }
        _ => {}
    }
    let where_sql = if wheres.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", wheres.join(" AND "))
    };
    (where_sql, args)
}

#[tauri::command]
pub fn query_tracks(state: State<'_, AppState>, q: TrackQuery) -> Result<Page<Track>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    // 搜索路径：多字段 + 拼音 + 歌词/文件名匹配、相关度评分（见 search.rs）
    if q.search
        .as_deref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
    {
        return crate::search::search_tracks(&conn, &q);
    }

    let (view_where, mut args) = view_filter(&q);
    let args = &mut args;
    let where_sql = view_where;
    // 排序：支持 "-" 前缀表示降序（表头点击排序）。
    // COLLATE PINYIN：自定义 collation（db.rs::open_conn 注册），分组序 = 数字 < 字母 < 汉字（拼音）。
    // 空值兜底用 CHAR(1114110)（U+10FFFE，大于 pinyin_key 的汉字哨兵 U+10FFFD），空专辑/艺人绝对排最后
    let order_sql = match q.sort.as_deref() {
        Some("-title") => "ORDER BY t.title COLLATE PINYIN DESC",
        Some("album") => "ORDER BY IFNULL(al.title, CHAR(1114110)) COLLATE PINYIN ASC, IFNULL(t.disc_no,0) ASC, IFNULL(t.track_no,0) ASC, t.title COLLATE PINYIN ASC",
        Some("-album") => "ORDER BY IFNULL(al.title, CHAR(1114110)) COLLATE PINYIN DESC, IFNULL(t.disc_no,0) DESC, IFNULL(t.track_no,0) DESC, t.title COLLATE PINYIN DESC",
        Some("artist") => "ORDER BY IFNULL(a.name, CHAR(1114110)) COLLATE PINYIN ASC, IFNULL(al.title, CHAR(1114110)) COLLATE PINYIN ASC, IFNULL(t.track_no,0) ASC",
        Some("-artist") => "ORDER BY IFNULL(a.name, CHAR(1114110)) COLLATE PINYIN DESC, IFNULL(al.title, CHAR(1114110)) COLLATE PINYIN DESC, IFNULL(t.track_no,0) DESC",
        Some("added") => "ORDER BY t.id DESC",
        Some("none") => "ORDER BY t.id ASC",
        Some("duration") => "ORDER BY IFNULL(t.duration,0) ASC",
        Some("-duration") => "ORDER BY IFNULL(t.duration,0) DESC",
        Some("recent") => "ORDER BY CASE WHEN t.last_played_at IS NULL THEN 1 ELSE 0 END, t.last_played_at DESC",
        _ => "ORDER BY t.title COLLATE PINYIN ASC",
    };
    let page = q.page.unwrap_or(0) as i64;
    let page_size = q.page_size.unwrap_or(200).clamp(1, 5000) as i64;

    let count_sql = format!(
        "SELECT COUNT(*) FROM tracks t LEFT JOIN artists a ON a.id = t.artist_id LEFT JOIN albums al ON al.id = t.album_id {where_sql}"
    );
    let total: i64 = conn
        .query_row(
            &count_sql,
            params_from_iter(args.iter().map(|b| b.as_ref())),
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;

    let sql = format!(
        "{TRACK_SELECT} {where_sql} {order_sql} LIMIT {page_size} OFFSET {}",
        page * page_size
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let mut items: Vec<Track> = stmt
        .query_map(params_from_iter(args.iter().map(|b| b.as_ref())), row_track)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    attach_artists(&conn, &mut items)?;
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
         {where_sql} ORDER BY al.title COLLATE PINYIN LIMIT {page_size} OFFSET {offset}"
    );
    let items: Vec<AlbumItem> = collect_rows(&conn, &sql, like.as_ref(), |r| {
        Ok(AlbumItem {
            id: r.get(0)?,
            title: r.get(1)?,
            artist: r.get(2)?,
            year: r.get(3)?,
            has_cover: r.get::<_, i64>(4)? != 0,
            track_count: r.get(5)?,
        })
    })?;
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
    // 只展示有曲目的艺人（主艺人或合作艺人）：albums.artist_id 现在可指向纯合辑/专辑归属艺人（无直接曲目），不进列表
    let base_where =
        "ar.id IN (SELECT artist_id FROM tracks UNION SELECT artist_id FROM track_artists)";
    let where_sql = if like.is_some() {
        format!("WHERE {base_where} AND ar.name LIKE ?1")
    } else {
        format!("WHERE {base_where}")
    };

    let total: i64 = if like.is_some() {
        conn.query_row(
            &format!("SELECT COUNT(*) FROM artists ar {where_sql}"),
            params![like],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?
    } else {
        conn.query_row(
            &format!("SELECT COUNT(*) FROM artists ar {where_sql}"),
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?
    };

    let sql = format!(
        "SELECT ar.id, ar.name, \
         (SELECT COUNT(*) FROM tracks t WHERE t.artist_id = ar.id \
           OR EXISTS (SELECT 1 FROM track_artists ta WHERE ta.track_id = t.id AND ta.artist_id = ar.id)) \
         FROM artists ar {where_sql} ORDER BY ar.name COLLATE PINYIN LIMIT {page_size} OFFSET {offset}"
    );
    let items: Vec<ArtistItem> = collect_rows(&conn, &sql, like.as_ref(), |r| {
        Ok(ArtistItem {
            id: r.get(0)?,
            name: r.get(1)?,
            track_count: r.get(2)?,
        })
    })?;
    Ok(Page { total, items })
}

#[tauri::command]
pub fn get_track(state: State<'_, AppState>, id: i64) -> Result<Option<Track>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut track = conn
        .query_row(
            &format!("{TRACK_SELECT} WHERE t.id = ?1"),
            params![id],
            row_track,
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some(t) = track.as_mut() {
        attach_artists(&conn, std::slice::from_mut(t))?;
    }
    Ok(track)
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
    attach_artists(&conn, &mut items)?;
    Ok(items)
}

#[tauri::command]
pub fn get_stream_url(state: State<'_, AppState>, id: i64) -> Result<String, String> {
    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let exists: Option<i64> = conn
            .query_row("SELECT id FROM tracks WHERE id = ?1", params![id], |r| {
                r.get(0)
            })
            .ok();
        if exists.is_none() {
            return Err(err(codes::TRACK_NOT_FOUND));
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
    let tracks: i64 = conn
        .query_row("SELECT COUNT(*) FROM tracks", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    let albums: i64 = conn
        .query_row("SELECT COUNT(*) FROM albums", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    let artists: i64 = conn
        .query_row("SELECT COUNT(*) FROM artists", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    let favorites: i64 = conn
        .query_row("SELECT COUNT(*) FROM tracks WHERE fav = 1", [], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?;
    Ok(LibraryStats {
        tracks,
        albums,
        artists,
        favorites,
    })
}

// ---------- 喜欢（M2.5） ----------

#[tauri::command]
pub fn favorite_toggle(state: State<'_, AppState>, id: i64, fav: bool) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE tracks SET fav = ?1 WHERE id = ?2",
        params![fav as i64, id],
    )
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

/// 前端切歌时报告当前曲目的专辑 id（非 Windows 上为空操作）；
/// Windows 任务栏悬停预览据此整块显示该专辑封面。
#[tauri::command]
pub fn set_thumbbar_album(album_id: Option<i64>) -> Result<(), String> {
    #[cfg(windows)]
    crate::thumbbar::set_album(album_id);
    #[cfg(not(windows))]
    let _ = album_id;
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
    // 透明背景：歌词浮窗必须透明（否则 macOS 显示 WKWebView 默认白底）。
    // macOS 需要 macos-private-api feature，已在 Cargo.toml 与 tauri.conf.json(macOSPrivateApi) 启用
    let builder = builder.transparent(true);
    // 先隐藏窗口：窗口可见时 WebView2 加载前端期间会闪出原生白底 + index.html
    // 不透明 splash（「先白一下」），等前端渲染完成再由前端 show()（挂载后双 rAF）
    let builder = builder.visible(false);
    let win = builder.build().map_err(|e| e.to_string())?;

    // 兜底：前端异常未调用 show 时 2 秒后强制显示（show 幂等，前端已显示再调无副作用）
    let fallback = win.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(2000));
        let _ = fallback.show();
    });

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
            return Err(err(codes::PREVENT_SLEEP_FAILED));
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
    let Some(base) = base else {
        return Err(err(codes::TRACK_NOT_LOCAL));
    };
    let full = std::path::PathBuf::from(base).join(rel);
    if !full.exists() {
        return Err(err(codes::FILE_MISSING));
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
        return Err(err(codes::PLAYLIST_NAME_EMPTY));
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO playlists (name, created_at) VALUES (?1, ?2)",
        params![name, now],
    )
    .map_err(|e| e.to_string())?;
    let id = conn.last_insert_rowid();
    Ok(Playlist {
        id,
        name,
        track_count: 0,
        created_at: Some(now),
        cover_album_id: None,
        description: None,
    })
}

#[tauri::command]
pub fn playlist_rename(state: State<'_, AppState>, id: i64, name: String) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(err(codes::PLAYLIST_NAME_EMPTY));
    }
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE playlists SET name = ?1 WHERE id = ?2",
        params![name, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 设置歌单简介（空字符串视为清除简介）
#[tauri::command]
pub fn playlist_set_description(
    state: State<'_, AppState>,
    id: i64,
    description: String,
) -> Result<(), String> {
    let trimmed = description.trim().to_string();
    let value: Option<String> = if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    };
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE playlists SET description = ?1 WHERE id = ?2",
        params![value, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn playlist_delete(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM playlists WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn playlist_get_items(
    state: State<'_, AppState>,
    id: i64,
    sort: Option<String>,
) -> Result<Vec<Track>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    // 默认（空/none）= 加入时间倒序：新添加的歌曲排在最前；同时加入的按插入顺序（id）倒序。
    // 表头排序与 query_tracks 同款 PINYIN 分组序（数字 < 字母 < 汉字），尾部以加入时间倒序兜底 tie-break
    let order_sql = match sort.as_deref() {
        Some("-title") => "ORDER BY t.title COLLATE PINYIN DESC, i.added_at DESC, i.id DESC",
        Some("album") => "ORDER BY IFNULL(al.title, CHAR(1114110)) COLLATE PINYIN ASC, IFNULL(t.disc_no,0) ASC, IFNULL(t.track_no,0) ASC, t.title COLLATE PINYIN ASC, i.added_at DESC, i.id DESC",
        Some("-album") => "ORDER BY IFNULL(al.title, CHAR(1114110)) COLLATE PINYIN DESC, IFNULL(t.disc_no,0) DESC, IFNULL(t.track_no,0) DESC, t.title COLLATE PINYIN DESC, i.added_at DESC, i.id DESC",
        Some("artist") => "ORDER BY IFNULL(a.name, CHAR(1114110)) COLLATE PINYIN ASC, IFNULL(al.title, CHAR(1114110)) COLLATE PINYIN ASC, IFNULL(t.track_no,0) ASC, i.added_at DESC, i.id DESC",
        Some("-artist") => "ORDER BY IFNULL(a.name, CHAR(1114110)) COLLATE PINYIN DESC, IFNULL(al.title, CHAR(1114110)) COLLATE PINYIN DESC, IFNULL(t.track_no,0) DESC, i.added_at DESC, i.id DESC",
        Some("duration") => "ORDER BY IFNULL(t.duration,0) ASC, i.added_at DESC, i.id DESC",
        Some("-duration") => "ORDER BY IFNULL(t.duration,0) DESC, i.added_at DESC, i.id DESC",
        _ => "ORDER BY i.added_at DESC, i.id DESC",
    };
    let sql = format!(
        "{TRACK_SELECT} JOIN playlist_items i ON i.track_id = t.id WHERE i.playlist_id = ?1 {order_sql}"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let mut items: Vec<Track> = stmt
        .query_map([id], row_track)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    // 与 query_tracks / get_tracks_by_ids 同口径：附加完整艺人列表，
    // 否则歌单里多艺人曲目只显示主艺人（与曲库/搜索视图不一致）
    attach_artists(&conn, &mut items)?;
    Ok(items)
}

#[tauri::command]
pub fn playlist_add_tracks(
    state: State<'_, AppState>,
    id: i64,
    track_ids: Vec<i64>,
) -> Result<usize, String> {
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
pub fn playlist_remove_track(
    state: State<'_, AppState>,
    id: i64,
    track_id: i64,
) -> Result<(), String> {
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
pub fn playlist_remove_tracks(
    state: State<'_, AppState>,
    id: i64,
    track_ids: Vec<i64>,
) -> Result<(), String> {
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
pub fn playlist_reorder(
    state: State<'_, AppState>,
    id: i64,
    track_ids: Vec<i64>,
) -> Result<(), String> {
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
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                ))
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

/// QRC 逐字歌词解析：解密（新旧两种加密格式）+ 词级毫秒时间轴，见 qrc.rs。
/// 非 QRC 内容返回 None，前端回退 LRC/纯文本解析。
#[tauri::command]
pub fn parse_qrc(raw: String) -> Result<Option<Vec<crate::qrc::QrcLine>>, String> {
    Ok(crate::qrc::parse(&raw))
}

/// 前端错误转发落盘（见 main.ts 的 installErrorGuard / boot 兜底）：release 版
/// WebView 没有控制台，渲染错误、未捕获的 Promise 拒绝、启动失败只有经由此命令
/// 才能进日志文件。排查用途，有意不本地化；凭证类内容不得经由此通道打印。
#[tauri::command]
pub fn frontend_log(level: String, message: String) {
    match level.as_str() {
        "error" => log::error!("[前端] {message}"),
        "warn" => log::warn!("[前端] {message}"),
        _ => log::info!("[前端] {message}"),
    }
}

/// 检查 GitHub 最新 Release（见 updater.rs）：远端版本更高才返回 Some。
/// 失败返回 Err（网络/限流），由前端决定提示方式（启动静默检查不弹框）。
///
/// 必须 async + spawn_blocking：Tauri 2 的同步 command 在**主线程**执行，
/// blocking HTTP 请求会把 UI 消息泵整个卡住（Windows 报「未响应」）。
#[tauri::command]
pub async fn check_github_update(
    app: AppHandle,
) -> Result<Option<crate::updater::ReleaseInfo>, String> {
    let current = app.package_info().version.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let result = crate::updater::latest_release(&current);
        match &result {
            Ok(Some(info)) => log::info!(
                "检查更新：发现新版本 {}（当前 {}）",
                info.version,
                current
            ),
            Ok(None) => log::info!("检查更新：已是最新（{current}）"),
            Err(e) => log::warn!("检查更新失败：{e}"),
        }
        result
    })
    .await
    .map_err(|e| format!("检查更新任务异常退出: {e}"))?
}

/// 下载更新安装包并做 SHA-256 校验（进度经 update:download-progress 事件回传），
/// 返回落地路径，供 install_update_and_restart 使用。
///
/// 必须 async + spawn_blocking：同步 command 跑在主线程，整个下载（十几 MB +
/// 校验文件 + SHA-256）会把 UI 卡死到下载结束（v0.5.7 实测「未响应」）。
#[tauri::command]
pub async fn download_update_installer(
    app: AppHandle,
    url: String,
    sha256_url: Option<String>,
    size: Option<u64>,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::updater::download_installer(&app, &url, sha256_url.as_deref(), size)
            .map(|p| p.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| format!("下载任务异常退出: {e}"))?
}

/// 运行已下载的安装包（静默安装 + 装完自动启动新版，见 installer 的 /LAUNCH 约定），
/// 随后退出当前进程。
///
/// 必须 async + spawn_blocking：同步 command 跑在主线程，静默安装期间 UI 冻结。
#[tauri::command]
pub async fn install_update_and_restart(app: AppHandle, path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::updater::run_installer(&path)?;
        // 留出安装器自解压初始化的时间，再退出当前实例
        std::thread::sleep(std::time::Duration::from_millis(500));
        log::info!("退出应用以完成更新安装");
        app.exit(0);
        Ok(())
    })
    .await
    .map_err(|e| format!("安装任务异常退出: {e}"))?
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
    db::set_setting(&conn, &key, &value).map_err(|e| e.to_string())?;
    Ok(())
}

/// 内置跳过目录名（与用户配置合并生效，见 `scanner::BUILTIN_SKIP_DIRS`）。
/// 设置页把这批「关键字」单独标出来，避免用户以为需要自己添加。
#[tauri::command]
pub fn get_builtin_skip_dirs() -> Vec<String> {
    scanner::BUILTIN_SKIP_DIRS.iter().map(|s| s.to_string()).collect()
}

// ---------- 自定义背景 ----------

/// 允许作为背景图的扩展名
const BG_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "gif"];

/// 自定义背景图最小边长（px）：低于此值铺满窗口会明显发糊
const MIN_BG_DIMENSION: u32 = 600;

/// 读取图片文件的像素宽高（png/jpeg/webp/bmp/gif），仅解析文件头不做完整解码
/// （避免引入图像解码依赖），无法识别的布局返回 None（调用方放行）。
fn image_dimensions(path: &std::path::Path) -> Option<(u32, u32)> {
    use std::io::Read;
    let mut data = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .read_to_end(&mut data)
        .ok()?;
    image_dimensions_bytes(&data)
}

fn image_dimensions_bytes(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < 33 {
        return None;
    }
    // PNG：8 字节签名 + IHDR（宽高为大端 16..24）
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        let w = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let h = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
        return Some((w, h));
    }
    // GIF：签名 + 逻辑屏幕尺寸（小端 6..10）
    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        let w = u16::from_le_bytes([data[6], data[7]]) as u32;
        let h = u16::from_le_bytes([data[8], data[9]]) as u32;
        return Some((w, h));
    }
    // BMP：BITMAPINFOHEADER 宽高在小端 18..26（高度可能为负 = 行序翻转，取绝对值）
    if data.starts_with(b"BM") {
        let w = i32::from_le_bytes([data[18], data[19], data[20], data[21]]);
        let h = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
        return Some((w.unsigned_abs(), h.unsigned_abs()));
    }
    // WebP：RIFF....WEBP，按 chunk 类型取尺寸
    if data.starts_with(b"RIFF") && data[8..12] == *b"WEBP" {
        return match &data[12..16] {
            // VP8X 扩展：画布尺寸 24 位小端，存的是「边长 - 1」
            b"VP8X" => {
                let w = u32::from_le_bytes([data[24], data[25], data[26], 0]) + 1;
                let h = u32::from_le_bytes([data[27], data[28], data[29], 0]) + 1;
                Some((w, h))
            }
            // VP8 有损：起始码后 14 位宽高（小端 2+2 字节，高 2 位是保留段）
            b"VP8 " => {
                let w = (u16::from_le_bytes([data[26], data[27]]) & 0x3fff) as u32;
                let h = (u16::from_le_bytes([data[28], data[29]]) & 0x3fff) as u32;
                Some((w, h))
            }
            // VP8L 无损：14 位宽高打包在 4 字节里（各 14 位，存「边长 - 1」）
            b"VP8L" => {
                let bits = u32::from_le_bytes([data[21], data[22], data[23], data[24]]);
                let w = (bits & 0x3fff) + 1;
                let h = ((bits >> 14) & 0x3fff) + 1;
                Some((w, h))
            }
            _ => None,
        };
    }
    // JPEG：扫描 marker 找 SOF 帧头（含尺寸）；SOF 前有 APP 段/EXIF，扫到 256KB 足够
    if data.starts_with(b"\xff\xd8") {
        let mut i = 2usize;
        while i + 4 <= data.len() {
            if data[i] != 0xff {
                i += 1;
                continue;
            }
            let marker = data[i + 1];
            // 填充字节与独立 marker 无段长度，跳过
            if marker == 0xff || (0xd0..=0xd7).contains(&marker) || marker == 0x01 {
                i += 2;
                continue;
            }
            // SOF0~3 / SOF9~11（基线/渐进/算术）帧头：len(2) + 精度(1) + 高(2, 大端) + 宽(2)
            if (0xc0..=0xcf).contains(&marker) && marker != 0xc4 && marker != 0xc8 && marker != 0xcc {
                if i + 9 > data.len() {
                    return None;
                }
                let h = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
                let w = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
                return Some((w, h));
            }
            let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
            i += 2 + len;
        }
    }
    None
}

/// 选择的自定义背景图复制到数据目录 {app_data_dir}/backgrounds/（文件名带时间戳，
/// 天然防缓存），返回协议访问用的文件名（前端拼 bg://file/{name}）。旧背景文件一并
/// 清理，不堆积。放数据目录而非缓存目录：Windows 存储感知/清理工具会清理缓存目录，
/// 背景图会莫名消失（前端自愈回默认背景并提示），用户数据目录才稳。
#[tauri::command]
pub fn set_background_image(app: AppHandle, path: String) -> Result<String, String> {
    let ext = std::path::Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    if !BG_EXTENSIONS.contains(&ext.as_str()) {
        return Err(err(codes::BG_FORMAT));
    }
    // 尺寸校验：分辨率过低铺满窗口会明显发糊；文件头无法解析时放行（不误杀特殊编码图）
    if let Some((w, h)) = image_dimensions(std::path::Path::new(&path)) {
        if w < MIN_BG_DIMENSION || h < MIN_BG_DIMENSION {
            return Err(err(codes::BG_TOO_SMALL));
        }
    }
    let dest_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| err1(codes::BG_SAVE_FAILED, "error", e))?
        .join("backgrounds");
    std::fs::create_dir_all(&dest_dir).map_err(|e| err1(codes::BG_SAVE_FAILED, "error", e))?;
    if let Ok(entries) = std::fs::read_dir(&dest_dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with("bg-") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let name = format!("bg-{stamp}.{ext}");
    std::fs::copy(&path, dest_dir.join(&name))
        .map_err(|e| err1(codes::BG_SAVE_FAILED, "error", e))?;
    Ok(name)
}

// ---------- 艺人分隔符 ----------

/// 单首曲目艺人拆分的变更（调整分隔符后返回给前端展示）
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArtistSplitChange {
    pub track_id: i64,
    pub title: String,
    pub old_artists: Vec<String>,
    pub new_artists: Vec<String>,
}

/// 当前启用的多艺人分隔符串（无设置时为默认全集），形如 ";&、&，,/"
#[tauri::command]
pub fn get_artist_separators(state: State<'_, AppState>) -> Result<String, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    Ok(db::get_setting(&conn, scanner::ARTIST_SEPARATORS_KEY)
        .map(|s| scanner::parse_separators(&s).into_iter().collect())
        .unwrap_or_else(|| scanner::SEPARATOR_CANDIDATES.iter().collect()))
}

/// 设置多艺人分隔符并立即按新分隔符重拆曲库，返回受影响曲目的变更列表。
/// raw_artist 尚未回填的曲目（待下次扫描完整解析）不在本次重拆范围内。
#[tauri::command]
pub fn set_artist_separators(
    state: State<'_, AppState>,
    value: String,
) -> Result<Vec<ArtistSplitChange>, String> {
    let seps = scanner::parse_separators(&value);
    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    db::set_setting(
        &conn,
        scanner::ARTIST_SEPARATORS_KEY,
        &seps.iter().collect::<String>(),
    )
    .map_err(|e| e.to_string())?;

    let tx = conn.transaction().map_err(|e| e.to_string())?;

    // 现有关联：track_id → [(artist_id, name)]（按 ord 排序）
    let mut current: HashMap<i64, Vec<(i64, String)>> = HashMap::new();
    {
        let mut stmt = tx
            .prepare(
                "SELECT ta.track_id, a.id, a.name FROM track_artists ta \
                 JOIN artists a ON a.id = ta.artist_id ORDER BY ta.track_id, ta.ord",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (tid, aid, name) = row.map_err(|e| e.to_string())?;
            current.entry(tid).or_default().push((aid, name));
        }
    }

    // 待重拆的曲目：已保存原始艺人标签的完整解析行
    struct Row {
        id: i64,
        title: String,
        raw: String,
    }
    let rows: Vec<Row> = {
        let mut stmt = tx
            .prepare("SELECT id, title, raw_artist FROM tracks WHERE raw_artist IS NOT NULL")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Row {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    raw: r.get(2)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };

    let mut changes: Vec<ArtistSplitChange> = Vec::new();
    // 完整缓存（含艺人别名）：重拆走与扫描一致的解析路径，合并过的旧名仍归到主艺人
    let mut caches = scanner::load_caches(&tx)?;
    for r in rows {
        let new_names = scanner::split_artists(&r.raw, &seps);
        let old_names: Vec<String> = current
            .get(&r.id)
            .map(|v| v.iter().map(|(_, n)| n.clone()).collect())
            .unwrap_or_default();
        // 与现有关联一致（大小写不敏感）则无需改动
        let same = old_names.len() == new_names.len()
            && old_names
                .iter()
                .zip(&new_names)
                .all(|(a, b)| a.eq_ignore_ascii_case(b));
        if same {
            continue;
        }

        let ids: Vec<i64> = new_names
            .iter()
            .map(|n| scanner::get_or_create_artist(&tx, &mut caches, n))
            .collect::<Result<Vec<i64>, String>>()?;
        tx.execute(
            "DELETE FROM track_artists WHERE track_id = ?1",
            params![r.id],
        )
        .map_err(|e| e.to_string())?;
        for (i, aid) in ids.iter().enumerate() {
            tx.execute(
                "INSERT OR IGNORE INTO track_artists (track_id, artist_id, ord) VALUES (?1, ?2, ?3)",
                params![r.id, aid, i as i64],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.execute(
            "UPDATE tracks SET artist_id = ?1 WHERE id = ?2",
            params![ids.first().copied().unwrap_or(0), r.id],
        )
        .map_err(|e| e.to_string())?;

        changes.push(ArtistSplitChange {
            track_id: r.id,
            title: r.title,
            old_artists: old_names,
            new_artists: new_names,
        });
    }

    // 回收不再被引用的孤儿艺人（与来源移除后的清理口径一致）
    tx.execute(
        "DELETE FROM artists WHERE id NOT IN (SELECT DISTINCT artist_id FROM tracks)
         AND id NOT IN (SELECT DISTINCT artist_id FROM track_artists)
         AND id NOT IN (SELECT DISTINCT artist_id FROM albums)",
        [],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;

    Ok(changes)
}

/// 艺人名规整变更：一行 = 一个被合并的艺人旧名 → 新名及其影响的曲目数
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArtistNormalizeChange {
    pub old_name: String,
    pub new_name: String,
    pub track_count: i64,
}

/// 一次性规整艺人名：剥离艺人名尾部括号注释（如「陈奕迅（Eason Chan）」→「陈奕迅」），
/// 并把曲目 / 专辑关联合并到规整名对应的同一位艺人。幂等：重复调用不产生新增变更。
/// 新扫描与分隔符重拆已自带规整，此命令用于清理已入库的历史数据。
#[tauri::command]
pub fn normalize_artist_names(
    state: State<'_, AppState>,
) -> Result<Vec<ArtistNormalizeChange>, String> {
    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    // 现有艺人：id → 名
    let mut rows: Vec<(i64, String)> = Vec::new();
    {
        let mut stmt = tx
            .prepare("SELECT id, name FROM artists")
            .map_err(|e| e.to_string())?;
        let it = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        for row in it {
            rows.push(row.map_err(|e| e.to_string())?);
        }
    }
    // 规整名（小写）→ 目标艺人 id。只登记「原名即规整名」的艺人：
    // 若括号名先入表，后到的规整名映射本应覆盖它；反过来（规整名先入表）则会让
    // 括号艺人「自己合并进自己」——拆光关联后删除艺人触发外键失败、整个事务回滚。
    // 两遍法保证目标永远是规整名艺人本体。
    let mut by_canon: HashMap<String, i64> = HashMap::new();
    for (id, name) in &rows {
        if scanner::canonical_artist(name) == *name {
            by_canon.insert(name.to_lowercase(), *id);
        }
    }
    // 已有别名（小写）→ 主艺人 id：规整名命中别名时直接落到主艺人，不再为其新建艺人。
    // INNER JOIN 过滤掉主艺人已被删除的悬空别名（此时按正常路径新建艺人）。
    let alias_to_artist: HashMap<String, i64> = {
        let mut stmt = tx
            .prepare(
                "SELECT aa.alias, aa.artist_id FROM artist_aliases aa \
                 JOIN artists a ON a.id = aa.artist_id",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<HashMap<_, _>, _>>()
            .map_err(|e| e.to_string())?
    };

    // 收集待合并：(旧 id, 目标 id, 旧名, 规整名)；先收集后迁移，避免中间态影响判定
    let mut pending: Vec<(i64, i64, String, String)> = Vec::new();
    for (id, name) in &rows {
        let canon = scanner::canonical_artist(name);
        if canon == *name {
            continue;
        }
        let key = canon.to_lowercase();
        let target: i64 = match by_canon.get(&key).copied() {
            Some(existing) => existing,
            None => match alias_to_artist.get(&key).copied() {
                Some(existing) => existing,
                None => {
                    tx.execute("INSERT OR IGNORE INTO artists (name) VALUES (?1)", [&canon])
                        .map_err(|e| e.to_string())?;
                    let nid: i64 = tx
                        .query_row(
                            "SELECT id FROM artists WHERE name = ?1 COLLATE NOCASE",
                            [&canon],
                            |r| r.get(0),
                        )
                        .map_err(|e| e.to_string())?;
                    by_canon.insert(key, nid);
                    nid
                }
            },
        };
        pending.push((*id, target, name.clone(), canon));
    }

    let mut changes: Vec<ArtistNormalizeChange> = Vec::new();
    // 撞 key 被合并的旧专辑：提交后清封面缓存（行已删，rowid 会被复用）
    let mut merged_album_ids: Vec<i64> = Vec::new();
    for (old, target, old_name, new_name) in pending {
        let (count, merged) = merge_artist_into(&tx, old, target)?;
        merged_album_ids.extend(merged);
        // 合并记忆：旧名 → 目标艺人。之后扫描（含改名后的标签）再遇到旧名，
        // 经别名解析仍归到目标艺人名下，不会重新建成独立艺人。
        tx.execute(
            "INSERT INTO artist_aliases (alias, artist_id) VALUES (?1, ?2) \
             ON CONFLICT(alias) DO UPDATE SET artist_id = excluded.artist_id",
            params![old_name, target],
        )
        .map_err(|e| e.to_string())?;
        changes.push(ArtistNormalizeChange {
            old_name,
            new_name,
            track_count: count,
        });
    }
    tx.commit().map_err(|e| e.to_string())?;
    if !merged_album_ids.is_empty() {
        crate::covers::purge(&state.covers_dir, &merged_album_ids);
    }

    Ok(changes)
}

/// 把 `source` 艺人并入 `target`（视为同一位艺人）：
/// - 曲目关联重挂到 target（同曲目已关联 target 的先拆掉，避免主键冲突）
/// - 专辑迁移；target 名下存在同名（不区分大小写）同年专辑时合并——
///   albums.key 唯一约束下直接 UPDATE 会失败，改为曲目并入目标专辑后删除旧专辑
/// - 迁移后 source 不再被任何引用则删除；其名下已有别名一并改指 target
/// - 返回 (受影响曲目数, 因合并被删除的专辑 id——调用方负责清理封面缓存)
fn merge_artist_into(
    tx: &rusqlite::Transaction,
    source: i64,
    target: i64,
) -> Result<(i64, Vec<i64>), String> {
    tx.execute(
        "DELETE FROM track_artists WHERE artist_id = ?1 \
         AND track_id IN (SELECT track_id FROM track_artists WHERE artist_id = ?2)",
        params![source, target],
    )
    .map_err(|e| e.to_string())?;
    let count: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM track_artists WHERE artist_id = ?1",
            [source],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    tx.execute(
        "UPDATE track_artists SET artist_id = ?1 WHERE artist_id = ?2",
        params![target, source],
    )
    .map_err(|e| e.to_string())?;

    let album_ids: Vec<i64> = {
        let mut stmt = tx
            .prepare("SELECT id FROM albums WHERE artist_id = ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([source], |r| r.get::<_, i64>(0))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    let mut merged_albums: Vec<i64> = Vec::new();
    for album_id in album_ids {
        let (title, year, has_cover): (String, Option<i64>, i64) = tx
            .query_row(
                "SELECT title, year, has_cover FROM albums WHERE id = ?1",
                [album_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(|e| e.to_string())?;
        let target_album: Option<i64> = tx
            .query_row(
                "SELECT id FROM albums WHERE artist_id = ?1 AND lower(title) = lower(?2) \
                 AND IFNULL(year, 0) = IFNULL(?3, 0)",
                params![target, title, year],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        match target_album {
            Some(target_album) => {
                tx.execute(
                    "UPDATE tracks SET album_id = ?1 WHERE album_id = ?2",
                    params![target_album, album_id],
                )
                .map_err(|e| e.to_string())?;
                tx.execute(
                    "UPDATE albums SET has_cover = max(has_cover, ?1) WHERE id = ?2",
                    params![has_cover, target_album],
                )
                .map_err(|e| e.to_string())?;
                tx.execute("DELETE FROM albums WHERE id = ?1", [album_id])
                    .map_err(|e| e.to_string())?;
                merged_albums.push(album_id);
            }
            None => {
                tx.execute(
                    "UPDATE albums SET artist_id = ?1 WHERE id = ?2",
                    params![target, album_id],
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }
    // 源艺人名下已有的别名改指 target（否则 target 将来再被并入他人时链条断裂）
    tx.execute(
        "UPDATE artist_aliases SET artist_id = ?1 WHERE artist_id = ?2",
        params![target, source],
    )
    .map_err(|e| e.to_string())?;
    // 源艺人已无任何引用：删除
    tx.execute(
        "DELETE FROM artists WHERE id = ?1 \
         AND NOT EXISTS (SELECT 1 FROM track_artists WHERE artist_id = ?1) \
         AND NOT EXISTS (SELECT 1 FROM albums WHERE artist_id = ?1)",
        [source],
    )
    .map_err(|e| e.to_string())?;
    Ok((count, merged_albums))
}

/// 艺人别名：旧名 → 主艺人（规整 / 自定义合并的合并记忆，设置页展示用）
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArtistAlias {
    pub alias: String,
    pub artist_id: i64,
    pub artist_name: String,
}

/// 列出全部合并记录（含历次规整与自定义合并），按主艺人名分组展示由前端处理
#[tauri::command]
pub fn list_artist_aliases(state: State<'_, AppState>) -> Result<Vec<ArtistAlias>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT aa.alias, a.id, a.name FROM artist_aliases aa \
             JOIN artists a ON a.id = aa.artist_id \
             ORDER BY a.name COLLATE PINYIN, aa.alias COLLATE PINYIN",
        )
        .map_err(|e| e.to_string())?;
    let items = stmt
        .query_map([], |r| {
            Ok(ArtistAlias {
                alias: r.get(0)?,
                artist_id: r.get(1)?,
                artist_name: r.get(2)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(items)
}

/// 自定义合并：把 source 艺人并入 target（用户认定两者是同一人，如改名 / 写法不同）。
/// 曲目与专辑关联全部迁移到 target，source 删除，并把 source 的名字记为别名——
/// 之后扫描再遇到这个名字（含改名前的旧标签）仍会归到 target 名下。
#[tauri::command]
pub fn merge_artist(
    state: State<'_, AppState>,
    source_id: i64,
    target_id: i64,
) -> Result<ArtistNormalizeChange, String> {
    if source_id == target_id {
        return Err(err(codes::ARTIST_MERGE_SAME));
    }
    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    let (source_name, target_name): (String, String) = {
        let source_name: Option<String> = conn
            .query_row(
                "SELECT name FROM artists WHERE id = ?1",
                params![source_id],
                |r| r.get(0),
            )
            .ok();
        let target_name: Option<String> = conn
            .query_row(
                "SELECT name FROM artists WHERE id = ?1",
                params![target_id],
                |r| r.get(0),
            )
            .ok();
        match (source_name, target_name) {
            (Some(s), Some(t)) => (s, t),
            _ => return Err(err(codes::ARTIST_NOT_FOUND)),
        }
    };

    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let (count, merged_album_ids) = merge_artist_into(&tx, source_id, target_id)?;
    tx.execute(
        "INSERT INTO artist_aliases (alias, artist_id) VALUES (?1, ?2) \
         ON CONFLICT(alias) DO UPDATE SET artist_id = excluded.artist_id",
        params![source_name, target_id],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    if !merged_album_ids.is_empty() {
        crate::covers::purge(&state.covers_dir, &merged_album_ids);
    }

    Ok(ArtistNormalizeChange {
        old_name: source_name,
        new_name: target_name,
        track_count: count,
    })
}

// ================================================================ 曲库移除与记录

/// 已移除歌曲记录（设置 → 已移除歌曲 展示用）
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RemovedTrack {
    pub id: i64,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub path: String,
    /// 'manual' = 手动从曲库移除；'scan' = 扫描发现文件消失
    pub reason: String,
    pub removed_at: i64,
}

/// 已移除歌曲列表（最新在前，最多 500 条；上限 1000 条自动裁剪）
#[tauri::command]
pub fn list_removed_tracks(state: State<'_, AppState>) -> Result<Vec<RemovedTrack>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, title, artist, album, path, reason, removed_at FROM removed_tracks \
             ORDER BY removed_at DESC, id DESC LIMIT 500",
        )
        .map_err(|e| e.to_string())?;
    let items = stmt
        .query_map([], |r| {
            Ok(RemovedTrack {
                id: r.get(0)?,
                title: r.get(1)?,
                artist: r.get(2)?,
                album: r.get(3)?,
                path: r.get(4)?,
                reason: r.get(5)?,
                removed_at: r.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(items)
}

/// 清空已移除歌曲记录
#[tauri::command]
pub fn clear_removed_tracks(state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM removed_tracks", [])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 还原结果：restored = 已接受还原的记录数（文件仍在，来源扫描会重新入库）；
/// missing = 文件已不存在 / 来源已删除的记录数
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RemovedRestoreResult {
    pub restored: usize,
    pub missing: usize,
}

/// 还原已移除的歌曲：确认文件仍在（本地查磁盘 / WebDAV 探测头部 1 字节），
/// 删除对应记录并对来源触发一次增量扫描——增量管线发现「库里没有、磁盘上有」
/// 会按标准路径重新入库（元数据/歌词/封面全走既有逻辑）。来源正在扫描时不重复
/// 触发（进行中的扫描本就会把它捞回来）。
#[tauri::command]
pub fn restore_removed_tracks(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<i64>,
) -> Result<RemovedRestoreResult, String> {
    if ids.is_empty() {
        return Ok(RemovedRestoreResult {
            restored: 0,
            missing: 0,
        });
    }
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    /// 移除记录行：(记录 id, 相对路径, 来源 id, 来源类型, 本地根目录, WebDAV 根地址)
    type RestoreRow = (
        i64,
        String,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    // 记录 + 来源信息（来源已删除的按 missing 处理）
    let rows: Vec<RestoreRow> = {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT r.id, r.path, r.source_id, s.kind, s.base_path, s.base_url \
                 FROM removed_tracks r \
                 LEFT JOIN sources s ON s.id = r.source_id \
                 WHERE r.id IN ({placeholders})"
            ))
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params_from_iter(ids.iter()), |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    drop(conn);

    let mut restorable: Vec<i64> = Vec::new(); // 可还原的记录 id（删记录）
    let mut missing = 0usize;
    let mut sources_to_scan: Vec<i64> = Vec::new();
    for (id, path, source_id, kind, base_path, base_url) in &rows {
        let available = match kind.as_deref() {
            Some("local") => base_path
                .as_ref()
                .map(|base| std::path::Path::new(base).join(path).is_file())
                .unwrap_or(false),
            Some("webdav") => {
                // 探测头部 1 字节：2xx 即存在（404/403 等视为不可还原）
                let probe = || -> Option<bool> {
                    let base = crate::network::webdav::normalize_base(base_url.as_ref()?).ok()?;
                    let url = crate::network::webdav::file_url(&base, path);
                    let auth = {
                        let conn = state.db.lock().ok()?;
                        let config: Option<String> = conn
                            .query_row(
                                "SELECT config FROM sources WHERE id = ?1",
                                params![source_id],
                                |r| r.get(0),
                            )
                            .ok();
                        crate::network::webdav::Auth::from_source(config.as_deref(), *source_id)
                    };
                    Some(
                        crate::network::webdav::download_short(&url, auth.as_ref(), Some((0, 0)))
                            .is_ok(),
                    )
                }()
                .unwrap_or(false);
                probe
            }
            _ => false,
        };
        if available {
            restorable.push(*id);
            if !sources_to_scan.contains(source_id) {
                sources_to_scan.push(*source_id);
            }
        } else {
            missing += 1;
        }
    }

    if !restorable.is_empty() {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let placeholders = restorable.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        conn.execute(
            &format!("DELETE FROM removed_tracks WHERE id IN ({placeholders})"),
            params_from_iter(restorable.iter()),
        )
        .map_err(|e| e.to_string())?;
        drop(conn);
        // 触发来源增量扫描（正在扫描中的来源跳过：进行中的扫描本就会重新入库）
        for source_id in sources_to_scan {
            let _ = spawn_scan(&app, &state, source_id, false);
        }
    }
    Ok(RemovedRestoreResult {
        restored: restorable.len(),
        missing,
    })
}

/// 从曲库移除指定曲目（不删除磁盘上的文件）：清理子表引用、回收孤儿专辑/艺人并
/// 清理封面缓存，同时写入移除记录（reason='manual'）。返回实际移除数量。
#[tauri::command]
pub fn remove_tracks(state: State<'_, AppState>, ids: Vec<i64>) -> Result<usize, String> {
    let mut ids = ids;
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Ok(0);
    }
    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let removed_at: i64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let mut removed = 0usize;
    /// 移除记录行：(标题, 艺人, 专辑, 路径, 来源 id)
    type RemovedInfo = (String, Option<String>, Option<String>, String, i64);
    for chunk in ids.chunks(500) {
        // 移除前先取展示信息写记录
        let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let infos: Vec<RemovedInfo> = {
            let mut stmt = tx
                .prepare(&format!(
                    "SELECT t.title, a.name, al.title, t.path, t.source_id FROM tracks t \
                     LEFT JOIN artists a ON a.id = t.artist_id \
                     LEFT JOIN albums al ON al.id = t.album_id \
                     WHERE t.id IN ({placeholders})"
                ))
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params_from_iter(chunk.iter()), |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })
                .map_err(|e| e.to_string())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?
        };
        for (title, artist, album, path, source_id) in &infos {
            tx.execute(
                "INSERT INTO removed_tracks (title, artist, album, path, source_id, reason, removed_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, 'manual', ?6)",
                params![title, artist, album, path, source_id, removed_at],
            )
            .map_err(|e| e.to_string())?;
        }
        // 子表引用清理（曲目删除由外键级联，但显式删除保证旧库兼容）
        for table in [
            "track_artists",
            "playlist_items",
            "lrc_files",
            "lyrics_index",
        ] {
            tx.execute(
                &format!("DELETE FROM {table} WHERE track_id IN ({placeholders})"),
                params_from_iter(chunk.iter()),
            )
            .map_err(|e| e.to_string())?;
        }
        removed += tx
            .execute(
                &format!("DELETE FROM tracks WHERE id IN ({placeholders})"),
                params_from_iter(chunk.iter()),
            )
            .map_err(|e| e.to_string())?;
    }

    // 回收孤儿专辑（先收集再删，与 purge 集合完全一致）与孤儿艺人
    let orphan_albums: Vec<i64> = {
        let mut stmt = tx
            .prepare("SELECT id FROM albums WHERE id NOT IN (SELECT DISTINCT album_id FROM tracks)")
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
    tx.execute(
        "DELETE FROM artists WHERE id NOT IN (SELECT DISTINCT artist_id FROM tracks)
         AND id NOT IN (SELECT DISTINCT artist_id FROM track_artists)
         AND id NOT IN (SELECT DISTINCT artist_id FROM albums)",
        [],
    )
    .map_err(|e| e.to_string())?;
    db::cap_removed_log(&tx).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;

    if !orphan_albums.is_empty() {
        crate::covers::purge(&state.covers_dir, &orphan_albums);
    }
    Ok(removed)
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
    let host = base
        .host_str()
        .map(|h| h.to_string())
        .unwrap_or_else(|| url.clone());
    let display_name = name.unwrap_or_else(|| format!("WebDAV ({host})"));
    let auth = crate::network::webdav::Auth { username, password };
    // 连通性验证：列根目录
    crate::network::webdav::list_dir(&base, Some(&auth))?;

    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let exists: Option<i64> = conn
        .query_row(
            "SELECT id FROM sources WHERE kind = 'webdav' AND base_url = ?1",
            params![base.as_str()],
            |r| r.get(0),
        )
        .ok();
    if exists.is_some() {
        return Err(err(codes::SOURCE_DUPLICATE_URL));
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
        // 安全相关：明文回退必须留痕（日志只记事件，绝不记凭证内容）
        log::warn!("WebDAV 凭证写入系统钥匙串失败，source {id} 回退明文存库");
        let fallback =
            serde_json::json!({ "username": auth.username, "password": auth.password }).to_string();
        let _ = conn.execute(
            "UPDATE sources SET config = ?1 WHERE id = ?2",
            params![fallback, id],
        );
    }
    drop(conn);
    log::info!("已添加 WebDAV 来源 {id}: {}", base.as_str());

    spawn_scan(&app, &state, id, false)?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.query_row(
        &format!("{SOURCE_SELECT} WHERE s.id = ?1"),
        params![id],
        row_source,
    )
    .map_err(|e| e.to_string())
}

// ================================================================ MV 播放

/// 获取视频文件的流 URL（用于播放 MV）；同名视频文件的探测与 video:// 协议共用
/// `scheme::find_local_mv`，避免两处各维护一份扩展名/stem 逻辑
#[tauri::command]
pub fn get_mv_url(
    _app: AppHandle,
    state: State<'_, AppState>,
    track_id: i64,
) -> Result<Option<String>, String> {
    let (source_kind, base_path, track_path): (String, Option<String>, String) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT s.kind, s.base_path, t.path FROM tracks t JOIN sources s ON s.id = t.source_id WHERE t.id = ?1",
            [track_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| e.to_string())?
    };

    // MV 探测只对本地来源生效（WebDAV 曲目没有 MV）
    if source_kind != "local" {
        return Ok(None);
    }
    let Some(base) = base_path else {
        return Ok(None);
    };
    if crate::scheme::find_local_mv(&base, &track_path).is_none() {
        return Ok(None);
    }
    // 返回自定义协议 URL
    Ok(Some(if cfg!(windows) {
        format!("http://video.localhost/mv/{track_id}")
    } else {
        format!("video://mv/{track_id}")
    }))
}

#[cfg(test)]
mod bg_dimension_tests {
    use super::*;

    #[test]
    fn png_size_from_ihdr() {
        let mut b = b"\x89PNG\r\n\x1a\n".to_vec();
        b.extend_from_slice(&[0, 0, 0, 13]);
        b.extend_from_slice(b"IHDR");
        b.extend_from_slice(&1920u32.to_be_bytes());
        b.extend_from_slice(&1080u32.to_be_bytes());
        b.extend_from_slice(&[8, 6, 0, 0, 0]);
        b.extend_from_slice(&[0, 0, 0, 0]); // CRC（仅补足 33 字节头）
        assert_eq!(image_dimensions_bytes(&b), Some((1920, 1080)));
    }

    #[test]
    fn gif_size_from_logical_screen() {
        let mut b = b"GIF89a".to_vec();
        b.extend_from_slice(&400u16.to_le_bytes());
        b.extend_from_slice(&300u16.to_le_bytes());
        b.extend_from_slice(&[0u8; 23]); // 补足 33 字节头
        assert_eq!(image_dimensions_bytes(&b), Some((400, 300)));
    }

    #[test]
    fn jpeg_size_scans_past_app_segments() {
        let mut b = vec![0xff, 0xd8];
        // APP1(EXIF) 段：marker + len=16 + 14 字节载荷
        b.extend_from_slice(&[0xff, 0xe1, 0x00, 0x10]);
        b.extend_from_slice(&[0u8; 14]);
        // SOF0：marker + len=17 + 精度 + 高 1080 + 宽 1920 + 分量数
        b.extend_from_slice(&[0xff, 0xc0, 0x00, 0x11, 0x08, 0x04, 0x38, 0x07, 0x80, 0x03]);
        b.extend_from_slice(&[0u8; 3]); // 补足 33 字节头
        assert_eq!(image_dimensions_bytes(&b), Some((1920, 1080)));
    }

    #[test]
    fn webp_vp8x_size() {
        let mut b = b"RIFF\x00\x00\x00\x00WEBP".to_vec();
        b.extend_from_slice(b"VP8X");
        b.extend_from_slice(&[10, 0, 0, 0]); // chunk size
        b.extend_from_slice(&[0, 0, 0, 0]); // reserved
        b.extend_from_slice(&[191, 7, 0]); // 宽-1（小端 24 位）= 1983
        b.extend_from_slice(&[127, 4, 0]); // 高-1 = 1151
        b.extend_from_slice(&[0, 0, 0]); // 补足 33 字节头
        assert_eq!(image_dimensions_bytes(&b), Some((1984, 1152)));
    }

    #[test]
    fn too_short_buffer_is_none() {
        assert_eq!(image_dimensions_bytes(&[0u8; 10]), None);
    }
}
