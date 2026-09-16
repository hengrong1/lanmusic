//! 性能与诊断指标：全部为**进程内内存态**（重启清零），供统计页「诊断」标签读取。
//!
//! 原则：打点必须零成本可忽略——原子计数 + Relaxed 序，耗时用墙钟差累计；
//! 不落盘、不做后台采样（内存/CPU 仅在诊断页打开时即时采样一次）。

use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

/// 进程启动时刻（run() 最早处写入）
pub static STARTED: Mutex<Option<Instant>> = Mutex::new(None);
/// Rust setup 阶段耗时（窗口构建前，不含 WebView 首帧渲染）
pub static SETUP_MS: AtomicU64 = AtomicU64::new(0);

// ---- WebDAV ----
pub static WEBDAV_REQUESTS: AtomicU64 = AtomicU64::new(0);
pub static WEBDAV_FAILURES: AtomicU64 = AtomicU64::new(0);
pub static WEBDAV_TOTAL_MS: AtomicU64 = AtomicU64::new(0);

// ---- 封面 ----
pub static COVER_CACHE_HITS: AtomicU64 = AtomicU64::new(0);
pub static COVER_CACHE_MISSES: AtomicU64 = AtomicU64::new(0);
pub static COVER_EXTRACT_OK: AtomicU64 = AtomicU64::new(0);
pub static COVER_EXTRACT_FAIL: AtomicU64 = AtomicU64::new(0);
pub static COVER_TOTAL_MS: AtomicU64 = AtomicU64::new(0);

// ---- 歌词 ----
pub static LYRICS_OK: AtomicU64 = AtomicU64::new(0);
pub static LYRICS_FAIL: AtomicU64 = AtomicU64::new(0);
pub static LYRICS_TOTAL_MS: AtomicU64 = AtomicU64::new(0);

// ---- 错误 ----
pub static PANIC_COUNT: AtomicU64 = AtomicU64::new(0);
pub static FRONTEND_ERRORS: AtomicU64 = AtomicU64::new(0);

// ---- 播放启动延迟（前端 requestPlay → playing 的墙钟差）----
pub static PLAY_LATENCY_COUNT: AtomicU64 = AtomicU64::new(0);
pub static PLAY_LATENCY_TOTAL_MS: AtomicU64 = AtomicU64::new(0);
pub static PLAY_LATENCY_MAX_MS: AtomicU64 = AtomicU64::new(0);

// ---- 最近一次扫描的分段耗时 ----
pub static LAST_SCAN_ENUM_MS: AtomicU64 = AtomicU64::new(0);
pub static LAST_SCAN_PARSE_MS: AtomicU64 = AtomicU64::new(0);

/// WebDAV 请求打点：requests 恒加，失败加 failures，累计耗时
pub fn webdav_done(ok: bool, ms: u64) {
    WEBDAV_REQUESTS.fetch_add(1, Ordering::Relaxed);
    if !ok {
        WEBDAV_FAILURES.fetch_add(1, Ordering::Relaxed);
    }
    WEBDAV_TOTAL_MS.fetch_add(ms, Ordering::Relaxed);
}

/// 播放启动延迟打点（毫秒）
pub fn play_latency(ms: u64) {
    PLAY_LATENCY_COUNT.fetch_add(1, Ordering::Relaxed);
    PLAY_LATENCY_TOTAL_MS.fetch_add(ms, Ordering::Relaxed);
    PLAY_LATENCY_MAX_MS.fetch_max(ms, Ordering::Relaxed);
}

// ---- 进程内存 / CPU 即时采样（sysinfo，诊断页打开时调用）----

static SYS: Mutex<Option<sysinfo::System>> = Mutex::new(None);

