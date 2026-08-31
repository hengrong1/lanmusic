//! 局域网子系统：WebDAV 客户端 / LAN 共享客户端 / axum 共享服务端 / mDNS 发现与广播。
//!
//! 安全约定：
//! - 共享服务绑定 0.0.0.0，但所有接口都要求 Bearer Token（6 位配对码，设置页可见）；
//! - WebDAV 凭证保存在 sources.config（JSON），不写入日志；
//! - 共享端只暴露「本地来源」的曲目（远程源不会被二次共享）。

use std::collections::HashMap;
use std::sync::OnceLock;

use percent_encoding::percent_decode_str;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use url::Url;

use crate::db;
use crate::scheme;
use crate::state::{AppState, ShareHandle};

pub const SERVICE_TYPE: &str = "_lanmusic._tcp.local.";
pub const DEFAULT_PORT: u16 = 45678;

pub fn share_default_port() -> u16 {
    DEFAULT_PORT
}

// ---------------------------------------------------------------- 通用工具

fn http_client() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("构建 HTTP 客户端失败")
    })
}

pub fn config_field(config: Option<&str>, key: &str) -> Option<String> {
    let cfg: serde_json::Value = serde_json::from_str(config?).ok()?;
    cfg.get(key)?.as_str().map(str::to_string)
}

pub fn get_setting(app: &AppHandle, key: &str) -> Option<String> {
    let state = app.state::<AppState>();
    let conn = state.db.lock().ok()?;
    db::get_setting(&conn, key)
}

pub fn set_setting(app: &AppHandle, key: &str, value: &str) {
    let state = app.state::<AppState>();
    let guard = state.db.lock();
    if let Ok(conn) = guard {
        let _ = db::set_setting(&conn, key, value);
    }
}

fn ensure_share_token(app: &AppHandle) -> Result<String, String> {
    if let Some(t) = get_setting(app, "share_token") {
        return Ok(t);
    }
    use rand::RngExt;
    let token = format!("{:06}", rand::rng().random_range(100_000..1_000_000));
    set_setting(app, "share_token", &token);
    Ok(token)
}

/// 共享给远程端的曲目元数据（服务端序列化 / 客户端反序列化共用）
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RemoteTrack {
    pub id: i64,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub year: Option<i64>,
    pub track_no: Option<i64>,
    pub duration: Option<f64>,
    pub format: Option<String>,
    pub album_id: Option<i64>,
    pub has_lrc: bool,
}

// ---------------------------------------------------------------- WebDAV 客户端

pub mod webdav {
    use super::*;

    #[derive(Clone, Debug)]
    pub struct Auth {
        pub username: String,
        pub password: String,
    }

    impl Auth {
        pub fn from_config(config: Option<&str>) -> Option<Auth> {
            Some(Auth {
                username: config_field(config, "username")?,
                password: config_field(config, "password")?,
            })
        }
    }

    /// 解码后的目录项：abs 为从站点根开始的绝对路径
    #[derive(Debug, Clone)]
    pub struct Item {
        pub abs: String,
        pub is_dir: bool,
        pub size: i64,
    }

