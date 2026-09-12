//! 歌词获取：外挂 .lrc 文件 / 内嵌 USLT / 远程源接口，统一返回原始文本。

use std::path::PathBuf;

use rusqlite::params;
use tauri::{AppHandle, Manager};

use crate::state::AppState;

pub struct SourceRef {
    pub kind: String,
    pub base_path: Option<String>,
    pub base_url: Option<String>,
    pub config: Option<String>,
}

pub fn source_ref(app: &AppHandle, source_id: i64) -> Result<SourceRef, String> {
    let state = app.state::<AppState>();
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT kind, base_path, base_url, config FROM sources WHERE id = ?1",
        params![source_id],
        |r| {
            Ok(SourceRef {
                kind: r.get(0)?,
                base_path: r.get(1)?,
                base_url: r.get(2)?,
                config: r.get(3)?,
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
            "SELECT t.source_id, t.path, l.path, t.has_embedded_lyrics, t.meta_state
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
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|e| e.to_string())?;
    drop(conn);

    let Some((source_id, rel, lrc_path, has_embedded, meta_state)) = row else {
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
        // WebDAV：外挂 .lrc 是完整 URL，按需下载；没有外挂歌词时回退到**内嵌歌词** ——
        // 只拉文件头部 1MB 交给 lofty 解析（与扫描读标签同一套路），不必整文件下载。
        "webdav" => {
            let auth = crate::network::webdav::Auth::from_source(src.config.as_deref(), source_id);
            if let Some(u) = lrc_path {
                if let Ok(parsed) = url::Url::parse(&u) {
                    // 外挂文件可能已被移动/删除：读取失败时继续尝试内嵌歌词
                    if let Ok(Some(text)) = crate::network::webdav::download_text(&parsed, auth.as_ref()) {
                        return Ok(Some(text));
                    }
                }
            }
            // 已完整解析（meta_state=1）且确认无内嵌歌词：不必白拉 1MB。
            // meta_state=0 是「快速导入 / 待补全」的行，标志本身就不可信，仍要去读一次。
            if has_embedded == 0 && meta_state == 1 {
                return Ok(None);
            }
            let Some(base) = src.base_url else { return Ok(None) };
            let base = crate::network::webdav::normalize_base(&base)?;
            let url = crate::network::webdav::file_url(&base, &rel);
            let head = crate::network::webdav::download(
                &url,
                auth.as_ref(),
                Some((0, crate::metadata::HEAD_FETCH_SIZE - 1)),
            );
            let Ok(bytes) = head else { return Ok(None) };
            Ok(crate::metadata::read_bytes(&bytes, false).ok().and_then(|m| m.lyrics))
        }
        _ => Ok(None),
    }
}

use rusqlite::OptionalExtension;
