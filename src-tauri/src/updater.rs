//! 应用内更新（自研，基于 GitHub Releases）。
//!
//! 检查更新走 **`releases.atom`**（而非 GitHub REST API）：
//! API 匿名访问限流 60 次/小时/IP，反复启动/测试极易触发
//! `403 rate limit exceeded`；atom feed 无此限制，且含版本号、说明与页面链接。
//! feed 不含资产列表，故资产地址按发布约定拼接（见 `asset_urls`），
//! 并用 HEAD 探测是否存在（不存在则前端退化为「前往下载页」）。
//!
//! 流程：
//!   1. 查最新 Release 版本并与当前版本比较（`latest_release`）；
//!   2. 应用内下载安装包 + SHA-256 校验（`download_installer`）；
//!   3. 静默运行安装包并在装完后自动拉起新版（`run_installer`，配合
//!      installer/LanMusic.iss 的 `/LAUNCH=1` 约定）。
//!
//! Release 资产约定（installer/build.ps1 产出并随包上传）：
//!   `LanMusic_<版本>_x64-setup.exe` + 同名 `.sha256` 校验文件。

use quick_xml::{events::Event, Reader};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

/// Release feed 仓库基址（拼接页面/资产地址用）
const REPO_URL: &str = "https://github.com/hengrong1/lanmusic";
/// 最新 Release 的 atom feed（**无 API 限流**）
const RELEASES_ATOM: &str = "https://github.com/hengrong1/lanmusic/releases.atom";
const USER_AGENT: &str = "LanMusic-Updater";
/// 下载进度事件名（payload 见 `DownloadProgress`）
pub const PROGRESS_EVENT: &str = "update:download-progress";

/// 最新 Release 信息（供前端引导下载/应用内更新）
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseInfo {
    /// 版本号（tag 去 v 前缀，如 `0.4.1`）
    pub version: String,
    /// Release 说明（GitHub 渲染 HTML 剥离后的纯文本，无结构；富文本展示用 notesHtml）
    pub notes: String,
    /// Release 说明（GitHub 渲染的 HTML 经 `sanitize_html` 净化，前端 v-html 富文本渲染；
    /// None = feed 无内容或净化后为空，前端回退纯文本 notes）
    pub notes_html: Option<String>,
    /// Release 页面地址（浏览器打开即下载入口）
    pub html_url: String,
    /// 安装包下载地址（`*_x64-setup.exe` 资产）；缺失时前端退化为「前往下载页」
    pub asset_url: Option<String>,
    /// 安装包文件名（展示/落盘用）
    pub asset_name: Option<String>,
    /// 安装包字节数（进度条总量；0/None 表示未知）
    pub asset_size: Option<u64>,
    /// SHA-256 校验文件地址（与安装包同名的 `.sha256` 资产）
    pub sha256_url: Option<String>,
}

/// 下载进度（经 `update:download-progress` 事件回传；total=0 表示未知）
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadProgress {
    downloaded: u64,
    total: u64,
}

/// 连接超时（建连阶段；下载体不设总超时——慢速网络下载 >120s 会被总超时杀掉，
/// 用户重下时进度条从 0 重来，表现为「进度往回跳」，v0.5.7 实测）
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
/// 下载体读空闲超时（两次收到数据之间的最大间隔，防连接假死后永久挂起）
const READ_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// 小请求（atom feed / HEAD 探测 / sha256 文件）专用总超时：这些请求体量小，
/// 给个总超时防挂死即可，与下载路径的长耗时无关
const SMALL_REQ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Windows 系统代理（IE/WinINET 设置，Clash/v2rayN 等写注册表的那份）。
/// reqwest 默认只认环境变量代理，GUI 代理软件设置的「系统代理」读不到，
/// 导致 GitHub 资产直连下载极慢——这里补读注册表。PAC（AutoConfigURL）不支持，忽略。
#[cfg(windows)]
fn system_proxy() -> Option<reqwest::Proxy> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
        .ok()?;
    let enabled: u32 = key.get_value("ProxyEnable").ok()?;
    if enabled != 1 {
        return None;
    }
    let server: String = key.get_value("ProxyServer").ok()?;
    // 两种格式：`127.0.0.1:7890`（全协议同一地址）或
    // `http=…;https=…;ftp=…;socks=…`（按协议区分）。https 流量优先取 https 代理，
    // 其次 http 代理（本地代理软件两者通常同端口）；socks 条目跳过（未启用 socks 特性）
    let url = if server.contains('=') {
        let (mut https_one, mut http_one) = (None, None);
        for kv in server.split(';') {
            let Some((k, v)) = kv.split_once('=') else { continue };
            match k.trim() {
                "https" => https_one = Some(v.trim().to_string()),
                "http" => http_one = Some(v.trim().to_string()),
                _ => {}
            }
        }
        https_one.or(http_one)?
    } else {
        server.trim().to_string()
    };
    if url.is_empty() {
        return None;
    }
    let full = if url.contains("://") { url } else { format!("http://{url}") };
    match reqwest::Proxy::all(&full) {
        Ok(p) => {
            log::info!("更新通道使用系统代理：{full}");
            Some(p)
        }
        Err(e) => {
            log::warn!("系统代理地址无法解析（{full}）：{e}");
            None
        }
    }
}

#[cfg(not(windows))]
fn system_proxy() -> Option<reqwest::Proxy> {
    None
}

/// 小请求客户端（atom / HEAD / sha256）：无总超时（总超时由各请求自带），带系统代理
fn http_client() -> Result<reqwest::blocking::Client, String> {
    let mut builder = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(CONNECT_TIMEOUT);
    if let Some(p) = system_proxy() {
        builder = builder.proxy(p);
    }
    builder.build().map_err(|e| e.to_string())
}

