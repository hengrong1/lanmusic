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
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
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
    /// Release 说明（markdown 原文，前端按纯文本渲染）
    pub notes: String,
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

fn http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())
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

/// 解析 `releases.atom` 取最新一条 release。
/// 返回 (tag 原文，如 `v0.5.2`；说明 HTML 片段；页面链接)。
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
                "link" if in_entry => {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == "href" {
                            link = attr.value.to_string();
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
        .header("Accept", "application/atom+xml")
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .text()
        .map_err(|e| e.to_string())?;

    let (tag, notes_html, html_url) = parse_latest_atom(&xml)?;
    let version = tag.trim_start_matches('v').to_string();
    if version_cmp(&version, current_version) != std::cmp::Ordering::Greater {
        return Ok(None);
    }
    // feed 不含资产列表：按发布约定拼接 + HEAD 探测存在性
    // （该 Release 未附带安装包时，前端退化为「前往下载页」）
    let (name, exe_url, sha_url) = asset_urls(&version);
    let has_asset = asset_exists(&client, &exe_url);
    if !has_asset {
        log::warn!("Release v{version} 未找到约定的安装包资产：{exe_url}（前端将退化为前往下载页）");
    }
    Ok(Some(ReleaseInfo {
        version,
        notes: strip_html(&notes_html),
        html_url,
        asset_name: has_asset.then(|| name.clone()),
        asset_url: has_asset.then(|| exe_url.clone()),
        // feed 不含资产大小 → 前端显示不定进度
        asset_size: None,
        sha256_url: has_asset.then(|| sha_url.clone()),
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
        .send()
        .map_err(|e| format!("校验文件下载失败：{e}"))?
        .error_for_status()
        .map_err(|e| format!("校验文件下载失败：{e}"))?
        .text()
        .map_err(|e| e.to_string())?;
    parse_sha256_text(&text)
}

/// 下载更新安装包到系统临时目录并做 SHA-256 校验，返回落地路径。
/// 进度经 `update:download-progress` 事件回传（200ms 节流）。
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

    let client = http_client()?;
    let mut resp = client
        .get(url)
        .send()
        .map_err(|e| format!("下载失败：{e}"))?
        .error_for_status()
        .map_err(|e| format!("下载失败：HTTP {e}"))?;
    let total = resp.content_length().or(size_hint).unwrap_or(0);

    log::info!("开始下载更新包 {file_name}（{total} 字节）");
    let started = std::time::Instant::now();
    let mut file = std::fs::File::create(&dest).map_err(|e| format!("无法创建下载文件：{e}"))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    let mut downloaded = 0u64;
    let mut last_emit = std::time::Instant::now();
    loop {
        let n = resp.read(&mut buf).map_err(|e| format!("下载中断：{e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| format!("写入失败：{e}"))?;
        hasher.update(&buf[..n]);
        downloaded += n as u64;
        if last_emit.elapsed() >= std::time::Duration::from_millis(200) {
            let _ = app.emit(PROGRESS_EVENT, DownloadProgress { downloaded, total });
            last_emit = std::time::Instant::now();
        }
    }
    drop(file);
    // 收尾事件把总量补齐（部分服务端不返回 content-length）
    let _ = app.emit(
        PROGRESS_EVENT,
        DownloadProgress {
            downloaded,
            total: if total == 0 { downloaded } else { total },
        },
    );
    let actual = format!("{:x}", hasher.finalize());

    if let Some(sha_url) = sha256_url {
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
    log::info!(
        "更新包已就绪：{}（{} 字节，耗时 {} ms）",
        dest.display(),
        downloaded,
        started.elapsed().as_millis()
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

#[cfg(test)]
mod tests {
    use super::{asset_urls, parse_latest_atom, parse_sha256_text, strip_html, version_cmp};
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
        let (tag, content, link) = parse_latest_atom(ATOM_FIXTURE).unwrap();
        assert_eq!(tag, "v0.5.2");
        assert_eq!(
            link,
            "https://github.com/hengrong1/lanmusic/releases/tag/v0.5.2"
        );
        // content 为还原实体后的 HTML 片段
        assert!(content.contains("<p>"), "应为 HTML 片段：{content}");
        assert!(content.contains("&amp;"), "XML 实体应已还原：{content}");
        assert!(!content.contains("v0.5.1"), "不应取到第二条 entry");
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
