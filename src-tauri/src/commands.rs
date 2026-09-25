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
    /// 艺人字段命中的艺人 id（含按合并别名 / 拼音命中者），仅搜索时非空——
    /// 前端据此只给真正命中的艺人上主题色，合作艺人不受牵连
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub matched_artist_ids: Vec<i64>,
    /// ReplayGain 轨道增益（dB）；标签缺失或未分析为 null
    pub rg_track_gain: Option<f64>,
    /// ReplayGain 轨道峰值（线性）；标签缺失或未分析为 null
    pub rg_track_peak: Option<f64>,
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

/// 文件夹视图：一个子目录（相对来源根目录的路径）
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderItem {
    /// 相对路径（以 / 分隔），作为进入下一级的 parent 参数
    pub path: String,
    /// 目录名（最后一段）
    pub name: String,
    /// 该目录（含子目录）下的曲目总数
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
     t.disc_no, t.duration, t.bitrate, t.sample_rate, t.bit_depth, t.format, t.path, t.has_embedded_lyrics, t.has_mv, t.fav, \
     t.rg_track_gain, t.rg_track_peak \
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
        matched_artist_ids: Vec::new(),
        rg_track_gain: r.get(17)?,
        rg_track_peak: r.get(18)?,
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
    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    let kind: Option<String> = conn
        .query_row("SELECT kind FROM sources WHERE id = ?1", params![id], |r| {
            r.get(0)
        })
        .ok();
    // 先删曲目再删来源。现库 schema 的 tracks.source_id 带 ON DELETE CASCADE（连接均开启
    // 外键），删来源本可级联清曲目；这里改为显式删除，让子表清理真实执行——既兼容外键
    // 未生效的异常连接，也让每条 DELETE 都作用于真实存在的行，而不是恒为空的子查询。
    // 主删除链 + 孤儿专辑/艺人清理整体入事务：任一步失败全部回滚，不会停在
    // 「曲目删了来源还在」之类的半删除态（对齐 remove_tracks 的口径）。
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM track_artists WHERE track_id IN (SELECT id FROM tracks WHERE source_id = ?1)",
        params![id],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM playlist_items WHERE track_id IN (SELECT id FROM tracks WHERE source_id = ?1)",
        params![id],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM lrc_files WHERE track_id IN (SELECT id FROM tracks WHERE source_id = ?1)",
        params![id],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM lyrics_index WHERE track_id IN (SELECT id FROM tracks WHERE source_id = ?1)",
        params![id],
    )
    .map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM tracks WHERE source_id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM sources WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    // 曲目删除后在同一事务里删孤儿专辑（先收集 id，提交后按它清缓存）
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
    if !orphan_albums.is_empty() {
        let placeholders = orphan_albums
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!("DELETE FROM albums WHERE id IN ({placeholders})");
        tx.execute(&sql, rusqlite::params_from_iter(orphan_albums.iter()))
            .map_err(|e| e.to_string())?;
    }
    // 专辑归属艺人（albums.artist_id）可能没有直接归属的曲目，删除时需一并排除
    tx.execute(
        "DELETE FROM artists WHERE id NOT IN (SELECT DISTINCT artist_id FROM tracks)
         AND id NOT IN (SELECT DISTINCT artist_id FROM track_artists)
         AND id NOT IN (SELECT DISTINCT artist_id FROM albums)",
        [],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    drop(conn);

    // 事务已提交、DB 行已删：按收集到的 id 清理封面缓存（与 remove_tracks 同序——
    // 先 DB 后文件；缓存可重建， purge 失败只影响下次加载速度）
    if !orphan_albums.is_empty() {
        crate::covers::purge(&state.covers_dir, &orphan_albums);
    }

    // 清理收尾：webdav 来源移除钥匙串凭证；本地来源停止目录监听
    match kind.as_deref() {
        Some("webdav") => crate::keyring::delete_password(id),
        Some("local") => crate::watcher::unwatch_source(&app, id),
        _ => {}
    }
    // 破坏性操作留痕：日志里能看到来源何时被删（曲目、歌单引用随之清除）
    log::info!(
        "已删除来源 {id}（{}）",
        kind.as_deref().unwrap_or("未知类型")
    );
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
    // 最近播放最多展示 500 首（用户定稿）：内层先取最近 500 首，再对窗口分页；
    // total 同步封顶，翻页越过 500 后自然返回空页。其余视图不限制。
    const RECENT_LIMIT: i64 = 500;
    let is_recent = q.view.as_deref() == Some("recent");

    let count_sql = if is_recent {
        format!(
            "SELECT MIN(cnt, {RECENT_LIMIT}) FROM (SELECT COUNT(*) AS cnt FROM tracks t LEFT JOIN artists a ON a.id = t.artist_id LEFT JOIN albums al ON al.id = t.album_id {where_sql})"
        )
    } else {
        format!(
            "SELECT COUNT(*) FROM tracks t LEFT JOIN artists a ON a.id = t.artist_id LEFT JOIN albums al ON al.id = t.album_id {where_sql}"
        )
    };
    let total: i64 = conn
        .query_row(
            &count_sql,
            params_from_iter(args.iter().map(|b| b.as_ref())),
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;

    let sql = if is_recent {
        format!(
            "SELECT * FROM ({TRACK_SELECT} {where_sql} {order_sql} LIMIT {RECENT_LIMIT}) LIMIT {page_size} OFFSET {}",
            page * page_size
        )
    } else {
        format!(
            "{TRACK_SELECT} {where_sql} {order_sql} LIMIT {page_size} OFFSET {}",
            page * page_size
        )
    };
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
    query_albums_conn(&conn, search, page, page_size)
}

fn query_albums_conn(
    conn: &rusqlite::Connection,
    search: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
) -> Result<Page<AlbumItem>, String> {
    let page = page.unwrap_or(0) as i64;
    let page_size = page_size.unwrap_or(200).clamp(1, 500) as i64;
    let offset = page * page_size;

    let like = search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{s}%"));
    // 合并记忆：搜旧艺人的别名也能命中其名下专辑
    let where_sql = if like.is_some() {
        "WHERE al.title LIKE ?1 OR IFNULL(a.name,'') LIKE ?1 \
         OR EXISTS (SELECT 1 FROM artist_aliases aa WHERE aa.artist_id = al.artist_id AND aa.alias LIKE ?1)"
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
    query_artists_conn(&conn, search, page, page_size)
}

fn query_artists_conn(
    conn: &rusqlite::Connection,
    search: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
) -> Result<Page<ArtistItem>, String> {
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
    // 合并记忆：搜旧艺人的别名也能命中合并后的主艺人（悬空别名的 artist_id 无对应行，不会误中）
    let where_sql = if like.is_some() {
        format!(
            "WHERE {base_where} AND (ar.name LIKE ?1 \
             OR EXISTS (SELECT 1 FROM artist_aliases aa WHERE aa.artist_id = ar.id AND aa.alias LIKE ?1))"
        )
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

/// 文件夹视图：列出 `parent` 目录（相对来源根路径，空串/None = 根）下的**直接子目录**及曲目数。
/// 前端用返回的 path 作为进入下一级的 parent，配合面包屑逐级浏览。
#[tauri::command]
pub fn query_folders(
    state: State<'_, AppState>,
    parent: Option<String>,
) -> Result<Vec<FolderItem>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let parent = parent.unwrap_or_default();
    // rest = 去掉 parent 前缀后的相对路径；child = rest 的第一段（无 '/' 表示该曲目直接位于 parent 中）。
    // LIKE 模式必须转义通配符：含 %/_ 的合法目录名（如 "90's_Hits"）会跨目录误匹配
    let pattern = format!("{}/%", crate::search::like_escape(&parent));
    let sql = "SELECT child, COUNT(*) FROM ( \
                   SELECT CASE WHEN instr(rest, '/') > 0 THEN substr(rest, 1, instr(rest, '/') - 1) ELSE '' END AS child \
                   FROM (SELECT CASE WHEN ?1 = '' THEN t.path ELSE substr(t.path, length(?1) + 2) END AS rest \
                         FROM tracks t WHERE (?1 = '' OR t.path LIKE ?2 ESCAPE '\\')) \
               ) WHERE child <> '' GROUP BY child ORDER BY child COLLATE PINYIN";
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![parent, pattern], |r| {
            let name: String = r.get(0)?;
            let count: i64 = r.get(1)?;
            let path = if parent.is_empty() {
                name.clone()
            } else {
                format!("{parent}/{name}")
            };
            Ok(FolderItem {
                path,
                name,
                track_count: count,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut items = Vec::new();
    for r in rows {
        items.push(r.map_err(|e| e.to_string())?);
    }
    Ok(items)
}

/// 文件夹视图：`folder` 目录下**直接存放**的曲目（不含子目录）。
#[tauri::command]
pub fn query_tracks_by_folder(
    state: State<'_, AppState>,
    folder: Option<String>,
) -> Result<Vec<Track>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let folder = folder.unwrap_or_default();
    // LIKE 模式转义同 query_folders：含 %/_ 的合法目录名不能跨目录误匹配
    let pattern = format!("{}/%", crate::search::like_escape(&folder));
    let sql = format!(
        "{TRACK_SELECT} WHERE (?1 = '' OR t.path LIKE ?2 ESCAPE '\\') \
         AND instr(CASE WHEN ?1 = '' THEN t.path ELSE substr(t.path, length(?1) + 2) END, '/') = 0 \
         ORDER BY t.path COLLATE PINYIN"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let mut items: Vec<Track> = stmt
        .query_map(params![folder, pattern], row_track)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    attach_artists(&conn, &mut items)?;
    Ok(items)
}

/// 写入某曲目的响度分析结果（ReplayGain 轨道增益 dB + 峰值）。
/// 分析在前端用 Web Audio 完成（见 src/composables/useLoudness.ts），此处只负责落库。
#[tauri::command]
pub fn save_loudness(
    state: State<'_, AppState>,
    id: i64,
    gain_db: f64,
    peak: f64,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE tracks SET rg_track_gain = ?1, rg_track_peak = ?2 WHERE id = ?3",
        params![gain_db, peak, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 启用 / 禁用系统媒体键（注册为全局快捷键，见 media_controls.rs；其他平台空操作）。
#[tauri::command]
pub fn media_controls_enable(app: AppHandle, enabled: bool) -> Result<Vec<String>, String> {
    Ok(crate::media_controls::set_enabled(&app, enabled))
}

// —— 系统级「正在播放」（souvlaki，见 now_playing.rs）——

/// 前端推送的当前曲目元数据（字段名经 camelCase 转换）
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NpMeta {
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_ms: Option<u64>,
    pub album_id: Option<i64>,
}

/// SMTC 无封面时的占位图：应用图标（编译期写死，首用落盘到 covers/_placeholder.png）。
/// covers 的两处清理（purge / enforce_limit）只动 `{id}.jpg` 与 `{id}.none`，占位图不受影响。
const NP_PLACEHOLDER_PNG: &[u8] = include_bytes!("../icons/128x128@2x.png");

/// 占位图 URL（落盘一次、复用；写失败返回 None，届时系统侧维持原图——可接受的兜底）
fn placeholder_cover_url(covers_dir: &std::path::Path) -> Option<String> {
    let p = covers_dir.join("_placeholder.png");
    if !p.is_file() {
        std::fs::write(&p, NP_PLACEHOLDER_PNG).ok()?;
    }
    Some(format!("file://{}", p.display()))
}

/// 封面快路径：只用已落盘的缓存 `covers/{album_id}.jpg`（一次 stat，零网络零提取锁）。
/// 没有现成缓存时由调用方决定「占位 + 后台提取」（见 now_playing_set）。
/// URL 拼法三平台统一（见 now_playing.rs 模块注释）：`file://` + 路径原样。
fn np_cover_url(covers_dir: &std::path::Path, album_id: Option<i64>) -> Option<String> {
    let id = album_id?;
    let jpg = covers_dir.join(format!("{id}.jpg"));
    jpg.is_file().then(|| format!("file://{}", jpg.display()))
}

/// 推送当前曲目元数据（切歌时调用；传 null 清空 → 系统侧 Stopped）。
///
/// 封面策略（修「切歌后系统浮层一直显示上一首封面」）：
/// - 缓存已有 → 随首推直接带上；
/// - 缓存未就绪 → 先推**应用图标占位**（souvlaki 不带封面时不清系统侧缩略图，
///   上一首的封面会一直挂着），同时后台调 `ensure_cover` 提取：300ms 内完成则
///   改推真封面（本地封面通常走这条，用户无感）；超时（远程 WebDAV 常见）由
///   后台任务等提取结果，成功且仍是当前曲目时补推（`refresh_cover` 按 album_id
///   严格校验，切歌后到期的补推自动丢弃）；
/// - 确认无封面（`.none` 哨兵）→ 占位即终态。
/// set_metadata 会阻塞等封面文件加载（见 now_playing.rs），必须 spawn_blocking。
#[tauri::command]
pub async fn now_playing_set(app: AppHandle, meta: Option<NpMeta>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(m) = meta else {
            return crate::now_playing::set_metadata(None, None);
        };
        let covers_dir = app.state::<AppState>().covers_dir.clone();
        // 快路径：封面已缓存
        if let Some(url) = m
            .album_id
            .and_then(|id| np_cover_url(&covers_dir, Some(id)))
        {
            return crate::now_playing::set_metadata(Some(m), Some(url));
        }
        // 无现成缓存：先推占位图，绝不让上一首的缩略图滞留
        let placeholder = placeholder_cover_url(&covers_dir);
        if let Some(id) = m.album_id {
            if !covers_dir.join(format!("{id}.none")).is_file() {
                let (tx, rx) = std::sync::mpsc::channel();
                let app2 = app.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(crate::covers::ensure_cover(&app2, id));
                });
                const FAST_WAIT: std::time::Duration = std::time::Duration::from_millis(300);
                match rx.recv_timeout(FAST_WAIT) {
                    // 本地封面极快：等到了直接随首推带上（用户无感，无占位闪烁）
                    Ok(Ok(Some(path))) => {
                        return crate::now_playing::set_metadata(
                            Some(m),
                            Some(format!("file://{}", path.display())),
                        );
                    }
                    // 确认无封面 / 提取失败：占位即终态
                    Ok(_) => {}
                    // 提取仍在进行（远程封面常见）：后台等结果，仍是当前曲目时补推
                    Err(_) => {
                        let meta2 = m.clone();
                        tauri::async_runtime::spawn_blocking(move || {
                            if let Ok(Ok(Some(path))) = rx.recv() {
                                if let Err(e) = crate::now_playing::refresh_cover(
                                    id,
                                    meta2,
                                    Some(format!("file://{}", path.display())),
                                ) {
                                    log::warn!("补推封面到系统媒体控件失败: {e}");
                                }
                            }
                        });
                    }
                }
            }
        }
        crate::now_playing::set_metadata(Some(m), placeholder)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 推送播放状态与进度（播放暂停变化即时调用 + 前端 1Hz 心跳）
#[tauri::command]
pub async fn now_playing_state(playing: bool, position_ms: u64) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::now_playing::set_state(playing, position_ms)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 启用 / 禁用系统媒体控件（设置开关调用；重新启用会恢复上次的元数据显示）
#[tauri::command]
pub async fn now_playing_enable(app: AppHandle, enabled: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || crate::now_playing::enable(&app, enabled))
        .await
        .map_err(|e| e.to_string())?
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
    // 必须与主窗口/托盘窗口同参（见 lib.rs WEBVIEW2_BROWSER_ARGS）：同一用户数据
    // 目录下主窗口已带参运行，再以默认参数建歌词窗会被 WebView2 拒绝——
    // 0x8007139F「组或资源的状态不是执行请求操作的正确状态」（v0.5.20 起回归）
    #[cfg(windows)]
    let builder = builder.additional_browser_args(crate::WEBVIEW2_BROWSER_ARGS);
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

// ---------- 听歌统计 ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenSummary {
    pub total_seconds: i64,
    pub total_plays: i64,
    pub today_seconds: i64,
    pub week_seconds: i64,
    /// 曲库累计播放次数（tracks.play_count 总和，含本版本之前的历史）
    pub total_track_plays: i64,
    /// 有效播放次数：本版本起，同一曲目同一天只计一次
    pub effective_plays: i64,
    /// 收听过的独立曲目 / 艺人 / 专辑数
    pub unique_tracks: i64,
    pub unique_artists: i64,
    pub unique_albums: i64,
    pub first_listened: Option<i64>,
    pub last_listened: Option<i64>,
    /// 有收听记录的自然日数（前端据此算日均/周均/月均）
    pub listen_days: i64,
}

/// 榜单统一行：kind=track/artist/album/genre 时 id/name/album_id 语义随 kind 变化
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenTopItem {
    pub kind: String,
    pub id: i64,
    pub name: String,
    pub artist: Option<String>,
    pub album_id: Option<i64>,
    pub seconds: i64,
    pub plays: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenDailyPoint {
    pub day: String,
    pub seconds: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenHourPoint {
    pub hour: String,
    pub seconds: i64,
}

/// 星期 × 小时热力格子（dow：0=周日 … 6=周六；hour：0-23，本地时区）
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenHeatCell {
    pub dow: i64,
    pub hour: i64,
    pub seconds: i64,
}

/// 记录一段实际收听（前端 player 心跳/结算：暂停、快进跳过的不计秒）。
/// seconds 钳制到 [1, 3600]，防前端异常写入脏数据；曲目不存在时外键约束自然拒绝。
/// mode 为收听时的播放模式（order/loop/one/shuffle），占比统计用；老数据为 NULL。
#[tauri::command]
pub fn report_listen(
    state: State<'_, AppState>,
    track_id: i64,
    seconds: i64,
    mode: Option<String>,
) -> Result<(), String> {
    let seconds = seconds.clamp(1, 3600);
    let mode = mode.filter(|m| matches!(m.as_str(), "order" | "loop" | "one" | "shuffle"));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO play_history (track_id, played_at, seconds, mode) VALUES (?1, ?2, ?3, ?4)",
        params![track_id, now, seconds, mode],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 汇总：总收听时长 / 片段数 / 今日 / 近 7 天（本地时区自然日）+
/// 曲库累计播放次数 / 有效播放次数（一曲一天一次）/ 独立曲目·艺人·专辑数 /
/// 首末收听时间 / 有收听记录的自然日数。
/// play_history 全表聚合在重度用户（数十万行）上可能超 100ms：放 spawn_blocking。
#[tauri::command]
pub async fn listen_stats_summary(app: AppHandle) -> Result<ListenSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        listen_stats_summary_impl(&app.state::<AppState>())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn listen_stats_summary_impl(state: &AppState) -> Result<ListenSummary, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let (total_seconds, total_plays, today_seconds, week_seconds) = conn
        .query_row(
            "SELECT COALESCE(SUM(seconds),0), COUNT(*), \
             COALESCE(SUM(CASE WHEN played_at >= CAST(strftime('%s','now','localtime','start of day') AS INTEGER) THEN seconds ELSE 0 END),0), \
             COALESCE(SUM(CASE WHEN played_at >= CAST(strftime('%s','now','localtime','-6 days','start of day') AS INTEGER) THEN seconds ELSE 0 END),0) \
             FROM play_history",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(|e| e.to_string())?;
    let (total_track_plays, listen_days) = conn
        .query_row(
            "SELECT (SELECT COALESCE(SUM(play_count),0) FROM tracks), \
             (SELECT COUNT(DISTINCT date(played_at,'unixepoch','localtime')) FROM play_history)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    let effective_plays: i64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT track_id || '-' || strftime('%Y%m%d', played_at,'unixepoch','localtime')) FROM play_history",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let (unique_tracks, unique_artists, unique_albums) = conn
        .query_row(
            "SELECT COUNT(DISTINCT h.track_id), \
             COUNT(DISTINCT IFNULL(t.artist_id, -1)), \
             COUNT(DISTINCT IFNULL(t.album_id, -1)) \
             FROM play_history h JOIN tracks t ON t.id = h.track_id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| e.to_string())?;
    let (first_listened, last_listened) = conn
        .query_row(
            "SELECT MIN(played_at), MAX(played_at) FROM play_history",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    Ok(ListenSummary {
        total_seconds,
        total_plays,
        today_seconds,
        week_seconds,
        total_track_plays,
        effective_plays,
        unique_tracks,
        unique_artists,
        unique_albums,
        first_listened,
        last_listened,
        listen_days,
    })
}

/// 收听榜单：kind = track | artist | album | genre；range = week(近 7 天滚动) | month(近 30 天) | all。
/// 按累计收听秒数降序（并列按片段数）。
#[tauri::command]
pub fn listen_top_tracks(
    state: State<'_, AppState>,
    kind: Option<String>,
    range: String,
    limit: Option<i64>,
) -> Result<Vec<ListenTopItem>, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let since: Option<i64> = match range.as_str() {
        "week" => Some(now - 7 * 86400),
        "month" => Some(now - 30 * 86400),
        _ => None,
    };
    let limit = limit.unwrap_or(20).clamp(1, 100);
    let kind = kind.unwrap_or_else(|| "track".into());
    let base = "FROM play_history h \
                JOIN tracks t ON t.id = h.track_id \
                LEFT JOIN artists a ON a.id = t.artist_id \
                LEFT JOIN albums al ON al.id = t.album_id \
                WHERE (?1 IS NULL OR h.played_at >= ?1)";
    let (select_sql, group_sql) = match kind.as_str() {
        "artist" => (
            "SELECT t.artist_id, IFNULL(a.name,''), NULL, MAX(t.album_id), SUM(h.seconds), COUNT(*)",
            " GROUP BY t.artist_id",
        ),
        "album" => (
            "SELECT t.album_id, IFNULL(al.title,''), NULL, t.album_id, SUM(h.seconds), COUNT(*)",
            " GROUP BY t.album_id",
        ),
        "genre" => (
            "SELECT 0, TRIM(t.genre), NULL, MAX(t.album_id), SUM(h.seconds), COUNT(*)",
            " GROUP BY TRIM(t.genre)",
        ),
        _ => (
            "SELECT h.track_id, t.title, a.name, t.album_id, SUM(h.seconds), COUNT(*)",
            " GROUP BY h.track_id",
        ),
    };
    let extra_where = match kind.as_str() {
        "artist" => " AND t.artist_id IS NOT NULL",
        "album" => " AND t.album_id IS NOT NULL",
        "genre" => " AND TRIM(IFNULL(t.genre,'')) != ''",
        _ => "",
    };
    let sql =
        format!("{select_sql} {base}{extra_where}{group_sql} ORDER BY 5 DESC, 6 DESC LIMIT ?2");
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let items = stmt
        .query_map(params![since, limit], |r| {
            Ok(ListenTopItem {
                kind: kind.clone(),
                id: r.get(0)?,
                name: r.get(1)?,
                artist: r.get(2)?,
                album_id: r.get(3)?,
                seconds: r.get(4)?,
                plays: r.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(items)
}

/// 收听趋势：granularity = day(默认) | week | month，days 为回看天数（滚动窗口，本地时区）。
/// week/month 直接返回非空桶（前端按序画图即可）。
#[tauri::command]
pub fn listen_daily(
    state: State<'_, AppState>,
    days: Option<i64>,
    granularity: Option<String>,
) -> Result<Vec<ListenDailyPoint>, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let since = now - days.unwrap_or(30).clamp(1, 3650) * 86400;
    let bucket = match granularity.as_deref() {
        Some("week") => "strftime('%Y-%W', played_at,'unixepoch','localtime')",
        Some("month") => "strftime('%Y-%m', played_at,'unixepoch','localtime')",
        _ => "date(played_at,'unixepoch','localtime')",
    };
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {bucket} d, SUM(seconds) \
             FROM play_history WHERE played_at >= ?1 \
             GROUP BY d ORDER BY d"
        ))
        .map_err(|e| e.to_string())?;
    let items = stmt
        .query_map(params![since], |r| {
            Ok(ListenDailyPoint {
                day: r.get(0)?,
                seconds: r.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(items)
}

/// 24 小时收听分布（本地时区），全量聚合；前端负责补齐 0-23 小时空桶。
#[tauri::command]
pub fn listen_hourly(state: State<'_, AppState>) -> Result<Vec<ListenHourPoint>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT strftime('%H', played_at,'unixepoch','localtime') h, SUM(seconds) \
             FROM play_history GROUP BY h ORDER BY h",
        )
        .map_err(|e| e.to_string())?;
    let items = stmt
        .query_map([], |r| {
            Ok(ListenHourPoint {
                hour: r.get(0)?,
                seconds: r.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(items)
}

/// 星期 × 小时收听热力图（本地时区）。dow：0=周日 … 6=周六；前端负责铺满 7×24 空格。
#[tauri::command]
pub fn listen_heatmap(state: State<'_, AppState>) -> Result<Vec<ListenHeatCell>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT CAST(strftime('%w', played_at,'unixepoch','localtime') AS INTEGER) dow, \
             CAST(strftime('%H', played_at,'unixepoch','localtime') AS INTEGER) hh, \
             SUM(seconds) \
             FROM play_history GROUP BY dow, hh",
        )
        .map_err(|e| e.to_string())?;
    let items = stmt
        .query_map([], |r| {
            Ok(ListenHeatCell {
                dow: r.get(0)?,
                hour: r.get(1)?,
                seconds: r.get(2)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(items)
}

/// civil date（YYYY-MM-DD）→ 天数序号（Howard Hinnant 算法），用于连续天数推算
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe
}

fn civil_to_days(s: &str) -> Option<i64> {
    let mut it = s.split('-');
    let y = it.next()?.parse::<i64>().ok()?;
    let m = it.next()?.parse::<i64>().ok()?;
    let d = it.next()?.parse::<i64>().ok()?;
    Some(days_from_civil(y, m, d))
}

/// 连续听歌天数：current = 最近一次连续段（最后收听日为今天或昨天才有效，否则 0）；
/// longest = 历史最长连续收听天数。均按本地时区自然日。
/// play_history 全表扫描+排序，放 spawn_blocking（同 listen_stats_summary）。
#[tauri::command]
pub async fn listen_streak(app: AppHandle) -> Result<(i64, i64), String> {
    tauri::async_runtime::spawn_blocking(move || listen_streak_impl(&app.state::<AppState>()))
        .await
        .map_err(|e| e.to_string())?
}

fn listen_streak_impl(state: &AppState) -> Result<(i64, i64), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut days: Vec<i64> = conn
        .prepare(
            "SELECT DISTINCT date(played_at,'unixepoch','localtime') FROM play_history ORDER BY 1",
        )
        .map_err(|e| e.to_string())?
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?
        .iter()
        .filter_map(|s| civil_to_days(s))
        .collect();
    days.sort_unstable();
    days.dedup();
    if days.is_empty() {
        return Ok((0, 0));
    }
    let mut longest = 0i64;
    let mut cur = 0i64;
    let mut prev: Option<i64> = None;
    for d in &days {
        cur = match prev {
            Some(p) if d - p == 1 => cur + 1,
            _ => 1,
        };
        longest = longest.max(cur);
        prev = Some(*d);
    }
    // 当前连续：最后收听日必须是今天或昨天（本地时区），否则连续已中断
    let today = conn
        .query_row("SELECT date('now','localtime')", [], |r| {
            r.get::<_, String>(0)
        })
        .map_err(|e| e.to_string())?;
    let today_days = civil_to_days(&today).unwrap_or(0);
    let last = *days.last().unwrap();
    let current = if today_days - last <= 1 { cur } else { 0 };
    Ok((current, longest))
}

/// 收听占比查询已移除（占比卡片下线）；play_history.mode 列保留采集，便于后续分析。

// ---------- 音乐库体检 ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NameCount {
    pub name: String,
    pub count: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YearCount {
    pub year: i64,
    pub count: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaCoverage {
    pub title: i64,
    pub artist: i64,
    pub album: i64,
    pub year: i64,
    pub genre: i64,
    pub track_no: i64,
    pub total: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryHealth {
    pub tracks: i64,
    pub albums: i64,
    pub artists: i64,
    pub genres: i64,
    pub total_seconds: f64,
    pub total_size: i64,
    pub size_known: i64,
    pub formats: Vec<NameCount>,
    pub sources: Vec<NameCount>,
    pub years: Vec<YearCount>,
    pub sample_rates: Vec<NameCount>,
    pub bit_depths: Vec<NameCount>,
    pub lyrics_embedded: i64,
    pub lyrics_external: i64,
    pub lyrics_qrc: i64,
    pub tracks_with_lyrics: i64,
    pub albums_with_cover: i64,
    pub tracks_with_cover: i64,
    pub mv_count: i64,
    pub meta: MetaCoverage,
    pub meta_incomplete: i64,
}

/// 音乐库体检：曲库构成、格式/来源/年份/码率分布、歌词/封面/MV/元数据覆盖率、疑似重复。
/// 多次全表聚合，放 spawn_blocking（同 listen_stats_summary）。
#[tauri::command]
pub async fn library_health(app: AppHandle) -> Result<LibraryHealth, String> {
    tauri::async_runtime::spawn_blocking(move || library_health_impl(&app.state::<AppState>()))
        .await
        .map_err(|e| e.to_string())?
}

fn library_health_impl(state: &AppState) -> Result<LibraryHealth, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let q1 = |sql: &str| -> Result<i64, String> {
        conn.query_row(sql, [], |r| r.get(0))
            .map_err(|e| e.to_string())
    };
    let name_counts = |sql: &str| -> Result<Vec<NameCount>, String> {
        let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok(NameCount {
                    name: r.get(0)?,
                    count: r.get(1)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(rows)
    };

    let tracks = q1("SELECT COUNT(*) FROM tracks")?;
    let albums = q1("SELECT COUNT(*) FROM albums")?;
    let artists = q1("SELECT COUNT(*) FROM artists")?;
    let genres = q1("SELECT COUNT(DISTINCT NULLIF(TRIM(genre),'')) FROM tracks")?;
    let (total_seconds, total_size, size_known): (f64, i64, i64) = conn
        .query_row(
            "SELECT COALESCE(SUM(duration),0), COALESCE(SUM(file_size),0), COUNT(file_size) FROM tracks",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| e.to_string())?;

    let formats = name_counts(
        "SELECT IFNULL(format,''), COUNT(*) FROM tracks GROUP BY format ORDER BY 2 DESC",
    )?;
    let sources = name_counts(
        "SELECT s.kind, COUNT(*) FROM tracks t JOIN sources s ON s.id = t.source_id GROUP BY s.kind ORDER BY 2 DESC",
    )?;
    let sample_rates = name_counts(
        "SELECT CAST(sample_rate AS TEXT), COUNT(*) FROM tracks WHERE sample_rate IS NOT NULL GROUP BY sample_rate ORDER BY 1",
    )?;
    let bit_depths = name_counts(
        "SELECT CAST(bit_depth AS TEXT), COUNT(*) FROM tracks WHERE bit_depth IS NOT NULL GROUP BY bit_depth ORDER BY 1",
    )?;
    let years: Vec<YearCount> = {
        let mut stmt = conn
            .prepare("SELECT year, COUNT(*) FROM tracks WHERE year IS NOT NULL GROUP BY year ORDER BY year")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok(YearCount {
                    year: r.get(0)?,
                    count: r.get(1)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows
    };

    let lyrics_embedded = q1("SELECT COUNT(*) FROM tracks WHERE has_embedded_lyrics = 1")?;
    let lyrics_external =
        q1("SELECT COUNT(*) FROM lrc_files WHERE path NOT LIKE '%.qrc' AND path LIKE '%.%'")?;
    let lyrics_qrc = q1("SELECT COUNT(*) FROM lrc_files WHERE path LIKE '%.qrc'")?;
    let tracks_with_lyrics =
        q1("SELECT COUNT(*) FROM tracks WHERE has_embedded_lyrics = 1 OR EXISTS (SELECT 1 FROM lrc_files WHERE lrc_files.track_id = tracks.id)")?;
    let albums_with_cover = q1("SELECT COUNT(*) FROM albums WHERE has_cover = 1")?;
    let tracks_with_cover = q1(
        "SELECT COUNT(*) FROM tracks t JOIN albums al ON al.id = t.album_id WHERE al.has_cover = 1",
    )?;
    let mv_count = q1("SELECT COUNT(*) FROM tracks WHERE has_mv = 1")?;
    let meta = conn
        .query_row(
            "SELECT COUNT(title), COUNT(artist_id), COUNT(album_id), COUNT(year), \
             COUNT(NULLIF(TRIM(genre),'')), COUNT(track_no), COUNT(*) FROM tracks",
            [],
            |r| {
                Ok(MetaCoverage {
                    title: r.get(0)?,
                    artist: r.get(1)?,
                    album: r.get(2)?,
                    year: r.get(3)?,
                    genre: r.get(4)?,
                    track_no: r.get(5)?,
                    total: r.get(6)?,
                })
            },
        )
        .map_err(|e| e.to_string())?;
    let meta_incomplete = q1("SELECT COUNT(*) FROM tracks WHERE meta_state = 0")?;

    Ok(LibraryHealth {
        tracks,
        albums,
        artists,
        genres,
        total_seconds,
        total_size,
        size_known,
        formats,
        sources,
        years,
        sample_rates,
        bit_depths,
        lyrics_embedded,
        lyrics_external,
        lyrics_qrc,
        tracks_with_lyrics,
        albums_with_cover,
        tracks_with_cover,
        mv_count,
        meta,
        meta_incomplete,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanHistoryItem {
    pub at: i64,
    pub source_name: String,
    pub added: i64,
    pub updated: i64,
    pub removed: i64,
    pub ms: i64,
}

/// 最近几次扫描的历史（来源名 + 增删改数量 + 耗时）。
#[tauri::command]
pub fn scan_history_list(state: State<'_, AppState>) -> Result<Vec<ScanHistoryItem>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT h.at, IFNULL(s.name,''), h.added, h.updated, h.removed, h.ms \
             FROM scan_history h LEFT JOIN sources s ON s.id = h.source_id \
             ORDER BY h.at DESC, h.id DESC LIMIT 10",
        )
        .map_err(|e| e.to_string())?;
    let items = stmt
        .query_map([], |r| {
            Ok(ScanHistoryItem {
                at: r.get(0)?,
                source_name: r.get(1)?,
                added: r.get(2)?,
                updated: r.get(3)?,
                removed: r.get(4)?,
                ms: r.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(items)
}

#[tauri::command]
pub async fn get_lyrics(app: AppHandle, id: i64) -> Result<Option<String>, String> {
    // WebDAV 外挂歌词/内嵌头部拉取是网络 IO（单次总超时 60s，最多串 3 次）：
    // 同步 command 跑主线程，断网/限流时会冻结整个 UI 消息泵（窗口、SMTC、
    // 全部 IPC 全卡）——必须 spawn_blocking
    tauri::async_runtime::spawn_blocking(move || crate::lyrics::fetch(&app, id))
        .await
        .map_err(|e| e.to_string())?
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
    if level == "error" {
        // 诊断：前端错误计数
        crate::diagnostics::FRONTEND_ERRORS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    match level.as_str() {
        "error" => log::error!("[前端] {message}"),
        "warn" => log::warn!("[前端] {message}"),
        _ => log::info!("[前端] {message}"),
    }
}

// ---------- 性能与诊断 ----------

#[tauri::command]
pub fn report_play_latency(ms: u64) {
    if ms <= 60_000 {
        crate::diagnostics::play_latency(ms);
    }
}

#[tauri::command]
pub async fn diagnostics_snapshot() -> Result<crate::diagnostics::DiagnosticsSnapshot, String> {
    // sysinfo 进程采样是重量级调用（Windows 上可达百毫秒级），必须挪出主线程：
    // 同步 command 跑主线程会阻塞 UI（含 GSAP 过渡），诊断页轮询期间页面切换会掉帧卡顿
    tauri::async_runtime::spawn_blocking(crate::diagnostics::snapshot)
        .await
        .map_err(|e| e.to_string())
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
            Ok(Some(info)) => {
                log::info!("检查更新：发现新版本 {}（当前 {}）", info.version, current)
            }
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
    scanner::BUILTIN_SKIP_DIRS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

// ---------- 全局快捷键（设置 → 通用 → 快捷键，见 global_shortcuts.rs） ----------

/// 全局快捷键绑定：action 为前端动作名（toggle/next/prev），shortcut 为热键描述串
/// （如 "CommandOrControl+Alt+P"，解析规则见 global-hotkey crate）
#[derive(Deserialize)]
pub struct GlobalShortcutBinding {
    pub action: String,
    pub shortcut: String,
}

/// 整体替换注册全局快捷键：先注销旧注册再逐一注册；
/// 任一失败（通常为组合已被其他程序占用）则全部回滚并返回错误，供前端提示。
#[tauri::command]
pub fn global_shortcut_apply(
    app: AppHandle,
    bindings: Vec<GlobalShortcutBinding>,
) -> Result<(), String> {
    crate::global_shortcuts::apply(
        &app,
        bindings
            .into_iter()
            .map(|b| (b.shortcut, b.action))
            .collect(),
    )
}

/// 注销本应用注册的全部全局快捷键（关闭全局快捷键开关时调用）
#[tauri::command]
pub fn global_shortcut_clear(app: AppHandle) -> Result<(), String> {
    crate::global_shortcuts::clear(&app)
}

/// 检测快捷键是否已被本应用注册（true = 已注册）
#[tauri::command]
pub fn global_shortcut_is_registered(app: AppHandle, shortcut: String) -> Result<bool, String> {
    crate::global_shortcuts::is_registered(&app, &shortcut)
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
    // 只读文件头（256KB 足够 PNG/GIF/BMP/WebP 与绝大多数 JPEG 的 SOF 段），
    // 不把整张图读进内存——本函数跑在同步 command 主线程上，100MB 位图就是
    // 100MB 的磁盘读 + 内存分配；截断时下方解析自然返回 None（放行逻辑兜底）
    let mut data = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(256 * 1024)
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
            if (0xc0..=0xcf).contains(&marker) && marker != 0xc4 && marker != 0xc8 && marker != 0xcc
            {
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
/// 文件头解析 + 目录清理 + 文件复制都是磁盘 IO：整体放 spawn_blocking，不占主线程。
#[tauri::command]
pub async fn set_background_image(app: AppHandle, path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || set_background_image_impl(&app, path))
        .await
        .map_err(|e| e.to_string())?
}

fn set_background_image_impl(app: &AppHandle, path: String) -> Result<String, String> {
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
/// 全库 track_artists 重拆（读全部关联行 + 逐行重写）万曲级可秒级，期间 db 锁
/// 被占、其他 IPC 全部排队——放 spawn_blocking 不占主线程。
#[tauri::command]
pub async fn set_artist_separators(
    app: AppHandle,
    value: String,
) -> Result<Vec<ArtistSplitChange>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        set_artist_separators_impl(&state, value)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn set_artist_separators_impl(
    state: &AppState,
    value: String,
) -> Result<Vec<ArtistSplitChange>, String> {
    let seps = scanner::parse_separators(&value);
    let mut conn = state.db.lock().map_err(|e| e.to_string())?;

    let tx = conn.transaction().map_err(|e| e.to_string())?;

    // 分隔符设置与重拆在同一个事务里落库：重拆失败回滚时设置一并回滚，
    // 不会出现「设置已是新值、存量数据还是旧拆法」的永久不一致
    db::set_setting(
        &tx,
        scanner::ARTIST_SEPARATORS_KEY,
        &seps.iter().collect::<String>(),
    )
    .map_err(|e| e.to_string())?;

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
/// - 快照合并历史（旧名 → 当时的曲目关联 / 主艺人归属 / 专辑归属），供「取消合并」拆回
/// - 曲目关联重挂到 target（同曲目已关联 target 的先拆掉，避免主键冲突）；
///   曲目主艺人冗余列 tracks.artist_id 同样外键引用 artists，一并改挂
/// - 专辑迁移；target 名下存在同名（不区分大小写）同年专辑时合并——
///   albums.key 唯一约束下直接 UPDATE 会失败，改为曲目并入目标专辑后删除旧专辑
/// - 迁移后 source 不再被任何引用则删除；其名下已有别名一并改指 target
/// - 返回 (受影响曲目数, 因合并被删除的专辑 id——调用方负责清理封面缓存)
fn merge_artist_into(
    tx: &rusqlite::Transaction,
    source: i64,
    target: i64,
) -> Result<(i64, Vec<i64>), String> {
    // 历史快照必须在任何改写之前：否则拆不回改写前的状态。
    // 同一旧名重复合并以最近一次为准（INSERT OR REPLACE）。
    let now: i64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let source_name: String = tx
        .query_row(
            "SELECT name FROM artists WHERE id = ?1",
            params![source],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    // 曲目关联快照：track_artists 行（ord 保留）+ 是否为该曲目的主艺人
    tx.execute(
        "INSERT OR REPLACE INTO artist_merge_history (alias, track_id, was_primary, ord, merged_at) \
         SELECT ?1, ta.track_id, (t.artist_id = ?2), ta.ord, ?3 \
         FROM track_artists ta JOIN tracks t ON t.id = ta.track_id WHERE ta.artist_id = ?2",
        params![source_name, source, now],
    )
    .map_err(|e| e.to_string())?;
    // 仅有主艺人列指向（无 track_artists 行）的曲目也纳入快照
    tx.execute(
        "INSERT OR REPLACE INTO artist_merge_history (alias, track_id, was_primary, ord, merged_at) \
         SELECT ?1, id, 1, 0, ?3 FROM tracks WHERE artist_id = ?2 \
         AND id NOT IN (SELECT track_id FROM track_artists WHERE artist_id = ?2)",
        params![source_name, source, now],
    )
    .map_err(|e| e.to_string())?;
    // 专辑归属快照（取消合并时只恢复仍存在且仍在 target 名下的专辑）
    tx.execute(
        "INSERT OR REPLACE INTO artist_merge_albums (alias, album_id, merged_at) \
         SELECT ?1, id, ?3 FROM albums WHERE artist_id = ?2",
        params![source_name, source, now],
    )
    .map_err(|e| e.to_string())?;

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
    tx.execute(
        "UPDATE tracks SET artist_id = ?1 WHERE artist_id = ?2",
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
    // 源艺人已无任何引用：删除（tracks/track_artists/albums 三处外键全部改挂后才满足）
    tx.execute(
        "DELETE FROM artists WHERE id = ?1 \
         AND NOT EXISTS (SELECT 1 FROM track_artists WHERE artist_id = ?1) \
         AND NOT EXISTS (SELECT 1 FROM tracks WHERE artist_id = ?1) \
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
    /// 合并历史里仍可拆回的曲目数（历史记录引入前的旧合并为 0）
    pub restorable_tracks: i64,
}

/// 列出全部合并记录（含历次规整与自定义合并），按主艺人名分组展示由前端处理
#[tauri::command]
pub fn list_artist_aliases(state: State<'_, AppState>) -> Result<Vec<ArtistAlias>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT aa.alias, a.id, a.name, \
             (SELECT COUNT(*) FROM artist_merge_history h WHERE h.alias = aa.alias) \
             FROM artist_aliases aa \
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
                restorable_tracks: r.get(3)?,
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

/// 取消合并结果
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArtistUnmergeResult {
    pub artist_id: i64,
    pub artist_name: String,
    /// 拆回关联的曲目数（含合作艺人关联）
    pub restored_tracks: i64,
    /// 从 target 名下拆回的专辑数
    pub restored_albums: i64,
}

/// 取消合并：把旧名 `alias` 恢复为独立艺人。
/// - 重建（或复用同名）艺人，按合并历史快照拆回曲目关联与主艺人归属；
///   仍存在且仍在 target 名下的专辑一并拆回
/// - 删除该别名（合并记忆）与对应历史；target 若因此不再被任何引用则删除
/// - 历史记录引入前的旧合并没有快照：只恢复艺人本身与合并记忆，已有曲目
///   重新扫描后按标签归位
#[tauri::command]
pub fn unmerge_artist(
    state: State<'_, AppState>,
    alias: String,
) -> Result<ArtistUnmergeResult, String> {
    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    unmerge_artist_conn(&mut conn, &alias)
}

fn unmerge_artist_conn(
    conn: &mut rusqlite::Connection,
    alias: &str,
) -> Result<ArtistUnmergeResult, String> {
    let alias = alias.trim().to_string();
    if alias.is_empty() {
        return Err(err(codes::ARTIST_ALIAS_NOT_FOUND));
    }
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    let target: Option<i64> = tx
        .query_row(
            "SELECT artist_id FROM artist_aliases WHERE alias = ?1",
            params![alias],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if target.is_none() {
        return Err(err(codes::ARTIST_ALIAS_NOT_FOUND));
    }

    // 重建旧名艺人（同名艺人已存在则复用）
    tx.execute(
        "INSERT OR IGNORE INTO artists (name) VALUES (?1)",
        params![alias],
    )
    .map_err(|e| e.to_string())?;
    let new_id: i64 = tx
        .query_row(
            "SELECT id FROM artists WHERE name = ?1 COLLATE NOCASE",
            params![alias],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;

    // 按快照拆回：曲目关联（含 ord）；主艺人列；仍在 target 名下的专辑。
    // 曲目/专辑被移除时历史行已随外键级联清理，这里只处理仍存在的。
    tx.execute(
        "INSERT OR IGNORE INTO track_artists (track_id, artist_id, ord) \
         SELECT track_id, ?2, ord FROM artist_merge_history WHERE alias = ?1",
        params![alias, new_id],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "UPDATE tracks SET artist_id = ?2 \
         WHERE id IN (SELECT track_id FROM artist_merge_history WHERE alias = ?1 AND was_primary = 1)",
        params![alias, new_id],
    )
    .map_err(|e| e.to_string())?;
    // 拆回关联的曲目数 = 历史里仍指向现存曲目的行（曲目被移除时已级联清理）
    let restored_tracks: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM artist_merge_history WHERE alias = ?1",
            params![alias],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let restored_albums: i64 = match target {
        Some(t) => tx
            .execute(
                "UPDATE albums SET artist_id = ?2 \
                 WHERE artist_id = ?3 AND id IN (SELECT album_id FROM artist_merge_albums WHERE alias = ?1)",
                params![alias, new_id, t],
            )
            .map_err(|e| e.to_string())? as i64,
        None => 0,
    };

    // 清理合并记忆与历史；target 若因此不再被引用则删除
    tx.execute(
        "DELETE FROM artist_aliases WHERE alias = ?1",
        params![alias],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM artist_merge_history WHERE alias = ?1",
        params![alias],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM artist_merge_albums WHERE alias = ?1",
        params![alias],
    )
    .map_err(|e| e.to_string())?;
    if let Some(t) = target {
        tx.execute(
            "DELETE FROM artists WHERE id = ?1 \
             AND NOT EXISTS (SELECT 1 FROM track_artists WHERE artist_id = ?1) \
             AND NOT EXISTS (SELECT 1 FROM tracks WHERE artist_id = ?1) \
             AND NOT EXISTS (SELECT 1 FROM albums WHERE artist_id = ?1)",
            [t],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;

    Ok(ArtistUnmergeResult {
        artist_id: new_id,
        artist_name: alias,
        restored_tracks,
        restored_albums,
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
/// WebDAV 探测是逐条网络请求（单条 10s 超时）：同步 command 跑主线程，勾 30 条
/// 不可达记录会冻结 UI 最长 300s——整体放 spawn_blocking。
#[tauri::command]
pub async fn restore_removed_tracks(
    app: AppHandle,
    ids: Vec<i64>,
) -> Result<RemovedRestoreResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        restore_removed_tracks_impl(&app, &state, ids)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn restore_removed_tracks_impl(
    app: &AppHandle,
    state: &AppState,
    ids: Vec<i64>,
) -> Result<RemovedRestoreResult, String> {
    if ids.is_empty() {
        return Ok(RemovedRestoreResult {
            restored: 0,
            missing: 0,
        });
    }
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
    // 记录 + 来源信息（来源已删除的按 missing 处理）；IN 列表分批
    // （SQLite 变量上限防御，对齐 remove_tracks 的 500/批）
    let rows: Vec<RestoreRow> = {
        let mut out: Vec<RestoreRow> = Vec::new();
        for chunk in ids.chunks(500) {
            let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT r.id, r.path, r.source_id, s.kind, s.base_path, s.base_url \
                     FROM removed_tracks r \
                     LEFT JOIN sources s ON s.id = r.source_id \
                     WHERE r.id IN ({placeholders})"
                ))
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params_from_iter(chunk.iter()), |r| {
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
            out.extend(
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(|e| e.to_string())?,
            );
        }
        out
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
        // DELETE 同样分批，与上方 SELECT 的 500/批一致
        for chunk in restorable.chunks(500) {
            let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            conn.execute(
                &format!("DELETE FROM removed_tracks WHERE id IN ({placeholders})"),
                params_from_iter(chunk.iter()),
            )
            .map_err(|e| e.to_string())?;
        }
        drop(conn);
        // 触发来源增量扫描（正在扫描中的来源跳过：进行中的扫描本就会重新入库）
        for source_id in sources_to_scan {
            let _ = spawn_scan(app, state, source_id, false);
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

/// 添加 WebDAV 来源（NAS），验证连通性后扫描。
/// 连通性验证是 PROPFIND 网络请求（http_client 总超时 60s）：同步 command 跑主线程，
/// 用户输入不可达的 NAS 地址点「添加」会冻结 UI 整整一分钟——整体放 spawn_blocking。
#[tauri::command]
pub async fn webdav_add_source(
    app: AppHandle,
    url: String,
    username: String,
    password: String,
    name: Option<String>,
) -> Result<Source, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        webdav_add_source_impl(&app, &state, url, username, password, name)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn webdav_add_source_impl(
    app: &AppHandle,
    state: &AppState,
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

    spawn_scan(app, state, id, false)?;
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
mod merge_artist_tests {
    use super::merge_artist_into;
    use rusqlite::Connection;

    fn conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        conn.execute_batch(crate::db::SCHEMA).unwrap();
        conn
    }

    #[test]
    fn merge_repoints_tracks_and_albums_then_deletes_source() {
        let mut conn = conn();
        // source(1)「旧艺人」与 target(2)「新艺人」各一张同名同年专辑；
        // T1 主艺人是旧艺人（tracks.artist_id = 1，缺失改挂时删艺人触发外键失败），
        // T3 同时关联两位（合并时旧艺人的关联先拆掉）；旧艺人另有一条既有别名。
        conn.execute_batch(
            "INSERT INTO sources (id, name) VALUES (1, 's');
             INSERT INTO artists (id, name) VALUES (1, '旧艺人'), (2, '新艺人');
             INSERT INTO albums (id, title, artist_id, year, key) VALUES
               (1, '专辑', 1, 2020, 'k1'), (2, '专辑', 2, 2020, 'k2');
             INSERT INTO tracks (id, source_id, path, title, artist_id, album_id) VALUES
               (1, 1, 'p1', 'T1', 1, 1),
               (2, 1, 'p2', 'T2', 2, 2),
               (3, 1, 'p3', 'T3', 2, 2);
             INSERT INTO track_artists (track_id, artist_id, ord) VALUES
               (1, 1, 0), (2, 2, 0), (3, 2, 0), (3, 1, 1);
             INSERT INTO artist_aliases (alias, artist_id) VALUES ('旧名2', 1);",
        )
        .unwrap();

        let (count, merged) = {
            let tx = conn.transaction().unwrap();
            let r = merge_artist_into(&tx, 1, 2).unwrap();
            tx.commit().unwrap();
            r
        };
        // 预拆后仅剩 T1 一条关联重挂
        assert_eq!(count, 1);
        // 同名同年专辑并入目标专辑后删除
        assert_eq!(merged, vec![1]);

        // 源艺人已删除（修复点：tracks.artist_id 未改挂时这里报 FOREIGN KEY constraint failed）
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM artists WHERE id = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(n, 0);
        // 曲目主艺人列全部改挂 target
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM tracks WHERE artist_id = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(n, 0);
        // T1 并入目标专辑；T3 只剩 target 一条关联
        let (aid, alid): (i64, i64) = conn
            .query_row(
                "SELECT artist_id, album_id FROM tracks WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((aid, alid), (2, 2));
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM track_artists WHERE track_id = 3 AND artist_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
        // 专辑只剩 target 一张；既有别名改指 target
        let (n, artist): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), IFNULL((SELECT artist_id FROM albums WHERE id = 2), 0) FROM albums",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((n, artist), (1, 2));
        let artist: i64 = conn
            .query_row(
                "SELECT artist_id FROM artist_aliases WHERE alias = '旧名2'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(artist, 2);
    }

    #[test]
    fn merge_moves_unique_album_and_deletes_source() {
        let mut conn = conn();
        // source 名下专辑与 target 不重名 → 整张专辑改挂 target，不发生专辑合并
        conn.execute_batch(
            "INSERT INTO sources (id, name) VALUES (1, 's');
             INSERT INTO artists (id, name) VALUES (1, '旧艺人'), (2, '新艺人');
             INSERT INTO albums (id, title, artist_id, year, key) VALUES (1, '独特专辑', 1, 2021, 'k1');
             INSERT INTO tracks (id, source_id, path, title, artist_id, album_id) VALUES
               (1, 1, 'p1', 'T1', 1, 1);
             INSERT INTO track_artists (track_id, artist_id, ord) VALUES (1, 1, 0);",
        )
        .unwrap();

        let (count, merged) = {
            let tx = conn.transaction().unwrap();
            let r = merge_artist_into(&tx, 1, 2).unwrap();
            tx.commit().unwrap();
            r
        };
        assert_eq!(count, 1);
        assert!(merged.is_empty());
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM artists WHERE id = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(n, 0);
        let (aid, alid): (i64, i64) = conn
            .query_row(
                "SELECT artist_id, album_id FROM tracks WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((aid, alid), (2, 1));
        let artist: i64 = conn
            .query_row("SELECT artist_id FROM albums WHERE id = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(artist, 2);
    }
}

#[cfg(test)]
mod alias_search_tests {
    use super::*;

    fn conn() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        crate::db::register_collations(&conn).unwrap();
        conn.execute_batch(crate::db::SCHEMA).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn
    }

    /// 合并后的状态：旧艺人已删、曲目/专辑归「新艺人」、旧名记为别名
    fn seed(conn: &rusqlite::Connection) {
        conn.execute_batch(
            "INSERT INTO sources (id, name) VALUES (1, 's');
             INSERT INTO artists (id, name) VALUES (2, '新艺人');
             INSERT INTO albums (id, title, artist_id, year, key) VALUES
               (1, '专辑', 2, 2020, 'k1');
             INSERT INTO tracks (id, source_id, path, title, artist_id, album_id) VALUES
               (1, 1, 'p1', 'T1', 2, 1);
             INSERT INTO track_artists (track_id, artist_id, ord) VALUES (1, 2, 0);
             INSERT INTO artist_aliases (alias, artist_id) VALUES ('旧艺人', 2);",
        )
        .unwrap();
    }

    #[test]
    fn query_artists_matches_merged_alias() {
        let conn = conn();
        seed(&conn);
        // 搜旧名命中合并后的主艺人
        let page = query_artists_conn(&conn, Some("旧艺人".into()), None, None).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].name, "新艺人");
        // 搜本名同样命中；无搜索词的列表不受别名影响（不产生重复项）
        let page = query_artists_conn(&conn, Some("新艺人".into()), None, None).unwrap();
        assert_eq!(page.total, 1);
        let page = query_artists_conn(&conn, None, None, None).unwrap();
        assert_eq!(page.total, 1);
        // 悬空别名（主艺人已删）不命中任何艺人
        conn.execute(
            "INSERT INTO artist_aliases (alias, artist_id) VALUES ('已删艺人', 999)",
            [],
        )
        .unwrap();
        let page = query_artists_conn(&conn, Some("已删艺人".into()), None, None).unwrap();
        assert_eq!(page.total, 0);
    }

    #[test]
    fn query_albums_matches_merged_alias() {
        let conn = conn();
        seed(&conn);
        // 搜旧名命中其名下专辑
        let page = query_albums_conn(&conn, Some("旧艺人".into()), None, None).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].title, "专辑");
        // 按专辑标题搜索不受影响
        let page = query_albums_conn(&conn, Some("专辑".into()), None, None).unwrap();
        assert_eq!(page.total, 1);
        let page = query_albums_conn(&conn, None, None, None).unwrap();
        assert_eq!(page.total, 1);
    }
}

#[cfg(test)]
mod unmerge_tests {
    use super::*;

    fn conn() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        crate::db::register_collations(&conn).unwrap();
        conn.execute_batch(crate::db::SCHEMA).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn merge_then_unmerge_round_trip() {
        let mut conn = conn();
        // 旧艺人：T1（主艺人）+ T3（合作艺人，ord 1）+ 独立专辑；新艺人：T2 + 自己的专辑
        conn.execute_batch(
            "INSERT INTO sources (id, name) VALUES (1, 's');
             INSERT INTO artists (id, name) VALUES (1, '旧艺人'), (2, '新艺人');
             INSERT INTO albums (id, title, artist_id, year, key) VALUES
               (1, '旧专辑', 1, 2020, 'k1'), (2, '新专辑', 2, 2021, 'k2');
             INSERT INTO tracks (id, source_id, path, title, artist_id, album_id) VALUES
               (1, 1, 'p1', 'T1', 1, 1),
               (2, 1, 'p2', 'T2', 2, 2),
               (3, 1, 'p3', 'T3', 2, 2);
             INSERT INTO track_artists (track_id, artist_id, ord) VALUES
               (1, 1, 0), (2, 2, 0), (3, 2, 0), (3, 1, 1);",
        )
        .unwrap();

        // 合并：记录历史（调用方在合并后写入别名行，这里保持一致）
        {
            let tx = conn.transaction().unwrap();
            merge_artist_into(&tx, 1, 2).unwrap();
            tx.commit().unwrap();
        }
        conn.execute(
            "INSERT INTO artist_aliases (alias, artist_id) VALUES ('旧艺人', 2)",
            [],
        )
        .unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM artist_merge_history WHERE alias = '旧艺人'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 2);

        // 取消合并
        let r = unmerge_artist_conn(&mut conn, "旧艺人").unwrap();
        assert_eq!(r.restored_tracks, 2);
        assert_eq!(r.restored_albums, 1);

        // 旧艺人恢复为独立艺人；别名与历史清除
        let new_id: i64 = conn
            .query_row(
                "SELECT id FROM artists WHERE name = '旧艺人' COLLATE NOCASE",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM artist_aliases WHERE alias = '旧艺人'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM artist_merge_history", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(n, 0);

        // T1 主艺人归位；T3 恢复合作关联（ord 不变），主艺人仍是新艺人
        let aid: i64 = conn
            .query_row("SELECT artist_id FROM tracks WHERE id = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(aid, new_id);
        let rows: Vec<(i64, i64, i64)> = conn
            .prepare("SELECT track_id, artist_id, ord FROM track_artists WHERE track_id = 3 ORDER BY ord")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(rows, vec![(3, 2, 0), (3, new_id, 1)]);
        // 独立专辑拆回
        let aid: i64 = conn
            .query_row("SELECT artist_id FROM albums WHERE id = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(aid, new_id);
        // 新艺人自己的曲目与专辑不受影响
        let aid: i64 = conn
            .query_row("SELECT artist_id FROM tracks WHERE id = 2", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(aid, 2);
        let aid: i64 = conn
            .query_row("SELECT artist_id FROM albums WHERE id = 2", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(aid, 2);
    }

    #[test]
    fn unmerge_after_same_title_album_merge_keeps_tracks_in_target_album() {
        let mut conn = conn();
        // 两张同名同年专辑：合并时旧专辑被删除（曲目并入目标专辑）
        conn.execute_batch(
            "INSERT INTO sources (id, name) VALUES (1, 's');
             INSERT INTO artists (id, name) VALUES (1, '旧艺人'), (2, '新艺人');
             INSERT INTO albums (id, title, artist_id, year, key) VALUES
               (1, '同名', 1, 2020, 'k1'), (2, '同名', 2, 2020, 'k2');
             INSERT INTO tracks (id, source_id, path, title, artist_id, album_id) VALUES
               (1, 1, 'p1', 'T1', 1, 1);
             INSERT INTO track_artists (track_id, artist_id, ord) VALUES (1, 1, 0);",
        )
        .unwrap();
        {
            let tx = conn.transaction().unwrap();
            let (_, merged) = merge_artist_into(&tx, 1, 2).unwrap();
            tx.commit().unwrap();
            assert_eq!(merged, vec![1]);
        }
        conn.execute(
            "INSERT INTO artist_aliases (alias, artist_id) VALUES ('旧艺人', 2)",
            [],
        )
        .unwrap();

        let r = unmerge_artist_conn(&mut conn, "旧艺人").unwrap();
        // 专辑已随合并删除，只拆回曲目归属
        assert_eq!(r.restored_tracks, 1);
        assert_eq!(r.restored_albums, 0);
        let new_id: i64 = conn
            .query_row(
                "SELECT id FROM artists WHERE name = '旧艺人' COLLATE NOCASE",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let (aid, alid): (i64, i64) = conn
            .query_row(
                "SELECT artist_id, album_id FROM tracks WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((aid, alid), (new_id, 2));
    }

    #[test]
    fn unmerge_unknown_alias_fails() {
        let mut conn = conn();
        assert!(unmerge_artist_conn(&mut conn, "不存在的别名").is_err());
        assert!(unmerge_artist_conn(&mut conn, "   ").is_err());
    }
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
