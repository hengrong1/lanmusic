//! 应用内更新（自研，基于 GitHub Releases）。
//!
//! 背景：Windows 打包从 Tauri NSIS 切换到 Inno Setup（见 installer/），
//! bundle.targets 置空后标准 updater（签名 latest.json）没有更新包可用，
//! 改为自研更新流程：
//!   1. 查询 GitHub 最新 Release 并与当前版本比较（`latest_release`）；
//!   2. 应用内下载安装包 + SHA-256 校验（`download_installer`）；
//!   3. 静默运行安装包并在装完后自动拉起新版（`run_installer`，配合
//!      installer/LanMusic.iss 的 `/LAUNCH=1` 约定）。
//!
//! Release 资产约定（installer/build.ps1 产出并随包上传）：
//!   `LanMusic_<版本>_x64-setup.exe` + 同名 `.sha256` 校验文件。
//! 缺校验资产仍可更新（只记 warn），缺安装包资产则前端退化为「前往 Release 页」。
//!
//! API：`GET /repos/{owner}/{repo}/releases/latest`（公开仓库免鉴权；
//! GitHub API 强制要求 User-Agent，缺失返回 403）。

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

const RELEASE_API: &str = "https://api.github.com/repos/hengrong1/lanmusic/releases/latest";
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

/// 从 Release 资产列表中挑安装包与配套校验文件。
/// 安装包判定：文件名以 `_x64-setup.exe` 结尾（installer 输出约定）；
/// 校验文件：与安装包同名的 `.sha256` 资产。
fn pick_assets(
    assets: &[serde_json::Value],
) -> (Option<(String, String, u64)>, Option<String>) {
    let name_of = |a: &serde_json::Value| a["name"].as_str().unwrap_or_default().to_string();
    let url_of = |a: &serde_json::Value| {
        a["browser_download_url"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    };
    let installer = assets
        .iter()
        .find(|a| name_of(a).ends_with("_x64-setup.exe"))
        .map(|a| (name_of(a), url_of(a), a["size"].as_u64().unwrap_or(0)));
    let sha = installer.as_ref().and_then(|(name, _, _)| {
        let target = format!("{name}.sha256");
        assets
            .iter()
            .find(|a| name_of(a) == target)
            .map(|a| url_of(a))
    });
    (installer, sha)
}

/// 查询 GitHub 最新 Release 并与当前版本比较。
/// 远端更高才返回 Some；网络失败返回 Err（由调用方决定提示方式）。
pub fn latest_release(current_version: &str) -> Result<Option<ReleaseInfo>, String> {
    let client = http_client()?;
    let resp: serde_json::Value = client
        .get(RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())?;

    let tag = resp["tag_name"].as_str().unwrap_or_default().to_string();
    let version = tag.trim_start_matches('v').to_string();
    if version_cmp(&version, current_version) != std::cmp::Ordering::Greater {
        return Ok(None);
    }
    let assets = resp["assets"].as_array().cloned().unwrap_or_default();
    let (installer, sha256_url) = pick_assets(&assets);
    Ok(Some(ReleaseInfo {
        version,
        notes: resp["body"].as_str().unwrap_or_default().to_string(),
        html_url: resp["html_url"].as_str().unwrap_or_default().to_string(),
        asset_name: installer.as_ref().map(|(n, _, _)| n.clone()),
        asset_url: installer.as_ref().map(|(_, u, _)| u.clone()),
        asset_size: installer.as_ref().map(|(_, _, s)| *s),
        sha256_url,
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
    use super::{parse_sha256_text, pick_assets, version_cmp};
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

    #[test]
    fn picks_installer_and_matching_sha_asset() {
        let assets = serde_json::json!([
            { "name": "latest.json", "browser_download_url": "https://x/latest.json", "size": 1 },
            { "name": "LanMusic_0.4.1_x64-setup.exe", "browser_download_url": "https://x/setup.exe", "size": 123 },
            { "name": "LanMusic_0.4.1_x64-setup.exe.sha256", "browser_download_url": "https://x/setup.exe.sha256", "size": 64 },
        ]);
        let (installer, sha) = pick_assets(assets.as_array().unwrap());
        let (name, url, size) = installer.unwrap();
        assert_eq!(name, "LanMusic_0.4.1_x64-setup.exe");
        assert_eq!(url, "https://x/setup.exe");
        assert_eq!(size, 123);
        assert_eq!(sha.unwrap(), "https://x/setup.exe.sha256");
    }

    #[test]
    fn no_installer_asset_yields_none() {
        let assets = serde_json::json!([
            { "name": "LanMusic_0.4.1_aarch64.dmg", "browser_download_url": "https://x/a.dmg", "size": 9 },
        ]);
        let (installer, sha) = pick_assets(assets.as_array().unwrap());
        assert!(installer.is_none());
        assert!(sha.is_none());
    }
}
