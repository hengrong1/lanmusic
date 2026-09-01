//! 歌词获取：外挂 .lrc 文件 / 内嵌 USLT / 远程源接口，统一返回原始文本。

use std::path::PathBuf;

use rusqlite::params;
use tauri::{AppHandle, Manager};

use crate::state::AppState;

pub struct SourceRef {
    pub kind: String,
    pub base_path: Option<String>,
    pub config: Option<String>,
}

pub fn source_ref(app: &AppHandle, source_id: i64) -> Result<SourceRef, String> {
    let state = app.state::<AppState>();
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT kind, base_path, config FROM sources WHERE id = ?1",
        params![source_id],
        |r| {
            Ok(SourceRef {
                kind: r.get(0)?,
                base_path: r.get(1)?,
                config: r.get(2)?,
            })
        },
    )
    .map_err(|e| e.to_string())
}

/// 返回原始歌词文本（可能是带时间轴的 LRC，也可能是纯文本）
pub fn fetch(app: &AppHandle, track_id: i64) -> Result<Option<String>, String> {
    let state = app.state::<AppState>();
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let row = conn
        .query_row(
            "SELECT t.source_id, t.path, l.path
             FROM tracks t
             JOIN sources s ON s.id = t.source_id
             LEFT JOIN lrc_files l ON l.track_id = t.id
             WHERE t.id = ?1",
            params![track_id],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|e| e.to_string())?;
    drop(conn);

    let Some((source_id, rel, lrc_path)) = row else {
        return Ok(None);
    };
    let src = source_ref(app, source_id)?;

    match src.kind.as_str() {
        // 本地：外挂 .lrc（扫描记录或同名懒检查）→ 内嵌歌词
        "local" => {
            if let Some(p) = lrc_path {
                // 外挂文件可能已被移动/删除：读取失败时降级到内嵌歌词，而不是整体报错
                if let Ok(bytes) = std::fs::read(PathBuf::from(&p)) {
                    return Ok(Some(String::from_utf8_lossy(&bytes).into_owned()));
                }
            }
            let Some(base) = src.base_path else { return Ok(None) };
            let full = PathBuf::from(base).join(&rel);
            // 兼容旧库：同名 .lrc 懒检查（无需等重新扫描）
            let sibling = full.with_extension("lrc");
            if sibling.is_file() {
                if let Ok(bytes) = std::fs::read(&sibling) {
                    return Ok(Some(String::from_utf8_lossy(&bytes).into_owned()));
                }
            }
            Ok(crate::metadata::read(&full, false).ok().and_then(|m| m.lyrics))
        }
        // WebDAV：外挂 .lrc 是完整 URL，按需下载；内嵌歌词暂不读取（避免整文件下载）
        "webdav" => {
            let Some(u) = lrc_path else { return Ok(None) };
            let parsed = url::Url::parse(&u).map_err(|e| e.to_string())?;
            let auth = crate::network::webdav::Auth::from_config(src.config.as_deref());
            crate::network::webdav::download_text(&parsed, auth.as_ref())
        }
        _ => Ok(None),
    }
}

use rusqlite::OptionalExtension;
