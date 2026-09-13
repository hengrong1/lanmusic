//! 歌词获取：外挂 .qrc/.lrc 文件 / 内嵌 USLT，按设置的优先顺序取用，统一返回原始文本。

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use tauri::{AppHandle, Manager};

use crate::state::AppState;

/// 歌词来源：外挂 .qrc（逐字）/ 外挂 .lrc / 内嵌歌词
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LyricSrc {
    Qrc,
    Lrc,
    Embedded,
}

/// 歌词来源优先级设置键（值形如 "qrc,lrc,embedded"，逗号分隔）
const LYRIC_PRIORITY_KEY: &str = "lm.lyricPriority";
/// 默认顺序：外挂 qrc → 外挂 lrc → 内嵌（逐字歌词优先）
const LYRIC_PRIORITY_DEFAULT: &str = "qrc,lrc,embedded";

/// 读取歌词来源优先级；缺失/损坏（重复项）时按出现顺序去重、缺失层级按默认顺序补齐，
/// 保证三种来源都保留尝试机会
fn lyric_priority(conn: &Connection) -> Vec<LyricSrc> {
    let raw = crate::db::get_setting(conn, LYRIC_PRIORITY_KEY)
        .unwrap_or_else(|| LYRIC_PRIORITY_DEFAULT.to_owned());
    let mut out: Vec<LyricSrc> = Vec::with_capacity(3);
    for part in raw.split(',') {
        let kind = match part.trim() {
            "qrc" => LyricSrc::Qrc,
            "lrc" => LyricSrc::Lrc,
            "embedded" => LyricSrc::Embedded,
            _ => continue,
        };
        if !out.contains(&kind) {
            out.push(kind);
        }
    }
    for kind in [LyricSrc::Qrc, LyricSrc::Lrc, LyricSrc::Embedded] {
        if !out.contains(&kind) {
            out.push(kind);
        }
    }
    out
}

/// 外挂歌词路径/URL 的来源类型（容忍 URL 携带的 query/fragment）
fn lyric_ext_kind(path: &str) -> Option<LyricSrc> {
    let clean = path.split(['?', '#']).next().unwrap_or(path);
    let ext = Path::new(clean).extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "qrc" => Some(LyricSrc::Qrc),
        "lrc" => Some(LyricSrc::Lrc),
        _ => None,
    }
}

/// 歌词文件字节解码：兼容 UTF-8（含 BOM）、UTF-16 LE/BE 与 GBK/GB18030——
/// QQ 音乐生态导出的歌词常见 UTF-16 或 GBK，直接按 UTF-8 读会得到乱码。
pub fn decode_lyric_bytes(bytes: &[u8]) -> String {
    // BOM 优先
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        let be = bytes[0] == 0xFE;
        return decode_utf16(&bytes[2..], be);
    }
    // 无 BOM 但穿插大量 NUL：UTF-16 特征（每 2 字节一个码元）
    let nuls = bytes.iter().filter(|b| **b == 0).count();
    if bytes.len() > 8 && nuls * 3 > bytes.len() {
        return decode_utf16(bytes, bytes[0] == 0);
    }
    // 严格 UTF-8：有效即采用（含无 BOM 的纯 ASCII / UTF-8 中文）
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.trim_start_matches('\u{feff}').to_owned();
    }
    // 二进制特征：控制字节（除 \t \n \r）占比 >2%。加密 QRC（QQ 音乐 DES 加密的原始
    // 字节）呈均匀随机分布，必然命中；GBK 文本文件不会含有这些字节
    let binary_signal = bytes
        .iter()
        .filter(|b| matches!(**b, 0x00..=0x08 | 0x0E..=0x1F))
        .count();
    if binary_signal * 50 > bytes.len() {
        // 转十六进制交给 qrc::parse 解密（加密 QRC 的归一载体，前端无需感知）
        let mut hex = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            hex.push_str(&format!("{:02X}", b));
        }
        return hex;
    }
    // UTF-8 无效且非二进制：按 GBK/GB18030 解码（QQ 音乐生态最常见的遗留编码）
    let (decoded, _, _) = encoding_rs::GB18030.decode(bytes);
    decoded.trim_start_matches('\u{feff}').to_owned()
}

/// UTF-16 解码（假设调用方已处理 BOM 剥离与字节序）
fn decode_utf16(bytes: &[u8], be: bool) -> String {
    let units: Vec<u16> = bytes
        .chunks(2)
        .map(|c| {
            let hi = c[0];
            let lo = c.get(1).copied().unwrap_or(0);
            if be {
                u16::from_be_bytes([hi, lo])
            } else {
                u16::from_le_bytes([hi, lo])
            }
        })
        .collect();
    String::from_utf16_lossy(&units)
}

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