/// 下载客户端（异步）：不设总超时（慢速网络下载可能远超任何固定值），
/// 只限建连与读空闲，配合 Range 断点续传把「卡死/中断」转成续传而非重下
fn download_client() -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_IDLE_TIMEOUT);
    if let Some(p) = system_proxy() {
        builder = builder.proxy(p);
    }
    builder.build().map_err(|e| e.to_string())
}

/// 语义化版本比较（三段数字，非数字段/缺段按 0 补齐：0.4 == 0.4.0）。
/// 返回 a 与 b 的大小关系；解析失败的段按 0 处理，极端畸形 tag 退化为 Equal
/// （宁可不提示更新，也不误报）。
fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |v: &str| -> Vec<u64> {
        v.trim_start_matches('v')
            .split('.')
            .map(|seg| seg.trim().parse::<u64>().unwrap_or(0))
            .collect()
    };
    let (av, bv) = (parse(a), parse(b));
    let len = av.len().max(bv.len());
    for i in 0..len {
        let (x, y) = (
            av.get(i).copied().unwrap_or(0),
            bv.get(i).copied().unwrap_or(0),
        );
        match x.cmp(&y) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }
    std::cmp::Ordering::Equal
}

/// 从 feed 条目解析版本号（**三段数字，不带 v 前缀**）。
///
/// ⚠️ feed 的 `<title>` 是 **Release 名称**，用户可以任意命名（如「LanMusic v0.5.5」），
/// 不能当版本号用：一是界面会显示成 `vLanMusic v0.5.5`，二是 `asset_urls` 会拼出
/// `LanMusic_LanMusic v0.5.5_x64-setup.exe` 这种地址、HEAD 必然 404。
/// 所以优先取 `<link>` 里的 `releases/tag/<tag>`（与 tag 名同源），
/// 拿不到再退回「在标题里找第一个 n.n[.n] 片段」。
fn version_from_entry(title: &str, link: &str) -> Option<String> {
    if let Some(idx) = link.find("/releases/tag/") {
        let tag = &link[idx + "/releases/tag/".len()..];
        let tag = tag.split(['/', '?', '#']).next().unwrap_or_default();
        let v = tag.trim_start_matches(['v', 'V']).trim();
        if v.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return Some(v.to_string());
        }
    }
    extract_version(title)
}

/// 在任意文本里找第一个形如 `1.2` / `1.2.3` 的片段（Release 名带前缀时的兜底）。
/// 只认「数字 + 至少一个点 + 数字」，避免把名称里的孤立数字（如「LanMusic 2」）当版本。
fn extract_version(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        let mut dots = 0;
        while i < bytes.len() {
            if bytes[i].is_ascii_digit() {
                i += 1;
            } else if bytes[i] == b'.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
                dots += 1;
                i += 1;
            } else {
                break;
            }
        }
        if dots >= 1 {
            return Some(s[start..i].to_string());
        }
    }
    None
}

