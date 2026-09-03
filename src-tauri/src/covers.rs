//! 专辑封面的惰性提取与缓存。
//!
//! 提取优先级：本地缓存 → WebDAV 目录约定文件（cover_url）→ 本地文件内嵌/同级封面。
//! 失败的专辑写入 `{id}.none` 哨兵，避免重复网络 I/O。

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager, Runtime};

use crate::state::AppState;

pub(crate) const COVER_NAMES: &[&str] = &[
    "cover.jpg", "cover.jpeg", "cover.png", "folder.jpg", "folder.png", "front.jpg", "front.png",
];
const MAX_DIM: u32 = 512;

/// 确保专辑封面已缓存。返回 Some(缓存文件路径) / None（确认无封面或暂不可得）。
pub fn ensure_cover<R: Runtime>(app: &AppHandle<R>, album_id: i64) -> Result<Option<PathBuf>, String> {
    let state = app.state::<AppState>();
    let jpg = state.covers_dir.join(format!("{album_id}.jpg"));
    let none = state.covers_dir.join(format!("{album_id}.none"));
    if jpg.is_file() {
        return Ok(Some(jpg));
    }
    if none.is_file() {
        return Ok(None);
    }

    // 串行化提取：专辑网格首屏可能同时请求几十个封面，避免并发网络读风暴
    let _guard = state.cover_extract.lock().map_err(|e| e.to_string())?;
    // 双重检查（排队期间可能已被其他请求完成）
    if jpg.is_file() {
        return Ok(Some(jpg));
    }
    if none.is_file() {
        return Ok(None);
    }

    // 专辑信息与候选来源
    let (cover_url, local_candidates) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let cover_url = conn
            .query_row("SELECT cover_url FROM albums WHERE id = ?1", [album_id], |r| {
                r.get::<_, Option<String>>(0)
            })
            .unwrap_or(None);

        let mut locals = Vec::new();
        {
            let mut stmt = conn
                .prepare(
                    "SELECT t.path, s.base_path FROM tracks t
                     JOIN sources s ON s.id = t.source_id
                     WHERE t.album_id = ?1 AND s.kind = 'local' AND s.base_path IS NOT NULL
                     ORDER BY t.disc_no, t.track_no, t.path LIMIT 8",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([album_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .map_err(|e| e.to_string())?;
            for row in rows {
                locals.push(row.map_err(|e| e.to_string())?);
            }
        }

        (cover_url, locals)
    };

    let save = |bytes: &[u8]| -> Result<(), String> { save_cover(&state.covers_dir, album_id, bytes) };
    let mut attempted = false;

    // 1) WebDAV 目录约定封面（扫描时已记录 URL）
    if let Some(url) = cover_url {
        attempted = true;
        if let Ok(parsed) = url::Url::parse(&url) {
            let auth = webdav_auth_for_album(app, album_id);
            if let Ok(bytes) = crate::network::webdav::download(&parsed, auth.as_ref(), None) {
                if save(&bytes).is_ok() {
                    mark_cover(app, album_id)?;
                    return Ok(Some(jpg));
                }
            }
        }
    }

    // 2) 本地文件：内嵌封面 / 同级约定文件
    for (rel, base) in &local_candidates {
        let full = PathBuf::from(base).join(rel);
        if !full.is_file() {
            continue;
        }
        attempted = true;
        if let Ok(meta) = crate::metadata::read(&full, true) {
            if let Some(bytes) = meta.cover {
                if save(&bytes).is_ok() {
                    mark_cover(app, album_id)?;
                    return Ok(Some(jpg));
                }
            }
        }
        if let Some(bytes) = find_sibling_cover(&full) {
            if save(&bytes).is_ok() {
                mark_cover(app, album_id)?;
                return Ok(Some(jpg));
            }
        }
    }

    if !attempted {
        // 专辑下没有任何可尝试的来源：不写哨兵（可能后续扫描会补充曲目）
        return Ok(None);
    }

    std::fs::write(&none, b"").map_err(|e| e.to_string())?;
    Ok(None)
}

