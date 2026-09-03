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

pub fn config_field(config: Option<&str>, key: &str) -> Option<String> {
    let cfg: serde_json::Value = serde_json::from_str(config?).ok()?;
    cfg.get(key)?.as_str().map(str::to_string)
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