/// 返回 (内存 MB, CPU 百分比)。CPU 百分比需要两次采样间隔才有意义：
/// sysinfo 首次采样恒为 0，诊断页轮询第二次起数值有效。
pub fn sample_process() -> (f64, f64) {
    let mut guard = SYS.lock().unwrap();
    let sys = guard.get_or_insert_with(sysinfo::System::new);
    let pid = sysinfo::Pid::from_u32(std::process::id());
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
    sys.process(pid)
        .map(|p| (p.memory() as f64 / 1048576.0, p.cpu_usage() as f64))
        .unwrap_or((0.0, 0.0))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagWebdav {
    pub requests: u64,
    pub failures: u64,
    pub total_ms: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagCovers {
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub extract_ok: u64,
    pub extract_fail: u64,
    pub total_ms: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagLyrics {
    pub ok: u64,
    pub fail: u64,
    pub total_ms: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagPlayLatency {
    pub count: u64,
    pub avg_ms: u64,
    pub max_ms: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsSnapshot {
    pub app_version: String,
    pub tauri_version: String,
    pub os: String,
    pub arch: String,
    pub webview_version: Option<String>,
    pub uptime_seconds: u64,
    pub setup_ms: u64,
    pub memory_mb: f64,
    pub cpu_percent: f64,
    pub webdav: DiagWebdav,
    pub covers: DiagCovers,
    pub lyrics: DiagLyrics,
    pub play_latency: DiagPlayLatency,
    pub panics: u64,
    pub frontend_errors: u64,
    pub last_scan_enum_ms: u64,
    pub last_scan_parse_ms: u64,
}

pub fn snapshot() -> DiagnosticsSnapshot {
    let uptime_seconds = STARTED
        .lock()
        .unwrap()
        .as_ref()
        .map(|t| t.elapsed().as_secs())
        .unwrap_or(0);
    let (memory_mb, cpu_percent) = sample_process();
    DiagnosticsSnapshot {
        app_version: env!("CARGO_PKG_VERSION").into(),
        tauri_version: tauri::VERSION.into(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        webview_version: webview_version(),
        uptime_seconds,
        setup_ms: SETUP_MS.load(Ordering::Relaxed),
        memory_mb,
        cpu_percent,
        webdav: DiagWebdav {
            requests: WEBDAV_REQUESTS.load(Ordering::Relaxed),
            failures: WEBDAV_FAILURES.load(Ordering::Relaxed),
            total_ms: WEBDAV_TOTAL_MS.load(Ordering::Relaxed),
        },
        covers: DiagCovers {
            cache_hits: COVER_CACHE_HITS.load(Ordering::Relaxed),
            cache_misses: COVER_CACHE_MISSES.load(Ordering::Relaxed),
            extract_ok: COVER_EXTRACT_OK.load(Ordering::Relaxed),
            extract_fail: COVER_EXTRACT_FAIL.load(Ordering::Relaxed),
            total_ms: COVER_TOTAL_MS.load(Ordering::Relaxed),
        },
        lyrics: DiagLyrics {
            ok: LYRICS_OK.load(Ordering::Relaxed),
            fail: LYRICS_FAIL.load(Ordering::Relaxed),
            total_ms: LYRICS_TOTAL_MS.load(Ordering::Relaxed),
        },
        play_latency: DiagPlayLatency {
            count: PLAY_LATENCY_COUNT.load(Ordering::Relaxed),
            avg_ms: PLAY_LATENCY_TOTAL_MS.load(Ordering::Relaxed)
                / PLAY_LATENCY_COUNT.load(Ordering::Relaxed).max(1),
            max_ms: PLAY_LATENCY_MAX_MS.load(Ordering::Relaxed),
        },
        panics: PANIC_COUNT.load(Ordering::Relaxed),
        frontend_errors: FRONTEND_ERRORS.load(Ordering::Relaxed),
        last_scan_enum_ms: LAST_SCAN_ENUM_MS.load(Ordering::Relaxed),
        last_scan_parse_ms: LAST_SCAN_PARSE_MS.load(Ordering::Relaxed),
    }
}

/// WebView2 运行时版本（Windows 读 EdgeUpdate 注册表；其他平台返回 None）
#[cfg(windows)]
fn webview_version() -> Option<String> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    let key = winreg::RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}")
        .ok()?;
    key.get_value("pv").ok()
}

#[cfg(not(windows))]
fn webview_version() -> Option<String> {
    None
}