/// 从专辑所属的 webdav 来源恢复认证（cover_url 的下载需要凭证）
fn webdav_auth_for_album<R: Runtime>(
    app: &AppHandle<R>,
    album_id: i64,
) -> Option<crate::network::webdav::Auth> {
    let state = app.state::<AppState>();
    let conn = state.db.lock().ok()?;
    conn.query_row(
        "SELECT s.id, s.config FROM tracks t JOIN sources s ON s.id = t.source_id
         WHERE t.album_id = ?1 AND s.kind = 'webdav' LIMIT 1",
        [album_id],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?)),
    )
    .ok()
    .and_then(|(source_id, cfg)| crate::network::webdav::Auth::from_source(cfg.as_deref(), source_id))
}

fn mark_cover<R: Runtime>(app: &AppHandle<R>, album_id: i64) -> Result<(), String> {
    let state = app.state::<AppState>();
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("UPDATE albums SET has_cover = 1 WHERE id = ?1", [album_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn find_sibling_cover(track_path: &Path) -> Option<Vec<u8>> {
    let dir = track_path.parent()?;
    for name in COVER_NAMES {
        let p = dir.join(name);
        if p.is_file() {
            return std::fs::read(&p).ok();
        }
    }
    None
}

/// 删除专辑的封面缓存文件（专辑被删除或其曲目重新解析时调用）。
///
/// 必须与 `DELETE FROM albums` 配对执行：SQLite 会复用已删除专辑的 rowid，
/// 若残留 `{id}.jpg` / `{id}.none`，新专辑会直接命中旧封面，表现为「歌和封面对不上」。
pub fn purge(covers_dir: &Path, album_ids: &[i64]) {
    for id in album_ids {
        let _ = std::fs::remove_file(covers_dir.join(format!("{id}.jpg")));
        let _ = std::fs::remove_file(covers_dir.join(format!("{id}.none")));
    }
}

/// 降采样到 512px 存为 JPEG；解码失败则原样保存
pub(crate) fn save_cover(covers_dir: &Path, album_id: i64, bytes: &[u8]) -> Result<(), String> {
    let out = covers_dir.join(format!("{album_id}.jpg"));
    match image::load_from_memory(bytes) {
        Ok(img) => {
            let img = img.resize(MAX_DIM, MAX_DIM, image::imageops::FilterType::Lanczos3);
            img.save_with_format(&out, image::ImageFormat::Jpeg)
                .map_err(|e| e.to_string())
        }
        Err(_) => std::fs::write(&out, bytes).map_err(|e| e.to_string()),
    }
}

// ================================================================ 缓存容量控制（LRU 式清理）

/// 上限设置键：app_settings.covers.max_mb（兆字节），缺省 500MB；0 表示不限制
const MAX_MB_KEY: &str = "covers.max_mb";
const DEFAULT_MAX_MB: u64 = 500;

/// 按设置清理封面缓存（启动时与每次扫描结束时调用）
pub fn enforce_limit_with_setting<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    let max_mb: u64 = {
        let Ok(conn) = state.db.lock() else { return };
        crate::db::get_setting(&conn, MAX_MB_KEY)
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_MAX_MB)
    };
    enforce_limit(&state.covers_dir, max_mb * 1024 * 1024);
}

/// 缓存总量超限时清理：先删 `.none` 哨兵（零成本，可重建），再按修改时间从旧到新删 `.jpg`。
/// `max_bytes` = 0 表示不限制。
pub fn enforce_limit(covers_dir: &Path, max_bytes: u64) {
    if max_bytes == 0 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(covers_dir) else { return };
    let mut files: Vec<(PathBuf, u64, std::time::SystemTime, bool)> = Vec::new();
    let mut total = 0u64;
    for entry in rd.flatten() {
        let p = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let is_sentinel = name.ends_with(".none");
        if !is_sentinel && !name.ends_with(".jpg") {
            continue;
        }
        total += meta.len();
        files.push((p, meta.len(), meta.modified().unwrap_or(std::time::UNIX_EPOCH), is_sentinel));
    }
    if total <= max_bytes {
        return;
    }
    // 排序：哨兵优先（is_none 排前），其后按 mtime 旧 → 新
    files.sort_by_key(|(_, _, mtime, is_none)| (!*is_none, *mtime));
    for (path, size, _, _) in files {
        if total <= max_bytes {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
}
