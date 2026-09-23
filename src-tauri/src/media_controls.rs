//! 系统媒体键集成：把键盘 / 耳机线控上的媒体键（播放暂停 / 上一首 / 下一首 / 停止）
//! 注册为全局快捷键，触发时以 `media-control` 事件广播给前端播放器
//! （见 src/composables/useMediaControls.ts）。
//!
//! 注意与 `now_playing.rs`（SMTC/MPRIS，默认开启）的关系：两者都响应硬件媒体键、
//! 互为替代——全局热键是**常驻注册**（不播放时也会抢走其他应用的媒体键），
//! SMTC 只在自己是当前媒体会话时收键。前端设置面板做互斥：开其一会自动关另一个。
//!
//! 实现说明：与「全局快捷键」共用 tauri-plugin-global-shortcut，但作为一组成员独立的
//! 热键注册——`global_shortcuts.rs` 的 apply/clear 会 `unregister_all()`，因此那两个入口
//! 结束后会回调本模块的 `reapply` 重新注册（是否启用由静态开关记住）。
//! 注册失败（被其他播放器占用等）只记日志、不致命，前端提示部分媒体键不可用。

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// 媒体键加速键名 → 前端动作名
///
/// 加速键名由 `global-hotkey` 的解析表决定（大小写不敏感，见 `hotkey.rs::parse_key`）。
/// 上一首 / 下一首的正确名字是 `MediaTrackPrev` / `MediaTrackNext`——
/// 别写成 `MediaPrevTrack` / `MediaNextTrack`（那种拼法不在表内，解析失败会让注册静默失效）。
const MEDIA_KEYS: [(&str, &str); 4] = [
    ("MediaPlayPause", "playpause"),
    ("MediaTrackNext", "next"),
    ("MediaTrackPrev", "prev"),
    ("MediaStop", "stop"),
];

static ENABLED: AtomicBool = AtomicBool::new(false);

/// 启用 / 禁用媒体键注册（设置开关调用）。
/// 启用时返回**注册失败**的加速键名（如被其他播放器占用），通常为空；禁用时恒为空。
pub fn set_enabled<R: Runtime>(app: &AppHandle<R>, enabled: bool) -> Vec<String> {
    ENABLED.store(enabled, Ordering::SeqCst);
    if enabled {
        register(app)
    } else {
        unregister(app);
        Vec::new()
    }
}

/// 全局快捷键模块 apply/clear 之后调用：若媒体键处于启用态则重新注册（结果仅记日志）
pub fn reapply<R: Runtime>(app: &AppHandle<R>) {
    if ENABLED.load(Ordering::SeqCst) {
        for accel in register(app) {
            log::warn!("系统媒体键重注册失败: {accel}");
        }
    }
}

fn register<R: Runtime>(app: &AppHandle<R>) -> Vec<String> {
    let gs = app.global_shortcut();
    let mut failed = Vec::new();
    for (accel, action) in MEDIA_KEYS {
        // 幂等：已注册则先注销（未注册时 unregister 返回 Err，忽略）
        let _ = gs.unregister(accel);
        let res = gs.on_shortcut(accel, move |app, _sc, event| {
            // 仅按下沿触发一次
            if event.state() == ShortcutState::Pressed {
                log::debug!("系统媒体键触发: {action}");
                // payload 统一对象格式，与 now_playing.rs（SMTC）同一通道同一消费者
                let _ = app.emit("media-control", serde_json::json!({ "action": action }));
            }
        });
        if let Err(e) = res {
            log::warn!("系统媒体键注册失败 {accel}: {e}");
            failed.push(accel.to_string());
        }
    }
    failed
}

fn unregister<R: Runtime>(app: &AppHandle<R>) {
    let gs = app.global_shortcut();
    for (accel, _) in MEDIA_KEYS {
        let _ = gs.unregister(accel);
    }
}