    pub fn list_dir(dir: &Url, auth: Option<&Auth>) -> Result<Vec<Item>, String> {
        let method = reqwest::Method::from_bytes(b"PROPFIND").map_err(|e| e.to_string())?;
        let mut req = http_client()
            .request(method, dir.as_str())
            .header("Depth", "1")
            .header("Content-Type", "application/xml");
        if let Some(a) = auth {
            req = req.basic_auth(&a.username, Some(&a.password));
        }
        let resp = req
            .body(r#"<?xml version="1.0"?><D:propfind xmlns:D="DAV:"><D:prop><D:resourcetype/><D:getcontentlength/></D:prop></D:propfind>"#)
            .send()
            .map_err(|e| format!("PROPFIND 失败：{e}"))?;
        if !resp.status().is_success() {
            return Err(format!("PROPFIND 返回 {}", resp.status()));
        }
        let xml = resp.text().map_err(|e| e.to_string())?;
        parse_propfind(&xml, dir)
    }

    fn local_name(name: &str) -> &str {
        match name.rfind(':') {
            Some(i) => &name[i + 1..],
            None => name,
        }
    }

    fn parse_propfind(xml: &str, dir: &Url) -> Result<Vec<Item>, String> {
        use quick_xml::events::Event;
        use quick_xml::Reader;

        let dir_path = decoded_path(dir);
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        let mut out = Vec::new();
        let mut cur: Option<(String, Option<i64>, bool)> = None; // (href, size, is_dir)
        #[derive(PartialEq)]
        enum Cap {
            None,
            Href,
            Len,
        }
        let mut cap = Cap::None;

        loop {
            match reader.read_event().map_err(|e| e.to_string())? {
                Event::Start(e) => match local_name(e.name().as_ref()) {
                    "response" => cur = Some((String::new(), None, false)),
                    "href" if cur.is_some() => cap = Cap::Href,
                    "getcontentlength" if cur.is_some() => cap = Cap::Len,
                    _ => {}
                },
                Event::Empty(e) => {
                    if local_name(e.name().as_ref()) == "collection" {
                        if let Some(c) = cur.as_mut() {
                            c.2 = true;
                        }
                    }
                }
                Event::Text(t) => {
                    let text = quick_xml::escape::unescape(&t)
                        .map_err(|e| e.to_string())?
                        .to_string();
                    if let Some(c) = cur.as_mut() {
                        match cap {
                            Cap::Href => c.0 = text,
                            Cap::Len => c.1 = text.parse::<i64>().ok(),
                            Cap::None => {}
                        }
                    }
                }
                Event::End(e) => match local_name(e.name().as_ref()) {
                    "response" => {
                        if let Some((href, size, is_dir)) = cur.take() {
                            // href 可能是完整 URL 或绝对路径；统一解码为绝对路径
                            let abs = if href.starts_with("http") {
                                Url::parse(&href)
                                    .map(|u| decoded_path(&u))
                                    .unwrap_or(href)
                            } else {
                                percent_decode_str(&href).decode_utf8_lossy().into_owned()
                            };
                            // 跳过目录自身
                            if abs.trim_end_matches('/') == dir_path.trim_end_matches('/') {
                                continue;
                            }
                            if !abs.is_empty() {
                                out.push(Item { abs: abs.trim_end_matches('/').to_string(), is_dir, size: size.unwrap_or(0) });
                            }
                        }
                    }
                    "href" | "getcontentlength" => cap = Cap::None,
                    _ => {}
                },
                Event::Eof => break,
                _ => {}
            }
        }
        Ok(out)
    }

    fn decoded_path(u: &Url) -> String {
        percent_decode_str(u.path()).decode_utf8_lossy().into_owned()
    }

    /// 确保来源根 URL 以 / 结尾（join 语义需要）
    pub fn normalize_base(base: &str) -> Result<Url, String> {
        let mut u = Url::parse(base).map_err(|e| format!("WebDAV 地址无效：{e}"))?;
        if !u.path().ends_with('/') {
            u.set_path(&format!("{}/", u.path()));
        }
        Ok(u)
    }

    pub fn file_url(base: &Url, rel: &str) -> Url {
        base.join(rel).unwrap_or_else(|_| base.clone())
    }

    pub fn download(url: &Url, auth: Option<&Auth>, range: Option<(u64, u64)>) -> Result<Vec<u8>, String> {
        let mut req = http_client().get(url.as_str());
        if let Some(a) = auth {
            req = req.basic_auth(&a.username, Some(&a.password));
        }
        if let Some((s, e)) = range {
            req = req.header("Range", format!("bytes={s}-{e}"));
        }
        let resp = req.send().map_err(|e| format!("下载失败：{e}"))?;
        if !resp.status().is_success() {
            return Err(format!("下载返回 {}", resp.status()));
        }
        resp.bytes().map(|b| b.to_vec()).map_err(|e| e.to_string())
    }

    pub fn download_text(url: &Url, auth: Option<&Auth>) -> Result<Option<String>, String> {
        let mut req = http_client().get(url.as_str());
        if let Some(a) = auth {
            req = req.basic_auth(&a.username, Some(&a.password));
        }
        let resp = req.send().map_err(|e| format!("下载歌词失败：{e}"))?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !resp.status().is_success() {
            return Err(format!("下载歌词返回 {}", resp.status()));
        }
        let bytes = resp.bytes().map_err(|e| e.to_string())?;
        Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
    }
}

// ---------------------------------------------------------------- LAN 共享客户端

pub mod lan {
    use super::*;