/// 解析 `releases.atom` 取最新一条 release。
/// 返回 (**Release 名称**（`<title>`，不是版本号，取版本用 `version_from_entry`）；
/// 说明 HTML 片段；页面链接)。
fn parse_latest_atom(xml: &str) -> Result<(String, String, String), String> {
    let mut reader = Reader::from_str(xml);
    let mut in_entry = false;
    let mut in_title = false;
    let mut in_content = false;
    let mut title = String::new();
    let mut content = String::new();
    let mut link = String::new();
    loop {
        match reader.read_event() {
            Err(e) => return Err(format!("解析 Release feed 失败：{e}")),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.name().as_ref() {
                "entry" if !in_entry => in_entry = true,
                "title" if in_entry => in_title = true,
                "content" if in_entry => in_content = true,
                // <link rel="alternate" type="text/html" href="…"/>
                // 只认 release 页链接（feed 可能还有别的 link，别让后面的覆盖掉）
                "link" if in_entry => {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == "href" {
                            let href = attr.value.to_string();
                            if link.is_empty() || href.contains("/releases/tag/") {
                                link = href;
                            }
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::Text(t)) if in_entry => {
                let text = quick_xml::escape::unescape(&t).map_err(|e| e.to_string())?;
                if in_title {
                    title.push_str(&text);
                }
                if in_content {
                    content.push_str(&text);
                }
            }
            Ok(Event::GeneralRef(r)) if in_entry => {
                // quick-xml 不还原预定义实体（payload 是实体名本身，见 network.rs 同款处理）
                let ch = if r.is_char_ref() {
                    r.resolve_char_ref().ok().flatten()
                } else {
                    match r.as_ref() {
                        "amp" => Some('&'),
                        "lt" => Some('<'),
                        "gt" => Some('>'),
                        "quot" => Some('"'),
                        "apos" => Some('\''),
                        _ => None,
                    }
                };
                if let Some(c) = ch {
                    if in_title {
                        title.push(c);
                    }
                    if in_content {
                        content.push(c);
                    }
                }
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                "entry" if in_entry => break, // 只取最新一条
                "title" => in_title = false,
                "content" => in_content = false,
                _ => {}
            },
            _ => {}
        }
    }
    if title.is_empty() {
        return Err("Release feed 中没有任何版本条目".into());
    }
    Ok((title, content, link))
}

/// 粗粒度 HTML 清洗：去标签 + 反转义常见实体，供前端按纯文本展示
/// （Release 说明在 feed 里是 HTML 片段）。块级标签转换为换行并折叠空行。
fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push('\n');
            }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// 轻量净化 GitHub 渲染的 Release HTML（供前端 v-html 富文本渲染）。
/// 纵深防御：GitHub 渲染管线本身已消毒（script/iframe/on* 在源头就不存在），
/// 此处兜底防 feed 被劫持或渲染策略变化。策略：
/// - `<script>` `<style>` `<iframe>` `<object>` `<embed>` `<svg>` `<math>` `<form>` `<button>` 整元素剔除
///   （含内部内容；找不到闭合标签则丢弃到结尾）
/// - `<img>` `<input>` `<video>` `<audio>` `<source>` `<track>` `<picture>` 标签剔除
///   （外链图片在本应用 CSP 的 img-src 白名单外，必然裂图；弹窗里也无展示价值）
/// - 标签属性剔除 `on*` 事件属性与 `javascript:`/`vbscript:` 协议 URL
/// - 其余标签与文本原样保留；纯 ASCII 结构扫描，多字节字符不受影响
fn sanitize_html(html: &str) -> Option<String> {
    if html.trim().is_empty() {
        return None;
    }
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut i = 0;
    while i < html.len() {
        let Some(rel) = lower[i..].find('<') else {
            out.push_str(&html[i..]);
            break;
        };
        let lt = i + rel;
        out.push_str(&html[i..lt]);
        let Some(gt_rel) = lower[lt..].find('>') else {
            out.push_str(&html[lt..]); // 残缺标签按文本透传
            break;
        };
        let gt = lt + gt_rel;
        let inner_raw = &html[lt + 1..gt]; // 原文（保留大小写）
        let inner_low = &lower[lt + 1..gt];
        let is_close = inner_low.starts_with('/');
        let name = inner_low
            .trim_start_matches('/')
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == ':'))
            .next()
            .unwrap_or("");
        if !is_close && matches!(name, "script" | "style" | "iframe" | "object" | "embed" | "svg" | "math" | "form" | "button") {
            // 整元素剔除：跳到对应闭合标签之后（找不到闭合则丢弃余下全部）
            let close = format!("</{name}");
            let base = gt + 1;
            if let Some(cr) = lower[base..].find(&close) {
                let after_close = base + cr + close.len();
                i = lower[after_close..]
                    .find('>')
                    .map_or(html.len(), |p| after_close + p + 1);
            } else {
                i = html.len();
            }
            continue;
        }
        if !is_close && matches!(name, "img" | "input" | "video" | "audio" | "source" | "track" | "picture") {
            i = gt + 1;
            continue;
        }
        // 闭合标签的属性被浏览器忽略，透传即可；开放标签重建并过滤属性
        out.push('<');
        if !is_close {
            out.push_str(&sanitize_attrs(inner_raw));
        } else {
            out.push_str(inner_raw);
        }
        out.push('>');
        i = gt + 1;
    }
    Some(out)
}

/// 重建标签的属性区（`inner` 含标签名与属性）：剔除 `on*` 事件属性，
/// 以及 `href`/`src` 等取值为 `javascript:`/`vbscript:` 协议的属性；其余原样保留。
fn sanitize_attrs(inner: &str) -> String {
    let name_end = inner
        .find(|c: char| c.is_ascii_whitespace())
        .unwrap_or(inner.len());
    let mut out = String::from(&inner[..name_end]);
    let rest = &inner[name_end..];
    let bytes = rest.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' {
            i += 1;
        }
        let attr_name = rest[start..i].to_ascii_lowercase();
        let mut seg_end = i;
        if i < bytes.len() && bytes[i] == b'=' {
            i += 1;
            if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1;
                }
            } else {
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
            }
            seg_end = i;
        }
        let seg = &rest[start..seg_end];
        let url_dropped = matches!(attr_name.as_str(), "href" | "src" | "xlink:href" | "action" | "formaction")
            && seg
                .split_once('=')
                .is_none_or(|(_, v)| {
                    // 先去引号再 trim：值形如 `" javascript:…"` 时引号内可能有前导空白
                    let v = v.trim().trim_matches(['"', '\'']).trim();
                    let v = v.to_ascii_lowercase();
                    v.starts_with("javascript:") || v.starts_with("vbscript:")
                });
        if !attr_name.starts_with("on") && !url_dropped {
            out.push(' ');
            out.push_str(seg);
        }
    }
    out
}

/// 按发布约定拼接资产地址（feed 不含资产列表）。
/// 返回 (文件名, 安装包 URL, 校验文件 URL)。
fn asset_urls(version: &str) -> (String, String, String) {
    let name = format!("LanMusic_{version}_x64-setup.exe");
    let base = format!("{REPO_URL}/releases/download/v{version}");
    let exe = format!("{base}/{name}");
    let sha = format!("{exe}.sha256");
    (name, exe, sha)
}

