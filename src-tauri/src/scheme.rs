//! `music://` 与 `cover://` 自定义协议。
//!
//! 前端 <audio> 的 src 指向 music://track/{id}（Windows 上为 http://music.localhost/track/{id}）。
//! 支持两类来源的统一路由：
//! - local  → 直接读本地文件流
//! - webdav → reqwest 代理转发（附带 Basic 认证，规避 WebView 跨域/凭证暴露）
//!
//! WebDAV 下载复用 network::webdav 的 HTTP 客户端。

use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::OnceLock;

use tauri::http::header::{ACCEPT_RANGES, CACHE_CONTROL, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, RANGE};
use tauri::http::{Request, Response, StatusCode};
use tauri::{AppHandle, Manager, Runtime, UriSchemeContext, UriSchemeResponder};

use crate::network;

/// 单次响应的最大体积：即使客户端请求未封顶的 Range（bytes=start-），
/// 也只返回该大小的数据，媒体引擎会自动发起后续 Range 请求。
const CHUNK: u64 = 2 * 1024 * 1024;

pub fn music_protocol<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    req: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = ctx.app_handle().clone();
    std::thread::spawn(move || {
        let resp = music_handle(app, req);
        let _ = responder.respond(resp);
    });
}

pub fn cover_protocol<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    req: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = ctx.app_handle().clone();
    std::thread::spawn(move || {
        let resp = cover_handle(app, req);
        let _ = responder.respond(resp);
    });
}

fn music_handle<R: Runtime>(app: AppHandle<R>, req: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let Some(id) = parse_id(&req, "track") else {
        return not_found();
    };
    if id <= 0 {
        return not_found();
    }
    let range = req
        .headers()
        .get(RANGE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    route_track(&app, id, range.as_deref())
}

/// 解析 track id 并按来源类型路由（协议与共享服务端共用）
pub(crate) fn route_track<R: Runtime>(
    app: &AppHandle<R>,
    id: i64,
    range: Option<&str>,
) -> Response<Vec<u8>> {
    let state = app.state::<crate::state::AppState>();
    let Ok(conn) = state.db.lock() else {
        return server_error();
    };
    let row = conn.query_row(
        "SELECT t.path, t.format, t.file_size, s.kind, s.base_path, s.base_url, s.config, s.id
         FROM tracks t JOIN sources s ON s.id = t.source_id
         WHERE t.id = ?1",
        [id],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, i64>(7)?,
            ))
        },
    );
    let (rel, format, size, kind, base_path, base_url, config, source_id) = match row {
        Ok(v) => v,
        Err(_) => return not_found(),
    };
    drop(conn);

    match kind.as_str() {
        "local" => {
            let Some(base) = base_path else { return not_found() };
            let full = PathBuf::from(base).join(&rel);
            let Ok(file) = std::fs::File::open(&full) else { return not_found() };
            let size = if size > 0 { size as u64 } else { file.metadata().map(|m| m.len()).unwrap_or(0) };
            serve_file_response(file, mime_of(format.as_deref()), size, range)
        }
        "webdav" => {
            let Some(base) = base_url else { return not_found() };
            let Ok(mut bu) = url::Url::parse(&base) else { return not_found() };
            if !bu.path().ends_with('/') {
                bu.set_path(&format!("{}/", bu.path()));
            }
            let Some(url) = bu.join(&rel).ok().map(|u| u.to_string()) else { return not_found() };
            let auth = match network::webdav::Auth::from_source(config.as_deref(), source_id) {
                Some(a) => ProxyAuth::Basic(a.username, a.password),
                None => ProxyAuth::None,
            };
            proxy_response(&url, range, auth)
        }
        _ => not_found(),
    }
}