    fn base_of(addr: &str) -> String {
        let addr = addr.trim();
        if addr.starts_with("http://") || addr.starts_with("https://") {
            addr.trim_end_matches('/').to_string()
        } else {
            format!("http://{}", addr.trim_end_matches('/'))
        }
    }

    fn get(base: &str, token: &str, path: &str) -> reqwest::blocking::RequestBuilder {
        http_client().get(format!("{base}{path}")).bearer_auth(token)
    }

    /// 握手并校验 token，返回 (库名, 曲目数)
    pub fn hello(addr: &str, token: &str) -> Result<(String, i64), String> {
        let base = base_of(addr);
        let resp = get(&base, token, "/api/hello")
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .map_err(|e| format!("连接失败：{e}"))?;
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err("配对码不正确".into());
        }
        if !resp.status().is_success() {
            return Err(format!("设备返回 {}", resp.status()));
        }
        let v: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
        let name = v["name"].as_str().unwrap_or("未知设备").to_string();
        let tracks = v["tracks"].as_i64().unwrap_or(0);
        Ok((name, tracks))
    }

    pub fn tracks_page(base: &str, token: &str, offset: i64, limit: i64) -> Result<Vec<RemoteTrack>, String> {
        let resp = get(base, token, &format!("/api/tracks?offset={offset}&limit={limit}"))
            .send()
            .map_err(|e| format!("拉取曲目失败：{e}"))?;
        if !resp.status().is_success() {
            return Err(format!("拉取曲目返回 {}", resp.status()));
        }
        resp.json().map_err(|e| e.to_string())
    }

    pub fn download_cover(base: &str, token: &str, remote_album_id: i64) -> Result<Vec<u8>, String> {
        let resp = get(base, token, &format!("/api/cover/{remote_album_id}"))
            .send()
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("封面返回 {}", resp.status()));
        }
        resp.bytes().map(|b| b.to_vec()).map_err(|e| e.to_string())
    }

    pub fn get_lrc(base: &str, token: &str, remote_track_id: i64) -> Result<Option<String>, String> {
        let resp = get(base, token, &format!("/api/lrc/{remote_track_id}"))
            .send()
            .map_err(|e| e.to_string())?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !resp.status().is_success() {
            return Err(format!("歌词返回 {}", resp.status()));
        }
        let bytes = resp.bytes().map_err(|e| e.to_string())?;
        Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
    }
}

// ---------------------------------------------------------------- mDNS 发现与广播

