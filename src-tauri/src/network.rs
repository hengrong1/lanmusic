//! WebDAV 客户端：PROPFIND 目录遍历、文件/文本下载（支持 Range 头部拉取）。
//!
//! 安全约定：WebDAV 凭证保存在 sources.config（JSON），不写入日志。

use std::sync::OnceLock;

use percent_encoding::percent_decode_str;
use url::Url;

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

/// 封面提取专用客户端：总超时 10s。封面提取在全局互斥队列里串行执行（见 covers.rs），
/// 复用 60s 超时的通用客户端时，一个不可达的远端会占住队列数分钟、拖垮整条封面管道。
fn cover_http_client() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("构建封面 HTTP 客户端失败")
    })
}

pub fn config_field(config: Option<&str>, key: &str) -> Option<String> {
    let cfg: serde_json::Value = serde_json::from_str(config?).ok()?;
    cfg.get(key)?.as_str().map(str::to_string)
}

/// 从 `err1(codes::DOWNLOAD_STATUS, "status", …)` 产出的信封里取出状态码。
fn download_status(err: &str) -> Option<u16> {
    let v: serde_json::Value =
        serde_json::from_str(err.strip_prefix(crate::error::PREFIX)?).ok()?;
    if v.get("code")?.as_str()? != crate::error::codes::DOWNLOAD_STATUS {
        return None;
    }
    v.get("params")?
        .get("status")?
        .as_str()?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// 该失败是否值得重试。
///
/// 429 / 401 / 403 一律不重试：OpenList 的 WebDAV 认证失败按 IP 计次
/// （`DefaultMaxAuthRetries = 5` → 锁 5 分钟），而且**每个被挡住的请求都会把封锁窗口
/// 重新续期**（见其 `server/webdav.go::WebDAVAuth`）。对 429 重试等于把锁定永久保持下去，
/// 所以这类失败必须立刻放弃。
pub fn is_retryable(err: &str) -> bool {
    !matches!(download_status(err), Some(429) | Some(401) | Some(403))
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
        /// 按来源恢复认证：username 取自 config，密码优先 config（旧库明文兼容），
        /// 否则从系统钥匙串读取（见 keyring.rs）
        pub fn from_source(config: Option<&str>, source_id: i64) -> Option<Auth> {
            let username = config_field(config, "username")?;
            let password = config_field(config, "password")
                .or_else(|| crate::keyring::get_password(source_id))?;
            Some(Auth { username, password })
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
            .map_err(|e| crate::error::err1(crate::error::codes::WEBDAV_PROPFIND_FAILED, "error", e))?;
        if !resp.status().is_success() {
            return Err(crate::error::err1(
                crate::error::codes::WEBDAV_PROPFIND_STATUS,
                "status",
                resp.status(),
            ));
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
        // (href, 长度的原始文本, is_dir)。文本逐事件累加，见下方 Text / GeneralRef 分支。
        let mut cur: Option<(String, String, bool)> = None;
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
                    "response" => cur = Some((String::new(), String::new(), false)),
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
                // 文本必须**累加**：quick-xml 会把实体引用（`&amp;` 等）切成独立事件，
                // 一个 href 可能对应多个 Text / GeneralRef。原先是赋值，导致含 `&` 的
                // 文件名只剩最后一段（「张碧晨&amp;王赫野 - 曲名」变成「王赫野 - 曲名」），
                // 之后按这个名字去 GET 必然 404。
                Event::Text(t) => {
                    let text = quick_xml::escape::unescape(&t)
                        .map_err(|e| e.to_string())?
                        .to_string();
                    if let Some(c) = cur.as_mut() {
                        match cap {
                            Cap::Href => c.0.push_str(&text),
                            Cap::Len => c.1.push_str(&text),
                            Cap::None => {}
                        }
                    }
                }
                Event::GeneralRef(r) => {
                    if let Some(c) = cur.as_mut() {
                        // 数字字符引用（&#38;）由 resolve_char_ref 解析；
                        // 预定义实体的 payload 是**实体名本身**（如 `amp`），
                        // 注意 BytesRef::xml10_content() 只做 EOL 归一化、并不还原实体，
                        // 必须自己映射回字符，否则 `&` 会变成 `amp` 的首字母 `a`。
                        let ch = if r.is_char_ref() {
                            r.resolve_char_ref().ok().flatten()
                        } else {
                            match r.into_inner().trim_matches(|ch| ch == '&' || ch == ';') {
                                "amp" => Some('&'),
                                "lt" => Some('<'),
                                "gt" => Some('>'),
                                "quot" => Some('"'),
                                "apos" => Some('\''),
                                _ => None,
                            }
                        };
                        if let Some(ch) = ch {
                            match cap {
                                Cap::Href => c.0.push(ch),
                                Cap::Len => c.1.push(ch),
                                Cap::None => {}
                            }
                        }
                    }
                }
                Event::End(e) => match local_name(e.name().as_ref()) {
                    "response" => {
                        if let Some((href, size_text, is_dir)) = cur.take() {
                            let size = size_text.trim().parse::<i64>().ok();
                            // href 可能是完整 URL 或绝对路径；统一解码为绝对路径
                            let abs = if href.starts_with("http") {
                                Url::parse(&href).map(|u| decoded_path(&u)).unwrap_or(href)
                            } else {
                                percent_decode_str(&href).decode_utf8_lossy().into_owned()
                            };
                            // 跳过目录自身
                            if abs.trim_end_matches('/') == dir_path.trim_end_matches('/') {
                                continue;
                            }
                            if !abs.is_empty() {
                                out.push(Item {
                                    abs: abs.trim_end_matches('/').to_string(),
                                    is_dir,
                                    size: size.unwrap_or(0),
                                });
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
        percent_decode_str(u.path())
            .decode_utf8_lossy()
            .into_owned()
    }

    /// 确保来源根 URL 以 / 结尾（join 语义需要）
    pub fn normalize_base(base: &str) -> Result<Url, String> {
        let mut u = Url::parse(base)
            .map_err(|e| crate::error::err1(crate::error::codes::WEBDAV_INVALID_URL, "error", e))?;
        if !u.path().ends_with('/') {
            u.set_path(&format!("{}/", u.path()));
        }
        Ok(u)
    }

    pub fn file_url(base: &Url, rel: &str) -> Url {
        base.join(rel).unwrap_or_else(|_| base.clone())
    }

    pub fn download(
        url: &Url,
        auth: Option<&Auth>,
        range: Option<(u64, u64)>,
    ) -> Result<Vec<u8>, String> {
        download_with(http_client(), url, auth, range)
    }

    /// 封面提取专用：短超时版本（失败代价低——封面缓存本就可重建，不值得等 60s）
    pub fn download_short(
        url: &Url,
        auth: Option<&Auth>,
        range: Option<(u64, u64)>,
    ) -> Result<Vec<u8>, String> {
        download_with(cover_http_client(), url, auth, range)
    }

    fn download_with(
        client: &'static reqwest::blocking::Client,
        url: &Url,
        auth: Option<&Auth>,
        range: Option<(u64, u64)>,
    ) -> Result<Vec<u8>, String> {
        let mut req = client.get(url.as_str());
        if let Some(a) = auth {
            req = req.basic_auth(&a.username, Some(&a.password));
        }
        if let Some((s, e)) = range {
            req = req.header("Range", format!("bytes={s}-{e}"));
        }
        let resp = req
            .send()
            .map_err(|e| crate::error::err1(crate::error::codes::DOWNLOAD_FAILED, "error", e))?;
        if !resp.status().is_success() {
            return Err(crate::error::err1(
                crate::error::codes::DOWNLOAD_STATUS,
                "status",
                resp.status(),
            ));
        }
        resp.bytes().map(|b| b.to_vec()).map_err(|e| e.to_string())
    }

    pub fn download_text(url: &Url, auth: Option<&Auth>) -> Result<Option<String>, String> {
        let mut req = http_client().get(url.as_str());
        if let Some(a) = auth {
            req = req.basic_auth(&a.username, Some(&a.password));
        }
        let resp = req.send().map_err(|e| {
            crate::error::err1(crate::error::codes::LYRICS_DOWNLOAD_FAILED, "error", e)
        })?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !resp.status().is_success() {
            return Err(crate::error::err1(
                crate::error::codes::LYRICS_DOWNLOAD_STATUS,
                "status",
                resp.status(),
            ));
        }
        let bytes = resp.bytes().map_err(|e| e.to_string())?;
        Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn base() -> Url {
            Url::parse("http://localhost:5244/dav/quark/").unwrap()
        }

        /// 文件名里的 `&` 在 XML 中是 `&amp;`，而 quick-xml 会把实体引用切成独立事件。
        /// href 必须按事件顺序**拼接**，否则只剩最后一段，得到「砍掉 & 前半截」的错名字，
        /// 进而 GET 变 404（扫描降级成「未知艺人」，播放失败）。
        #[test]
        fn propfind_keeps_ampersand_in_file_name() {
            let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
<D:response><D:href>/dav/quark/</D:href><D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype></D:prop></D:propstat></D:response>
<D:response><D:href>/dav/quark/%E5%BC%A0%E7%A2%A7%E6%99%A8&amp;%E7%8E%8B%E8%B5%AB%E9%87%8E%20-%20%E5%AD%97%E5%AD%97%E5%8F%A5%E5%8F%A5%20(Live).flac</D:href><D:propstat><D:prop><D:resourcetype/><D:getcontentlength>58794518</D:getcontentlength></D:prop></D:propstat></D:response>
</D:multistatus>"#;
            let items = parse_propfind(xml, &base()).unwrap();
            assert_eq!(items.len(), 1);
            assert_eq!(
                items[0].abs,
                "/dav/quark/张碧晨&王赫野 - 字字句句 (Live).flac"
            );
            assert_eq!(items[0].size, 58794518);
            assert!(!items[0].is_dir);
        }

        /// 多个实体（`杨丞琳&胡宇桐&李润祺`）同样要完整保留。
        #[test]
        fn propfind_keeps_multiple_ampersands() {
            let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
<D:response><D:href>/dav/quark/a&amp;b&amp;c.flac</D:href><D:propstat><D:prop><D:resourcetype/><D:getcontentlength>123</D:getcontentlength></D:prop></D:propstat></D:response>
</D:multistatus>"#;
            let items = parse_propfind(xml, &base()).unwrap();
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].abs, "/dav/quark/a&b&c.flac");
            assert_eq!(items[0].size, 123);
        }

        /// 其他预定义实体与数字字符引用也应还原。
        #[test]
        fn propfind_resolves_other_entities() {
            let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
<D:response><D:href>/dav/quark/a&lt;b&gt;&#38;c&quot;d.flac</D:href><D:propstat><D:prop><D:resourcetype/><D:getcontentlength>9</D:getcontentlength></D:prop></D:propstat></D:response>
</D:multistatus>"#;
            let items = parse_propfind(xml, &base()).unwrap();
            assert_eq!(items[0].abs, "/dav/quark/a<b>&c\"d.flac");
        }

        /// href 为绝对 URL 时同样保留完整路径。
        #[test]
        fn propfind_absolute_href_keeps_ampersand() {
            let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
<D:response><D:href>http://localhost:5244/dav/quark/x&amp;y.flac</D:href><D:propstat><D:prop><D:resourcetype/><D:getcontentlength>7</D:getcontentlength></D:prop></D:propstat></D:response>
</D:multistatus>"#;
            let items = parse_propfind(xml, &base()).unwrap();
            assert_eq!(items[0].abs, "/dav/quark/x&y.flac");
        }
    }
}