/// 返回原始歌词文本（外挂 .qrc/.lrc 与内嵌歌词按设置的优先顺序取用，见 lyric_priority）
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
    let priority = lyric_priority(&conn);
    drop(conn);

    let Some((source_id, rel, lrc_path, has_embedded, meta_state)) = row else {
        return Ok(None);
    };
    let src = source_ref(app, source_id)?;

    match src.kind.as_str() {
        // 本地：按优先级尝试外挂 .qrc/.lrc（扫描记录或同名懒检查）→ 内嵌歌词
        "local" => {
            let full = src.base_path.map(|b| PathBuf::from(b).join(&rel));
            // 扫描记录的外挂歌词（每曲一条，可能是 .lrc 或 .qrc）
            let recorded = lrc_path
                .as_deref()
                .map(|p| (lyric_ext_kind(p), PathBuf::from(p)));
            for kind in priority {
                match kind {
                    LyricSrc::Qrc | LyricSrc::Lrc => {
                        // 扫描记录优先（免磁盘探测），读取失败再懒检查同名文件（无需等重新扫描）
                        if let Some((Some(k), p)) = &recorded {
                            if *k == kind {
                                if let Ok(bytes) = std::fs::read(p) {
                                    return Ok(Some(decode_lyric_bytes(&bytes)));
                                }
                            }
                        }
                        let ext = if kind == LyricSrc::Qrc { "qrc" } else { "lrc" };
                        if let Some(full) = &full {
                            let sibling = full.with_extension(ext);
                            if sibling.is_file() {
                                if let Ok(bytes) = std::fs::read(&sibling) {
                                    return Ok(Some(decode_lyric_bytes(&bytes)));
                                }
                            }
                        }
                    }
                    LyricSrc::Embedded => {
                        if let Some(text) = full
                            .as_deref()
                            .and_then(|f| crate::metadata::read(f, false).ok())
                            .and_then(|m| m.lyrics)
                        {
                            return Ok(Some(text));
                        }
                    }
                }
            }
            Ok(None)
        }
        // WebDAV：外挂歌词是完整 URL（每曲只记录一条，.lrc 或 .qrc），按需下载；
        // 内嵌歌词只拉文件头部 1MB 交给 lofty 解析（与扫描读标签同一套路），不必整文件下载。
        "webdav" => {
            let auth = crate::network::webdav::Auth::from_source(src.config.as_deref(), source_id);
            let external = lrc_path
                .as_deref()
                .and_then(|u| lyric_ext_kind(u).map(|k| (k, u)));
            for kind in priority {
                match kind {
                    LyricSrc::Qrc | LyricSrc::Lrc => {
                        // 仅当记录的外挂类型与该层级匹配时才尝试下载（类型不符时该层级无候选）
                        if let Some((k, u)) = &external {
                            if *k == kind {
                                if let Ok(parsed) = url::Url::parse(u) {
                                    if let Ok(Some(text)) = crate::network::webdav::download_text(
                                        &parsed,
                                        auth.as_ref(),
                                    ) {
                                        return Ok(Some(text));
                                    }
                                }
                            }
                        }
                    }
                    LyricSrc::Embedded => {
                        // 已完整解析（meta_state=1）且确认无内嵌歌词：不必白拉 1MB。
                        // meta_state=0 是「快速导入 / 待补全」的行，标志本身就不可信，仍要去读一次。
                        if has_embedded == 0 && meta_state == 1 {
                            continue;
                        }
                        let Some(base) = src.base_url.as_deref() else {
                            continue;
                        };
                        let Ok(base) = crate::network::webdav::normalize_base(base) else {
                            continue;
                        };
                        let url = crate::network::webdav::file_url(&base, &rel);
                        let Ok(bytes) = crate::network::webdav::download(
                            &url,
                            auth.as_ref(),
                            Some((0, crate::metadata::HEAD_FETCH_SIZE - 1)),
                        ) else {
                            continue;
                        };
                        if let Some(text) = crate::metadata::read_bytes(&bytes, false)
                            .ok()
                            .and_then(|m| m.lyrics)
                        {
                            return Ok(Some(text));
                        }
                    }
                }
            }
            Ok(None)
        }
        _ => Ok(None),
    }
}

use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE app_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn priority_default_when_missing() {
        let conn = setup();
        assert_eq!(
            lyric_priority(&conn),
            vec![LyricSrc::Qrc, LyricSrc::Lrc, LyricSrc::Embedded]
        );
    }

    #[test]
    fn priority_custom_order_and_repair() {
        let conn = setup();
        conn.execute(
            "INSERT INTO app_settings (key, value) VALUES ('lm.lyricPriority', 'embedded, lrc')",
            [],
        )
        .unwrap();
        assert_eq!(
            lyric_priority(&conn),
            vec![LyricSrc::Embedded, LyricSrc::Lrc, LyricSrc::Qrc]
        );

        // 重复项去重、非法项忽略、缺失层级按默认顺序补齐
        conn.execute(
            "UPDATE app_settings SET value = 'lrc,lrc,bogus' WHERE key = 'lm.lyricPriority'",
            [],
        )
        .unwrap();
        assert_eq!(
            lyric_priority(&conn),
            vec![LyricSrc::Lrc, LyricSrc::Qrc, LyricSrc::Embedded]
        );
    }

    #[test]
    fn ext_kind_tolerates_url() {
        assert_eq!(lyric_ext_kind("/a/b.qrc"), Some(LyricSrc::Qrc));
        assert_eq!(
            lyric_ext_kind("http://x/a.LRC?download=1"),
            Some(LyricSrc::Lrc)
        );
        assert_eq!(lyric_ext_kind("http://x/a.txt"), None);
    }
}