pub mod mdns {
    use super::*;
    use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};

    pub fn daemon<R: Runtime>(app: &AppHandle<R>) -> Result<mdns_sd::ServiceDaemon, String> {
        let state = app.state::<AppState>();
        let mut guard = state.mdns_daemon.lock().map_err(|e| e.to_string())?;
        if guard.is_none() {
            *guard = Some(ServiceDaemon::new().map_err(|e| format!("mDNS 初始化失败：{e}"))?);
        }
        // 借用后克隆出句柄（ServiceDaemon 是轻量句柄）
        Ok(guard.as_ref().unwrap().clone())
    }

    /// 注册共享服务，返回服务全名
    pub fn register<R: Runtime>(app: &AppHandle<R>, port: u16, name: &str) -> Result<String, String> {
        let daemon = daemon(app)?;
        let safe_name: String = name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
            .collect();
        let host = format!("{safe_name}.local.");
        let props: HashMap<String, String> = [("ver".to_string(), "1".to_string())].into();
        let service = ServiceInfo::new(SERVICE_TYPE, name, &host, (), port, Some(props))
            .map_err(|e| format!("mDNS 服务信息无效：{e}"))?;
        let fullname = service.get_fullname().to_string();
        daemon.register(service).map_err(|e| format!("mDNS 注册失败：{e}"))?;
        Ok(fullname)
    }

    pub fn unregister<R: Runtime>(app: &AppHandle<R>, fullname: &str) {
        if let Ok(daemon) = daemon(app) {
            let _ = daemon.unregister(fullname);
        }
    }

    pub fn browse_start<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
        let daemon = daemon(app)?;
        {
            let state = app.state::<AppState>();
            let mut b = state.browsing.lock().map_err(|e| e.to_string())?;
            if *b {
                return Ok(());
            }
            *b = true;
        }
        let receiver = daemon.browse(SERVICE_TYPE).map_err(|e| format!("mDNS 浏览失败：{e}"))?;
        let app2 = app.clone();
        std::thread::spawn(move || loop {
            match receiver.recv() {
                Ok(ServiceEvent::ServiceResolved(info)) => {
                    let id = info.get_fullname().to_string();
                    let name = id.split('.').next().unwrap_or("设备").to_string();
                    let host = info
                        .get_addresses()
                        .iter()
                        .next()
                        .map(|ip| ip.to_string())
                        .unwrap_or_else(|| info.get_hostname().to_string());
                    let port = info.get_port();
                    let _ = app2.emit(
                        "net:device_found",
                        serde_json::json!({ "id": id, "name": name, "host": host, "port": port }),
                    );
                }
                Ok(ServiceEvent::ServiceRemoved(_, fullname)) => {
                    let _ = app2.emit("net:device_lost", serde_json::json!({ "id": fullname }));
                }
                Ok(ServiceEvent::SearchStopped(_)) => break,
                Ok(_) => {}
                Err(_) => break,
            }
        });
        Ok(())
    }

    pub fn browse_stop(app: &AppHandle) {
        let state = app.state::<AppState>();
        if let Ok(daemon) = daemon(app) {
            let _ = daemon.stop_browse(SERVICE_TYPE);
        }
        let guard = state.browsing.lock();
        if let Ok(mut b) = guard {
            *b = false;
        }
    }
}

// ---------------------------------------------------------------- 共享服务端（axum）

pub mod share {
    use super::*;
    use axum::extract::{Path as AxPath, Query};
    use axum::http::HeaderMap;
    use axum::response::IntoResponse;
    use axum::routing::get;
    use axum::{Json, Router};