/// 带本地文件流响应（支持 Range，2MB 分块封顶）
pub(crate) fn serve_file_response(
    mut file: std::fs::File,
    mime: &'static str,
    size: u64,
    range_header: Option<&str>,
) -> Response<Vec<u8>> {
    match range_header.and_then(|s| parse_range(s, size)) {
        Some((start, end)) => {
            if file.seek(SeekFrom::Start(start)).is_err() {
                return server_error();
            }
            let len = (end - start + 1) as usize;
            let buf = file.take(len as u64).bytes().collect::<Result<Vec<u8>, _>>();
            match buf {
                Ok(data) => Response::builder()
                    .status(StatusCode::PARTIAL_CONTENT)
                    .header(CONTENT_TYPE, mime)
                    .header(ACCEPT_RANGES, "bytes")
                    .header(CONTENT_RANGE, format!("bytes {}-{}/{}", start, end, size))
                    .header(CONTENT_LENGTH, data.len())
                    .header(CACHE_CONTROL, "no-store")
                    .header("Access-Control-Allow-Origin", "*")
                    .body(data)
                    .unwrap_or_else(|_| server_error()),
                Err(_) => server_error(),
            }
        }
        // 无 Range 头：整文件返回（正常媒体引擎首次请求都带 Range，此路径很少触发）
        None => match file.bytes().collect::<Result<Vec<u8>, _>>() {
            Ok(data) => Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_TYPE, mime)
                .header(ACCEPT_RANGES, "bytes")
                .header(CONTENT_LENGTH, data.len())
                .header(CACHE_CONTROL, "no-store")
                .header("Access-Control-Allow-Origin", "*")
                .body(data)
                .unwrap_or_else(|_| server_error()),
            Err(_) => server_error(),
        },
    }
}

// ---------------------------------------------------------------- 远程代理

pub(crate) enum ProxyAuth {
    None,
    Basic(String, String),
}

/// 经 Rust 代理远程音频流：转发 Range 并按 CHUNK 封顶，附带认证头。
fn client() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("构建 HTTP 客户端失败")
    })
}

fn cap_open_range(r: &str) -> String {
    // 所有范围形态统一封顶到 CHUNK，避免远端按原始 Range 返回 Content-Range
    // 而本地 take(CHUNK) 截断后 Content-Length 与之矛盾：
    // - bytes=N-   → bytes=N-{N+CHUNK-1}
    // - bytes=N-M  → bytes=N-{min(M, N+CHUNK-1)}
    // - bytes=-N   → bytes=-{min(N, CHUNK)}（后缀范围取末尾更少字节，Content-Range 会如实回告）
    // 无法解析/非法的范围原样转发（远端会返回 416，代理降级为 not_found）
    if let Some(rest) = r.strip_prefix("bytes=") {
        if let Some((start, end)) = rest.split_once('-') {
            if start.trim().is_empty() {
                // 后缀范围：限制请求的末尾字节数
                if let Ok(n) = end.trim().parse::<u64>() {
                    let capped = n.min(CHUNK);
                    if capped > 0 {
                        return format!("bytes=-{capped}");
                    }
                }
            } else if let Ok(s) = start.trim().parse::<u64>() {
                let limit = s.saturating_add(CHUNK - 1);
                let capped_end = if end.trim().is_empty() {
                    Some(limit)
                } else {
                    end.trim().parse::<u64>().ok().map(|m| m.min(limit))
                };
                if let Some(e) = capped_end {
                    if e >= s {
                        return format!("bytes={s}-{e}");
                    }
                }
            }
        }
    }
    r.to_string()
}

pub(crate) fn proxy_response(
    url: &str,
    client_range: Option<&str>,
    auth: ProxyAuth,
) -> Response<Vec<u8>> {
    let mut req = client().get(url);
    req = match &auth {
        ProxyAuth::None => req,
        ProxyAuth::Basic(u, p) => req.basic_auth(u, Some(p)),
    };
    if let Some(r) = client_range {
        req = req.header(RANGE, cap_open_range(r));
    }
    let Ok(resp) = req.send() else {
        return bad_gateway();
    };
    let status = resp.status();
    if !(status.is_success() || status == reqwest::StatusCode::PARTIAL_CONTENT) {
        return not_found();
    }
    let content_range: Option<String> = resp
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let is_partial = content_range.is_some();

    // 最多缓冲 CHUNK 字节，超出部分截断（客户端会继续请求后续 Range）
    let mut resp = resp;
    let mut buf = Vec::new();
    let _ = (&mut resp).take(CHUNK).read_to_end(&mut buf);

    let mut builder = Response::builder()
        .status(if is_partial { StatusCode::PARTIAL_CONTENT } else { StatusCode::OK })
        .header(ACCEPT_RANGES, "bytes")
        .header(CONTENT_LENGTH, buf.len())
        .header(CACHE_CONTROL, "no-store")
        .header("Access-Control-Allow-Origin", "*");
    if let Some(cr) = content_range {
        builder = builder.header(CONTENT_RANGE, cr);
    }
    builder.body(buf).unwrap_or_else(|_| server_error())
}