/// HEAD 探测资产可下载性：Release 未附带该资产时返回 false，
/// 前端据此退化为「前往下载页」，而不是让用户点到 404。
fn asset_exists(client: &reqwest::blocking::Client, url: &str) -> bool {
    client
        .head(url)
        .timeout(SMALL_REQ_TIMEOUT)
        .send()
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// 查询最新 Release 并与当前版本比较（走 `releases.atom`，**免 API 限流**）。
/// 远端更高才返回 Some；网络失败返回 Err（由调用方决定提示方式）。
pub fn latest_release(current_version: &str) -> Result<Option<ReleaseInfo>, String> {
    let client = http_client()?;
    let xml = client
        .get(RELEASES_ATOM)
        .timeout(SMALL_REQ_TIMEOUT)
        .header("Accept", "application/atom+xml")
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .text()
        .map_err(|e| e.to_string())?;

    let (title, notes_html, html_url) = parse_latest_atom(&xml)?;
    // 版本号不能取 <title>（那是 Release 名称，用户可任意写）——见 version_from_entry
    let version = version_from_entry(&title, &html_url)
        .ok_or_else(|| format!("无法从 Release feed 解析版本号（Release 名称：{title}）"))?;
    if version_cmp(&version, current_version) != std::cmp::Ordering::Greater {
        return Ok(None);
    }
    // feed 不含资产列表：按发布约定拼接 + HEAD 探测存在性。
    // ⚠️ releases.atom 连**草稿** Release 也会列出（tag 一推 CI 建草稿就出现），
    // 此时资产对匿名下载不开放（HEAD 404）→ 提示出来只会得到一个「前往下载页」死链
    // （草稿 release 页对未登录也是 404）。所以资产探测不到一律视为「发布未完成」，
    // 当作没有更新处理（宁可不提示，不误导）。副作用：真发布忘传资产时也收不到提示
    // （本来也升不了级），日志有留痕；HEAD 偶发网络错误同样落此分支，重新检查即可。
    let (name, exe_url, sha_url) = asset_urls(&version);
    if !asset_exists(&client, &exe_url) {
        log::warn!("Release v{version} 未找到可下载的安装包资产：{exe_url}（视为发布未完成/草稿，不提示更新）");
        return Ok(None);
    }
    // link 缺失时按 tag 约定补出页面地址（「前往下载页」按钮依赖它，不能是空串）
    let html_url = if html_url.is_empty() {
        format!("{REPO_URL}/releases/tag/v{version}")
    } else {
        html_url
    };
    Ok(Some(ReleaseInfo {
        version,
        notes: strip_html(&notes_html),
        notes_html: sanitize_html(&notes_html),
        html_url,
        asset_name: Some(name),
        asset_url: Some(exe_url),
        // feed 不含资产大小 → 前端显示不定进度
        asset_size: None,
        sha256_url: Some(sha_url),
    }))
}

/// 读取 `.sha256` 文件内容并解析出哈希值。
/// 兼容 `sha256sum` 输出（`<hex>  <file>`）与纯 hex 两种格式。
fn parse_sha256_text(text: &str) -> Result<String, String> {
    text.split_whitespace()
        .find(|t| t.len() == 64 && t.chars().all(|c| c.is_ascii_hexdigit()))
        .map(|s| s.to_ascii_lowercase())
        .ok_or_else(|| "校验文件格式无法识别（未找到 64 位十六进制摘要）".to_string())
}

fn fetch_sha256(client: &reqwest::blocking::Client, url: &str) -> Result<String, String> {
    let text = client
        .get(url)
        .timeout(SMALL_REQ_TIMEOUT)
        .send()
        .map_err(|e| format!("校验文件下载失败：{e}"))?
        .error_for_status()
        .map_err(|e| format!("校验文件下载失败：{e}"))?
        .text()
        .map_err(|e| e.to_string())?;
    parse_sha256_text(&text)
}

/// 分片并发数：GitHub 对单连接限速明显（国内直连尤其如此），多连接分片下载
/// 是绕开单连接限速的有效手段；走系统代理时同样适用
const SEGMENT_CONNECTIONS: usize = 8;
/// 单分片最大重试次数（重试带 Range 续传，不从头来）
const SEGMENT_RETRIES: usize = 4;
/// 单流整段重试次数（服务器不支持 Range 时的兜底路径）
const SINGLE_RETRIES: usize = 3;
/// 总大小低于该值不分片（分片收益覆盖不了开销），直接单流下载
const SEGMENT_MIN_TOTAL: u64 = 2 * 1024 * 1024;
/// 总大小超过该值不分片（分片收在内存里组装，防超大资产吃内存；安装包远小于此）
const SEGMENT_MAX_TOTAL: u64 = 256 * 1024 * 1024;

/// 解析 Content-Range 头的总大小：`bytes 0-0/12345` → 12345；`bytes 0-0/*` → None
fn content_range_total(value: &str) -> Option<u64> {
    value.rsplit('/').next()?.trim().parse::<u64>().ok()
}

/// 进度回传：200ms 节流。分片并发时多个下载任务共享同一份节流时钟与总量
struct ProgressEmit {
    app: AppHandle,
    total: u64,
    last: Mutex<std::time::Instant>,
}

impl ProgressEmit {
    fn emit(&self, downloaded: u64) {
        // try_lock：拿不到锁说明另一分片刚发过，跳过本次即可（200ms 节奏不受影响）
        if let Ok(mut last) = self.last.try_lock() {
            if last.elapsed() >= std::time::Duration::from_millis(200) {
                let _ = self.app.emit(
                    PROGRESS_EVENT,
                    DownloadProgress { downloaded, total: self.total },
                );
                *last = std::time::Instant::now();
            }
        }
    }

    /// 收尾事件：必定发送，总量未知（0）时用实际字节数补齐
    fn emit_final(&self, downloaded: u64) {
        let total = if self.total == 0 { downloaded } else { self.total };
        let _ = self
            .app
            .emit(PROGRESS_EVENT, DownloadProgress { downloaded, total });
    }
}

/// 重试间隔（线性退避）。阻塞 sleep 会短暂占用一个 runtime 工作线程——
/// 分片重试是低频路径，多线程 runtime 下可接受（避免为此引入 tokio 直接依赖）
fn retry_wait(attempt: usize) {
    std::thread::sleep(std::time::Duration::from_millis(300 * attempt as u64));
}

/// 探测服务器是否支持 Range 断点续传，并顺带取文件总大小（Content-Range 的 total）
async fn probe_range(client: &reqwest::Client, url: &str) -> (bool, Option<u64>) {
    match client
        .get(url)
        .header("Range", "bytes=0-0")
        .timeout(SMALL_REQ_TIMEOUT)
        .send()
        .await
    {
        Ok(resp) if resp.status() == reqwest::StatusCode::PARTIAL_CONTENT => {
            let total = resp
                .headers()
                .get(reqwest::header::CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .and_then(content_range_total);
            (true, total)
        }
        Ok(resp) => {
            log::info!("服务器不支持 Range（HTTP {}），走单流下载", resp.status());
            (false, None)
        }
        Err(e) => {
            log::warn!("Range 探测失败（退化为单流下载）：{e}");
            (false, None)
        }
    }
}

/// 单分片下载：[start, end] 闭区间；buf 保留已收字节做断点续传。
/// 全局进度计数由多个分片共享（Arc<AtomicU64>），分片内重试不倒退——
/// 除非服务器无视 Range 回 200，此时该分片清零重收（进度同步回退，
/// 前端有单调钳制兜底，进度条不往回走）。
async fn download_segment(
    client: &reqwest::Client,
    url: &str,
    start: u64,
    end: u64,
    buf: &mut Vec<u8>,
    progress: &Arc<AtomicU64>,
    emit: &Arc<ProgressEmit>,
) -> Result<(), String> {
    let expected = (end - start + 1) as usize;
    for attempt in 1..=SEGMENT_RETRIES {
        if attempt > 1 {
            retry_wait(attempt - 1);
        }
        let got = buf.len() as u64;
        let range = format!("bytes={}-{}", start + got, end);
        let send = client.get(url).header("Range", &range).send().await;
        // 服务器必须回 206（Partial Content）；200 意味着 Range 被无视，
        // 读下去会是整段内容而非剩余段，只能清零重收
        let status_ok =
            matches!(&send, Ok(r) if r.status() == reqwest::StatusCode::PARTIAL_CONTENT);
        if !status_ok {
            if got > 0 {
                progress.fetch_sub(got, Ordering::Relaxed);
                buf.clear();
            }
            let reason = match &send {
                Ok(r) => format!("HTTP {}", r.status()),
                Err(e) => e.to_string(),
            };
            log::warn!("分片 {range} 未按续传返回（{reason}，第 {attempt} 次）");
            continue;
        }
        let mut resp = send.unwrap();
        loop {
            match resp.chunk().await {
                Ok(Some(chunk)) => {
                    buf.extend_from_slice(&chunk);
                    progress.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                    emit.emit(progress.load(Ordering::Relaxed));
                }
                Ok(None) => break,
                Err(e) => {
                    // 读空闲超时 / 连接被重置：保留已收字节，续传重试
                    log::warn!("分片 {range} 读取中断（第 {attempt} 次）：{e}");
                    break;
                }
            }
        }
        if buf.len() >= expected {
            buf.truncate(expected);
            return Ok(());
        }
        // 连接提前关闭或中途出错 → 循环顶部退避后续传
    }
    Err(format!(
        "分片 {}-{} 下载失败（已重试 {SEGMENT_RETRIES} 次，已收 {} 字节）",
        start + buf.len() as u64,
        end,
        buf.len()
    ))
}

/// 分片并发下载：均切 {SEGMENT_CONNECTIONS} 片，各自带续传重试；
/// 全部完成后按偏移组装到 dest。返回总字节数（= total）。
async fn download_segmented(
    client: &reqwest::Client,
    url: &str,
    total: u64,
    dest: &Path,
    progress: &Arc<AtomicU64>,
    emit: &Arc<ProgressEmit>,
) -> Result<u64, String> {
    let seg = total / SEGMENT_CONNECTIONS as u64;
    let mut ranges: Vec<(u64, u64)> = Vec::with_capacity(SEGMENT_CONNECTIONS);
    let mut start = 0u64;
    for i in 0..SEGMENT_CONNECTIONS {
        let end = if i == SEGMENT_CONNECTIONS - 1 {
            total - 1
        } else {
            start + seg - 1
        };
        ranges.push((start, end));
        start = end + 1;
    }

    let mut handles = Vec::with_capacity(SEGMENT_CONNECTIONS);
    for (s, e) in ranges {
        let client = client.clone();
        let url = url.to_string();
        let progress = Arc::clone(progress);
        let emit = Arc::clone(emit);
        handles.push(tauri::async_runtime::spawn(async move {
            let mut buf = Vec::with_capacity((e - s + 1) as usize);
            download_segment(&client, &url, s, e, &mut buf, &progress, &emit).await?;
            Ok::<_, String>((s, buf))
        }));
    }

    let mut parts = Vec::with_capacity(handles.len());
    for h in handles {
        match h.await {
            Ok(Ok(part)) => parts.push(part),
            Ok(Err(e)) => return Err(e),
            Err(e) => return Err(format!("下载任务异常退出: {e}")),
        }
    }
    parts.sort_by_key(|(s, _)| *s);

    let mut file = std::fs::File::create(dest).map_err(|e| format!("无法创建下载文件：{e}"))?;
    for (offset, buf) in &parts {
        file.seek(SeekFrom::Start(*offset))
            .map_err(|e| format!("写入失败：{e}"))?;
        file.write_all(buf).map_err(|e| format!("写入失败：{e}"))?;
    }
    file.flush().map_err(|e| format!("写入失败：{e}"))?;
    Ok(total)
}

/// 单流下载兜底（无 Range 支持或文件太小）：整段下载，失败从头重试。
/// 服务器不回 content-length 时以「读到流末尾」为完成标志（完整性由 SHA-256 终审）。
async fn download_single_stream(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    progress: &Arc<AtomicU64>,
    emit: &Arc<ProgressEmit>,
) -> Result<u64, String> {
    for attempt in 1..=SINGLE_RETRIES {
        if attempt > 1 {
            retry_wait(attempt - 1);
        }
        progress.store(0, Ordering::Relaxed); // 无续传能力，只能整段重来
        let mut resp = match client.get(url).send().await {
            Ok(r) => match r.error_for_status() {
                Ok(r) => r,
                Err(e) => return Err(format!("下载失败：HTTP {e}")),
            },
            Err(e) => {
                log::warn!("下载请求失败（第 {attempt} 次）：{e}");
                continue;
            }
        };
        let expected = resp.content_length();
        let mut file = std::fs::File::create(dest).map_err(|e| format!("无法创建下载文件：{e}"))?;
        let mut downloaded = 0u64;
        loop {
            match resp.chunk().await {
                Ok(Some(chunk)) => {
                    file.write_all(&chunk).map_err(|e| format!("写入失败：{e}"))?;
                    downloaded += chunk.len() as u64;
                    progress.store(downloaded, Ordering::Relaxed);
                    emit.emit(downloaded);
                }
                Ok(None) => break,
                Err(e) => {
                    log::warn!("下载读取中断（第 {attempt} 次）：{e}");
                    downloaded = 0;
                    break;
                }
            }
        }
        let complete = match expected {
            Some(len) => downloaded == len,
            None => downloaded > 0,
        };
        if complete {
            file.flush().map_err(|e| format!("写入失败：{e}"))?;
            return Ok(downloaded);
        }
    }
    Err("下载失败（已重试 3 次）".into())
}

/// 流式计算文件 SHA-256（分片下载组装完的整包文件）
fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("无法打开下载文件：{e}"))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 256 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| format!("读取下载文件失败：{e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// 下载更新安装包到系统临时目录并做 SHA-256 校验，返回落地路径。
/// 进度经 `update:download-progress` 事件回传（200ms 节流）。
/// 下载策略：支持 Range 且文件大小合适时分片并发（含断点续传），否则单流整段重试。
/// 旧实现给客户端设了 120s 总超时：慢速网络下下载必被杀掉、用户重下进度从 0 重来
/// （「进度条往回跳」的根源），现已改为只限建连/读空闲，不限总时长。
/// 校验失败会删除文件并返回 Err（绝不安装未通过校验的包）。
pub fn download_installer(
    app: &AppHandle,
    url: &str,
    sha256_url: Option<&str>,
    size_hint: Option<u64>,
) -> Result<PathBuf, String> {
    // 文件名只取 URL 末段（asset 名来自远端 JSON，防路径穿越）
    let raw_name = url.rsplit('/').next().unwrap_or_default();
    let file_name = Path::new(raw_name)
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| s.ends_with(".exe"))
        .ok_or_else(|| "安装包地址异常（非 exe 资产）".to_string())?
        .to_string();
    let dest = std::env::temp_dir().join(&file_name);

    // 复用已下载的安装包：文件名按版本固定（temp 里同版本永远只有一个文件），
    // 已存在且 SHA-256 与校验资产一致时直接跳过整包下载——
    // 「下载完成后重新检查更新再点下载」不再浪费流量；无校验资产或校验不符则重新下载覆盖。
    'reuse: {
        let Some(sha_url) = sha256_url else { break 'reuse };
        if !dest.is_file() {
            break 'reuse;
        }
        let Ok(existing) = sha256_file(&dest) else { break 'reuse };
        let Ok(client) = http_client() else { break 'reuse };
        if fetch_sha256(&client, sha_url).as_deref() == Ok(existing.as_str()) {
            log::info!("临时目录已有校验一致的安装包，跳过下载：{}", dest.display());
            return Ok(dest);
        }
        log::info!("临时目录已有同名安装包但校验不符，重新下载覆盖");
    }

    let client = download_client()?;
    let started = std::time::Instant::now();
    log::info!("开始下载更新包 {file_name}");

    // 下载体不设总超时，卡死由读空闲超时兜底；block_on 挂在 spawn_blocking
    // 线程上（见 commands.rs），不占 UI 线程
    let downloaded = tauri::async_runtime::block_on(async {
        let (range_ok, probed_total) = probe_range(&client, url).await;
        let total = probed_total.or(size_hint).unwrap_or(0);
        let progress = Arc::new(AtomicU64::new(0));
        let emit = Arc::new(ProgressEmit {
            app: app.clone(),
            total,
            last: Mutex::new(std::time::Instant::now()),
        });

        let bytes = if range_ok && total >= SEGMENT_MIN_TOTAL && total <= SEGMENT_MAX_TOTAL {
            log::info!("服务器支持断点续传，分 {SEGMENT_CONNECTIONS} 片并发下载（{total} 字节）");
            download_segmented(&client, url, total, &dest, &progress, &emit).await?
        } else {
            if range_ok && total > SEGMENT_MAX_TOTAL {
                log::warn!("文件 {total} 字节超出分片上限，走单流下载");
            }
            download_single_stream(&client, url, &dest, &progress, &emit).await?
        };
        // 收尾事件把总量补齐（部分服务端不返回 content-length）
        emit.emit_final(bytes);
        Ok::<u64, String>(bytes)
    })?;

    let actual = sha256_file(&dest)?;

    if let Some(sha_url) = sha256_url {
        let client = http_client()?;
        let expected = fetch_sha256(&client, sha_url)?;
        if expected != actual {
            let _ = std::fs::remove_file(&dest);
            log::error!("更新包校验失败（期望 {expected}，实际 {actual}），已删除下载文件");
            return Err("安装包 SHA-256 校验失败，已删除下载文件".into());
        }
        log::info!("更新包下载并校验通过：{file_name}（{actual}）");
    } else {
        log::warn!("更新包缺少 .sha256 校验资产，已跳过校验（建议补齐发布流程）");
    }
    let secs = started.elapsed().as_secs_f64().max(0.001);
    log::info!(
        "更新包已就绪：{}（{} 字节，{:.1}s，平均 {:.2} MB/s）",
        dest.display(),
        downloaded,
        secs,
        downloaded as f64 / 1024.0 / 1024.0 / secs
    );
    Ok(dest)
}

/// 运行已下载的安装包（静默安装 + 装完自动启动新版，配合 .iss 的 `/LAUNCH=1`）。
/// 仅接受系统临时目录下的 `LanMusic*setup.exe`——防前端被注入后借此执行任意程序。
pub fn run_installer(path: &str) -> Result<(), String> {
    let p = PathBuf::from(path);
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let in_temp = p
        .parent()
        .map(|d| d == std::env::temp_dir())
        .unwrap_or(false);
    if !in_temp || !name.starts_with("LanMusic") || !name.ends_with(".exe") {
        log::warn!("拒绝运行非法路径的安装包：{path}");
        return Err("安装包路径不合法".into());
    }
    if !p.is_file() {
        return Err("安装包不存在（可能已被清理），请重新下载".into());
    }
    std::process::Command::new(&p)
        .args(["/SILENT", "/LAUNCH=1"])
        .spawn()
        .map_err(|e| format!("无法启动安装程序：{e}"))?;
    log::info!("已启动安装程序（静默 + 装完自动启动）：{}", p.display());
    Ok(())
}

/// 启动清理：删除系统临时目录中**旧于当前版本**的更新安装包
/// （`LanMusic_<版本>_x64-setup.exe`；其他平台临时目录不会有此文件名，空跑无害）。
/// 应用内更新成功后旧版本安装包会残留在 temp（Inno Setup 安装程序不自删临时文件），
/// 逐版本升级会越积越多；等于/新于当前版本的文件保留——当前版本同名包可能是
/// 已下载待安装的副本（配合 download_installer 的校验复用），更高版本可能是刚取回的新包。
pub fn cleanup_old_installers() {
    let current = env!("CARGO_PKG_VERSION");
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    let mut removed = 0usize;
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else { continue };
        let Some(ver) = name
            .strip_prefix("LanMusic_")
            .and_then(|s| s.strip_suffix("_x64-setup.exe"))
        else {
            continue;
        };
        if version_cmp(ver, current) != std::cmp::Ordering::Less {
            continue;
        }
        if std::fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    if removed > 0 {
        log::info!("已清理临时目录中旧版本的更新安装包 {removed} 个");
    }
}

#[cfg(test)]
mod tests {
    use super::{
        asset_urls, parse_latest_atom, parse_sha256_text, sanitize_html, strip_html, version_cmp,
        version_from_entry,
    };
    use std::cmp::Ordering;

    #[test]
    fn v_prefix_and_missing_patch() {
        assert_eq!(version_cmp("v0.4.1", "0.4.0"), Ordering::Greater);
        assert_eq!(version_cmp("0.4", "0.4.0"), Ordering::Equal);
    }

    #[test]
    fn numeric_not_lexicographic() {
        assert_eq!(version_cmp("0.10.0", "0.9.0"), Ordering::Greater);
        assert_eq!(version_cmp("0.4.10", "0.4.9"), Ordering::Greater);
    }

    #[test]
    fn older_and_equal() {
        assert_eq!(version_cmp("0.3.9", "0.4.0"), Ordering::Less);
        assert_eq!(version_cmp("v0.4.0", "0.4.0"), Ordering::Equal);
    }

    #[test]
    fn malformed_segments_fall_back_to_zero() {
        assert_eq!(version_cmp("0.4.x", "0.4.0"), Ordering::Equal);
        assert_eq!(version_cmp("", "0.0.0"), Ordering::Equal);
    }

    #[test]
    fn content_range_total_parses_header() {
        assert_eq!(super::content_range_total("bytes 0-0/12345"), Some(12345));
        assert_eq!(super::content_range_total("bytes 0-0/*"), None);
        assert_eq!(super::content_range_total("bytes 0-0/"), None);
        assert_eq!(super::content_range_total("garbage"), None);
    }

    #[test]
    fn sha256_text_accepts_both_formats() {
        let hex = "a".repeat(64);
        // 纯 hex
        assert_eq!(parse_sha256_text(&hex).unwrap(), hex);
        // sha256sum 输出：<hex>  <file>（大写也归一化为小写）
        let upper = "A".repeat(64);
        assert_eq!(parse_sha256_text(&format!("{upper}  LanMusic_0.4.1_x64-setup.exe\n")).unwrap(), hex);
        // 不像摘要的内容应报错
        assert!(parse_sha256_text("not a hash").is_err());
    }

    /// 真实 releases.atom 片段（截取前两条 entry；content 是转义后的 HTML）
    const ATOM_FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xml:lang="en-US">
  <title>Release notes from lanmusic</title>
  <entry>
    <id>tag:github.com,2008:Repository/1/v0.5.2</id>
    <link rel="alternate" type="text/html" href="https://github.com/hengrong1/lanmusic/releases/tag/v0.5.2"/>
    <title>v0.5.2</title>
    <content type="html">&lt;p&gt;修复检查更新限流 &amp;amp; 其他&lt;/p&gt;</content>
  </entry>
  <entry>
    <title>v0.5.1</title>
    <content type="html">&lt;p&gt;旧版本&lt;/p&gt;</content>
  </entry>
</feed>"#;

    #[test]
    fn parses_first_atom_entry_only() {
        let (title, content, link) = parse_latest_atom(ATOM_FIXTURE).unwrap();
        assert_eq!(title, "v0.5.2");
        assert_eq!(
            link,
            "https://github.com/hengrong1/lanmusic/releases/tag/v0.5.2"
        );
        // 版本号从 link 里的 tag 取（明文 tag 与 Release 名一致时也走这条路）
        assert_eq!(version_from_entry(&title, &link).as_deref(), Some("0.5.2"));
        // content 为还原实体后的 HTML 片段
        assert!(content.contains("<p>"), "应为 HTML 片段：{content}");
        assert!(content.contains("&amp;"), "XML 实体应已还原：{content}");
        assert!(!content.contains("v0.5.1"), "不应取到第二条 entry");
    }

    /// Release 被命名过（如「LanMusic v0.5.5」）时，版本必须来自 tag 而不是名称
    #[test]
    fn version_comes_from_tag_not_release_name() {
        let link = "https://github.com/hengrong1/lanmusic/releases/tag/v0.5.5";
        assert_eq!(
            version_from_entry("LanMusic v0.5.5", link).as_deref(),
            Some("0.5.5")
        );
        // 名称里没有可解析版本时，仍以 tag 为准
        assert_eq!(
            version_from_entry("第一个正式版", link).as_deref(),
            Some("0.5.5")
        );
        // tag 自带的大写 V 前缀同样要去掉
        assert_eq!(
            version_from_entry(
                "LanMusic",
                "https://github.com/x/y/releases/tag/V0.6.0"
            )
            .as_deref(),
            Some("0.6.0")
        );
    }

    #[test]
    fn version_falls_back_to_title_without_link() {
        assert_eq!(
            version_from_entry("Release v0.5.5", "").as_deref(),
            Some("0.5.5")
        );
        // 名称里的孤立数字不算版本，找「数字.数字」片段
        assert_eq!(
            version_from_entry("LanMusic 2 (v0.6.0)", "").as_deref(),
            Some("0.6.0")
        );
        // 完全解析不出 → None（调用方报错，而不是拿错误的版本号去拼资产地址）
        assert_eq!(version_from_entry("LanMusic", ""), None);
        assert_eq!(version_from_entry("LanMusic 2", ""), None);
    }

    #[test]
    fn strip_html_yields_plain_text() {
        assert_eq!(
            strip_html("<p>第一行</p><ul><li>项一</li><li>项二</li></ul>"),
            "第一行\n项一\n项二"
        );
        assert_eq!(strip_html("<p>a &amp; b</p>"), "a & b");
        assert_eq!(strip_html(""), "");
    }

    #[test]
    fn sanitize_keeps_safe_markup() {
        let s = sanitize_html(
            r#"<p>修复 <strong>限流</strong> 问题</p><ul><li>项一</li><li>项二</li></ul><a href="https://github.com/x" target="_blank">链接</a>"#,
        )
        .unwrap();
        assert!(s.contains("<p>修复 <strong>限流</strong> 问题</p>"));
        assert!(s.contains("<ul><li>项一</li><li>项二</li></ul>"));
        assert!(s.contains(r#"href="https://github.com/x""#));
        assert!(s.contains(r#"target="_blank""#));
    }

    #[test]
    fn sanitize_drops_dangerous_elements() {
        let s = sanitize_html(
            r#"<p>前</p><script>alert(1)</script><p>中</p><style>.x{color:red}</style><iframe src="https://evil.example"></iframe><img src="https://github.com/a.png" onerror="alert(1)"><p>后</p>"#,
        )
        .unwrap();
        assert!(!s.contains("alert"));
        assert!(!s.contains("<script") && !s.contains("</script"));
        assert!(!s.contains(".x") && !s.contains("<style"));
        assert!(!s.contains("iframe") && !s.contains("evil.example"));
        assert!(!s.contains("<img") && !s.contains("onerror"));
        assert!(s.contains("前") && s.contains("中") && s.contains("后"), "安全内容应保留：{s}");
    }

    #[test]
    fn sanitize_drops_event_handlers_and_js_urls() {
        let s = sanitize_html(
            r#"<p onclick="alert(1)" title="ok">x</p><a href=" javascript:alert(1)">y</a>"#,
        )
        .unwrap();
        assert!(!s.contains("onclick"), "{s}");
        assert!(s.contains(r#"title="ok""#), "{s}");
        assert!(!s.to_ascii_lowercase().contains("javascript:"), "{s}");
        assert!(s.contains("<p title=\"ok\">x</p>"), "{s}");
    }

    #[test]
    fn sanitize_handles_unclosed_and_empty() {
        // 无闭合 script：丢弃到结尾
        assert_eq!(sanitize_html("<p>说明</p><script>bad").unwrap(), "<p>说明</p>");
        // 纯空白 → None（前端回退纯文本）
        assert_eq!(sanitize_html("   "), None);
        // 残缺标签按文本透传
        assert_eq!(sanitize_html("a < b").unwrap(), "a < b");
    }

    #[test]
    fn asset_urls_follow_release_convention() {
        let (name, exe, sha) = asset_urls("0.5.2");
        assert_eq!(name, "LanMusic_0.5.2_x64-setup.exe");
        assert_eq!(
            exe,
            "https://github.com/hengrong1/lanmusic/releases/download/v0.5.2/LanMusic_0.5.2_x64-setup.exe"
        );
        assert_eq!(sha, format!("{exe}.sha256"));
    }

    #[test]
    fn atom_without_entries_is_error() {
        assert!(parse_latest_atom("<feed><title>x</title></feed>").is_err());
    }
}