    fn check_auth(headers: &HeaderMap, token: &str) -> bool {
        headers
            .get(reqwest::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .map(|v| v == format!("Bearer {token}"))
            .unwrap_or(false)
    }

    fn unauthorized() -> axum::response::Response {
        (axum::http::StatusCode::UNAUTHORIZED, "unauthorized").into_response()
    }

    fn convert(resp: tauri::http::Response<Vec<u8>>) -> axum::response::Response {
        let (parts, body) = resp.into_parts();
        axum::response::Response::from_parts(parts, axum::body::Body::from(body))
    }

    pub fn start(app: &AppHandle) -> Result<(), String> {
        let state = app.state::<AppState>();
        if state.share.lock().map_err(|e| e.to_string())?.is_some() {
            return Err("共享已在运行".into());
        }
        let port: u16 = get_setting(app, "share_port")
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_PORT);
        let token = ensure_share_token(app)?;
        let name = get_setting(app, "share_name").unwrap_or_else(|| "LanMusic".into());

        let fullname = mdns::register(app, port, &name)?;
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        *state.share.lock().map_err(|e| e.to_string())? = Some(ShareHandle { shutdown: tx, port, mdns_fullname: fullname });
        drop(state);

        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            let router = build_router(app2.clone(), token);
            let bind = tokio::net::TcpListener::bind(("0.0.0.0", port)).await;
            match bind {
                Ok(listener) => {
                    let _ = app2.emit(
                        "share:status",
                        serde_json::json!({ "running": true, "port": port }),
                    );
                    let _ = axum::serve(listener, router)
                        .with_graceful_shutdown(async move {
                            let _ = rx.await;
                        })
                        .await;
                }
                Err(e) => {
                    let _ = app2.emit(
                        "share:status",
                        serde_json::json!({ "running": false, "error": format!("端口 {port} 绑定失败：{e}") }),
                    );
                }
            }
            // 清理运行句柄
            let state = app2.state::<AppState>();
            if let Some(handle) = state.share.lock().ok().and_then(|mut g| g.take()) {
                mdns::unregister(&app2, &handle.mdns_fullname);
            }
            let _ = app2.emit(
                "share:status",
                serde_json::json!({ "running": false, "port": port }),
            );
        });
        Ok(())
    }

    pub fn stop(app: &AppHandle) {
        let state = app.state::<AppState>();
        let guard = state.share.lock();
        if let Ok(mut g) = guard {
            if let Some(handle) = g.take() {
                let _ = handle.shutdown.send(());
            }
        }
    }

    pub fn is_running(app: &AppHandle) -> bool {
        let state = app.state::<AppState>();
        state.share.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    pub fn running_port(app: &AppHandle) -> Option<u16> {
        let state = app.state::<AppState>();
        state.share.lock().ok().and_then(|g| g.as_ref().map(|h| h.port))
    }

    fn build_router(app: AppHandle, token: String) -> Router {
        let token_hello = token.clone();
        let app_hello = app.clone();
        let app_tracks = app.clone();
        let token_tracks = token.clone();
        let app_stream = app.clone();
        let token_stream = token.clone();
        let app_cover = app.clone();
        let token_cover = token.clone();
        let app_lrc = app.clone();
        let token_lrc = token.clone();

        Router::new()
            .route(
                "/api/hello",
                get(move |headers: HeaderMap| async move {
                    if !check_auth(&headers, &token_hello) {
                        return unauthorized();
                    }
                    let tracks: i64 = {
                        let state = app_hello.state::<AppState>();
                        let conn = match state.db.lock() {
                            Ok(c) => c,
                            Err(_) => return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db").into_response(),
                        };
                        conn.query_row("SELECT COUNT(*) FROM tracks", [], |r| r.get(0)).unwrap_or(0)
                    };
                    let name = get_setting(&app_hello, "share_name").unwrap_or_else(|| "LanMusic".into());
                    Json(serde_json::json!({ "app": "lanmusic", "protocol": 1, "name": name, "tracks": tracks })).into_response()
                }),
            )
            .route(
                "/api/tracks",
                get(move |headers: HeaderMap, Query(q): Query<HashMap<String, String>>| async move {
                    if !check_auth(&headers, &token_tracks) {
                        return unauthorized();
                    }
                    let offset: i64 = q.get("offset").and_then(|v| v.parse().ok()).unwrap_or(0);
                    let limit: i64 = q.get("limit").and_then(|v| v.parse().ok()).unwrap_or(500).clamp(1, 1000);
                    let state = app_tracks.state::<AppState>();
                    let conn = match state.db.lock() {
                        Ok(c) => c,
                        Err(_) => return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db").into_response(),
                    };
                    let mut stmt = match conn.prepare(
                        "SELECT t.id, t.title, a.name, al.title, al2.name, t.year, t.track_no, t.duration, t.format, t.album_id, \
                                (t.has_embedded_lyrics = 1 OR l.track_id IS NOT NULL) \
                         FROM tracks t \
                         JOIN sources s ON s.id = t.source_id AND s.kind = 'local' AND s.enabled = 1 \
                         LEFT JOIN artists a ON a.id = t.artist_id \
                         LEFT JOIN albums al ON al.id = t.album_id \
                         LEFT JOIN artists al2 ON al2.id = al.artist_id \
                         LEFT JOIN lrc_files l ON l.track_id = t.id \
                         ORDER BY t.id LIMIT ?1 OFFSET ?2",
                    ) {
                        Ok(s) => s,
                        Err(e) => return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
                    };
                    let items: Vec<RemoteTrack> = stmt
                        .query_map(params![limit, offset], |r| {
                            Ok(RemoteTrack {
                                id: r.get(0)?,
                                title: r.get(1)?,
                                artist: r.get(2)?,
                                album: r.get(3)?,
                                album_artist: r.get(4)?,
                                year: r.get(5)?,
                                track_no: r.get(6)?,
                                duration: r.get(7)?,
                                format: r.get(8)?,
                                album_id: r.get(9)?,
                                has_lrc: r.get::<_, i64>(10).unwrap_or(0) != 0,
                            })
                        })
                        .ok()
                        .and_then(|rows| rows.collect::<Result<Vec<_>, _>>().ok())
                        .unwrap_or_default();
                    Json(items).into_response()
                }),
            )
            .route(
                "/api/stream/{id}",
                get(move |headers: HeaderMap, AxPath(id): AxPath<String>| async move {
                    if !check_auth(&headers, &token_stream) {
                        return unauthorized();
                    }
                    let Ok(id) = id.parse::<i64>() else {
                        return (axum::http::StatusCode::NOT_FOUND, "").into_response();
                    };
                    let range = headers
                        .get(reqwest::header::RANGE)
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_string);
                    let app2 = app_stream.clone();
                    let resp = tauri::async_runtime::spawn_blocking(move || {
                        scheme::serve_local_track_response(&app2, id, range.as_deref())
                    })
                    .await
                    .unwrap_or_else(|_| scheme::error_response());
                    convert(resp)
                }),
            )
            .route(
                "/api/cover/{id}",
                get(move |headers: HeaderMap, AxPath(id): AxPath<String>| async move {
                    if !check_auth(&headers, &token_cover) {
                        return unauthorized();
                    }
                    let Ok(id) = id.parse::<i64>() else {
                        return (axum::http::StatusCode::NOT_FOUND, "").into_response();
                    };
                    let app2 = app_cover.clone();
                    let bytes = tauri::async_runtime::spawn_blocking(move || {
                        crate::covers::ensure_cover(&app2, id).ok().flatten().and_then(|p| std::fs::read(p).ok())
                    })
                    .await
                    .ok()
                    .flatten();
                    match bytes {
                        Some(b) => ([(axum::http::header::CONTENT_TYPE, "image/jpeg")], b).into_response(),
                        None => (axum::http::StatusCode::NOT_FOUND, "").into_response(),
                    }
                }),
            )
            .route(
                "/api/lrc/{id}",
                get(move |headers: HeaderMap, AxPath(id): AxPath<String>| async move {
                    if !check_auth(&headers, &token_lrc) {
                        return unauthorized();
                    }
                    let Ok(id) = id.parse::<i64>() else {
                        return (axum::http::StatusCode::NOT_FOUND, "").into_response();
                    };
                    let app2 = app_lrc.clone();
                    let lrc = tauri::async_runtime::spawn_blocking(move || crate::lyrics::fetch(&app2, id))
                        .await
                        .ok()
                        .and_then(|r| r.ok())
                        .flatten();
                    match lrc {
                        Some(text) => ([(axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")], text).into_response(),
                        None => (axum::http::StatusCode::NOT_FOUND, "").into_response(),
                    }
                }),
            )
    }
}
