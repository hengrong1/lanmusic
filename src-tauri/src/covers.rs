//! 专辑封面的惰性提取与缓存。
//!
//! 提取优先级：本地缓存 → WebDAV 目录约定文件（cover_url）→ 本地文件内嵌/同级封面
//! → LAN 共享端 /api/cover。失败的专辑写入 `{id}.none` 哨兵，避免重复网络 I/O。

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
    let (cover_url, remote_id, local_candidates, lan_source) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let (cover_url, remote_id) = conn
            .query_row("SELECT cover_url, remote_id FROM albums WHERE id = ?1", [album_id], |r| {
                Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<i64>>(1)?))
            })
            .unwrap_or((None, None));

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

        let lan = conn
            .query_row(
                "SELECT s.base_url, s.config FROM tracks t
                 JOIN sources s ON s.id = t.source_id
                 WHERE t.album_id = ?1 AND s.kind = 'lan' LIMIT 1",
                [album_id],
                |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?)),
            )
            .ok();

        (cover_url, remote_id, locals, lan)
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

    // 3) LAN 共享端封面接口（需要对方缓存中的专辑 remote_id）
    if let (Some(rid), Some((Some(base), Some(config)))) = (remote_id, lan_source) {
        attempted = true;
        let token = crate::network::config_field(Some(config.as_str()), "token").unwrap_or_default();
        if let Ok(bytes) = crate::network::lan::download_cover(base.trim_end_matches('/'), &token, rid) {
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
        "SELECT s.config FROM tracks t JOIN sources s ON s.id = t.source_id
         WHERE t.album_id = ?1 AND s.kind = 'webdav' LIMIT 1",
        [album_id],
        |r| r.get::<_, Option<String>>(0),
    )
    .ok()
    .flatten()
    .and_then(|cfg| crate::network::webdav::Auth::from_config(Some(&cfg)))
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