fn cover_handle<R: Runtime>(app: AppHandle<R>, req: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let Some(id) = parse_id(&req, "album") else {
        return not_found();
    };
    if id <= 0 {
        return not_found();
    }
    // 惰性提取：缓存未命中时现场读文件提取封面（见 covers.rs）
    match crate::covers::ensure_cover(&app, id) {
        Ok(Some(file)) => match std::fs::read(&file) {
            Ok(bytes) => Response::builder()
                .status(StatusCode::OK)
                // 缓存有效期不可过长：专辑重解析/删除后 rowid 会被复用，同名 URL
                // 可能指向新封面，immutable 长缓存会让旧图一直存活
                .header(CONTENT_TYPE, sniff_image_mime(&bytes))
                .header(CACHE_CONTROL, "public, max-age=3600")
                .header("Access-Control-Allow-Origin", "*")
                .body(bytes)
                .unwrap_or_else(|_| server_error()),
            Err(_) => not_found(),
        },
        Ok(None) => not_found(),
        Err(_) => server_error(),
    }
}

/// 解析请求中的资源 id，兼容两种 URL 形态：
/// - Windows: `http://music.localhost/track/123` → host = "music.localhost"，path = "/track/123"
/// - macOS/Linux: `music://track/123` → host = "track"（URI 语法里 // 后是 authority），path = "/123"
fn parse_id(req: &Request<Vec<u8>>, kind: &str) -> Option<i64> {
    let uri = req.uri();
    let path = uri.path().trim_start_matches('/');
    if uri.host() == Some(kind) {
        path.parse::<i64>().ok()
    } else {
        path.strip_prefix(kind)?.strip_prefix('/')?.parse::<i64>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(url: &str) -> Request<Vec<u8>> {
        Request::builder().uri(url).body(Vec::new()).unwrap()
    }

    #[test]
    fn parses_macos_style_host_form() {
        // music://track/498 → "track" 是 host，不是路径前缀
        assert_eq!(parse_id(&req("music://track/498"), "track"), Some(498));
        assert_eq!(parse_id(&req("cover://album/364"), "album"), Some(364));
    }

    #[test]
    fn parses_windows_style_path_form() {
        assert_eq!(
            parse_id(&req("http://music.localhost/track/498"), "track"),
            Some(498)
        );
        assert_eq!(
            parse_id(&req("http://cover.localhost/album/364"), "album"),
            Some(364)
        );
    }

    #[test]
    fn rejects_mismatched_or_garbage() {
        assert_eq!(parse_id(&req("music://track/abc"), "track"), None);
        assert_eq!(parse_id(&req("music://other/498"), "track"), None);
        assert_eq!(parse_id(&req("music://track/"), "track"), None);
    }
}

/// 解析 "bytes=a-b" / "bytes=a-" / "bytes=-n"，并按 CHUNK 封装上限
fn parse_range(s: &str, size: u64) -> Option<(u64, u64)> {
    if size == 0 {
        return None;
    }
    let rest = s.trim().strip_prefix("bytes=")?;
    let (a, b) = rest.split_once('-')?;
    let (start, end) = if a.is_empty() {
        // 后缀范围：最后 n 字节
        let n: u64 = b.trim().parse().ok()?;
        (size.saturating_sub(n), size - 1)
    } else {
        let start: u64 = a.trim().parse().ok()?;
        if start >= size {
            return None;
        }
        let end = if b.trim().is_empty() {
            size - 1
        } else {
            b.trim().parse::<u64>().ok()?.min(size - 1)
        };
        (start, end)
    };
    if start > end {
        return None;
    }
    let capped = end.min(start + CHUNK - 1);
    Some((start, capped))
}

pub(crate) fn mime_of(format: Option<&str>) -> &'static str {
    match format.unwrap_or("") {
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "m4a" | "m4b" | "mp4" => "audio/mp4",
        "aac" => "audio/aac",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        "aif" | "aiff" => "audio/aiff",
        _ => "application/octet-stream",
    }
}

/// 按魔数嗅探图片类型（封面缓存文件统一存 .jpg 扩展名，但内容可能是 PNG/WebP 原样字节）
pub(crate) fn sniff_image_mime(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0xFF, 0xD8]) {
        "image/jpeg"
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if bytes.len() > 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        "application/octet-stream"
    }
}

fn not_found() -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Vec::new())
        .unwrap()
}

fn bad_gateway() -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::BAD_GATEWAY)
        .body(Vec::new())
        .unwrap()
}

fn server_error() -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(Vec::new())
        .unwrap()
}
