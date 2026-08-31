//! `music://` 与 `cover://` 自定义协议。
//!
//! 前端 <audio> 的 src 指向 music://track/{id}（Windows 上为 http://music.localhost/track/{id}）。
//! 支持三类来源的统一路由：
//! - local  → 直接读本地文件流
//! - webdav → reqwest 代理转发（附带 Basic 认证，规避 WebView 跨域/凭证暴露）
//! - lan    → 代理转发对方设备的 /api/stream/{remote_id}（Bearer 配对码）
//!
//! 共享服务端（network::share）复用 serve_local_track_response 提供本机曲库。

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
        "SELECT t.path, t.format, t.file_size, t.remote_id, s.kind, s.base_path, s.base_url, s.config
         FROM tracks t JOIN sources s ON s.id = t.source_id
         WHERE t.id = ?1",
        [id],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<String>>(7)?,
            ))
        },
    );
    let (rel, format, size, remote_id, kind, base_path, base_url, config) = match row {
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
            let auth = match network::webdav::Auth::from_config(config.as_deref()) {
                Some(a) => ProxyAuth::Basic(a.username, a.password),
                None => ProxyAuth::None,
            };
            proxy_response(&url, range, auth)
        }
        "lan" => {
            let (Some(base), Some(rid)) = (base_url, remote_id) else { return not_found() };
            let token = network::config_field(config.as_deref(), "token").unwrap_or_default();
            let url = format!("{}/api/stream/{}", base.trim_end_matches('/'), rid);
            proxy_response(&url, range, ProxyAuth::Bearer(token))
        }
        _ => not_found(),
    }
}

/// 共享服务端用：只允许本地来源（远程源不被二次共享）
pub(crate) fn serve_local_track_response<R: Runtime>(
    app: &AppHandle<R>,
    track_id: i64,
    range: Option<&str>,
) -> Response<Vec<u8>> {
    let state = app.state::<crate::state::AppState>();
    let Ok(conn) = state.db.lock() else {
        return server_error();
    };
    let row = conn.query_row(
        "SELECT t.path, t.format, t.file_size, s.base_path
         FROM tracks t JOIN sources s ON s.id = t.source_id
         WHERE t.id = ?1 AND s.kind = 'local' AND s.enabled = 1",
        [track_id],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        },
    );
    let (rel, format, size, base_path) = match row {
        Ok(v) => v,
        Err(_) => return not_found(),
    };
    drop(conn);

    let Some(base) = base_path else { return not_found() };
    let full = PathBuf::from(base).join(&rel);
    let Ok(file) = std::fs::File::open(&full) else { return not_found() };
    let size = if size > 0 { size as u64 } else { file.metadata().map(|m| m.len()).unwrap_or(0) };
    serve_file_response(file, mime_of(format.as_deref()), size, range)
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
    Bearer(String),
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
    // bytes=N- → bytes=N-{N+CHUNK-1}；显式结尾与后缀范围原样转发
    if let Some(rest) = r.strip_prefix("bytes=") {
        if let Some((start, end)) = rest.split_once('-') {
            if end.trim().is_empty() {
                if let Ok(n) = start.trim().parse::<u64>() {
                    return format!("bytes={}-{}", n, n.saturating_add(CHUNK - 1));
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
        ProxyAuth::Bearer(t) => req.bearer_auth(t),
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
                .header(CONTENT_TYPE, "image/jpeg")
                .header(CACHE_CONTROL, "public, max-age=31536000, immutable")
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

pub(crate) fn error_response() -> Response<Vec<u8>> {
    server_error()
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
