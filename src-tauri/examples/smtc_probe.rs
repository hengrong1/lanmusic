//! 临时诊断工具：枚举系统当前所有 SMTC 媒体会话。
//!
//! 用途：确认 WebView2 是否为 `<audio>` 注册了「幽灵会话」（空元数据的紧凑卡片，
//! 暂停时顶掉我们 souvlaki 注册的正式会话）。**需在应用正在运行（最好暂停态）时运行**：
//!
//! ```text
//! cd src-tauri && cargo run --example smtc_probe
//! ```
//!
//! 正常应只看到一条 AUMID 指向 LanMusic 的会话；若出现两条（其中一条 TITLE 为空），
//! 即幽灵会话实锤，靠主窗口 `additional_browser_args` 禁 `MediaSessionService` 修。
use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mgr = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.get()?;
    let sessions = mgr.GetSessions()?;
    let count = sessions.Size()?;
    println!("SESSION_COUNT={count}");
    for i in 0..count {
        let s = sessions.GetAt(i)?;
        let aumid = s.SourceAppUserModelId().unwrap_or_default();
        let status = s
            .GetPlaybackInfo()
            .and_then(|p| p.PlaybackStatus())
            .map(|st| format!("{st:?}"))
            .unwrap_or_else(|_| "?".into());
        let (title, artist) = match s.TryGetMediaPropertiesAsync() {
            Ok(op) => match op.get() {
                Ok(p) => (
                    p.Title().unwrap_or_default().to_string(),
                    p.Artist().unwrap_or_default().to_string(),
                ),
                Err(e) => (format!("<props err: {e}>"), String::new()),
            },
            Err(e) => (format!("<props async err: {e}>"), String::new()),
        };
        println!(
            "SESSION[{i}] AUMID={aumid} | TITLE=[{title}] | ARTIST=[{artist}] | STATUS={status}"
        );
    }
    Ok(())
}
