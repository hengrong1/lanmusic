//! 全局快捷键（tauri-plugin-global-shortcut）：
//! 系统级热键的注册 / 注销 / 检测，触发时以 `global-shortcut` 事件把动作名广播给前端。
//!
//! 配置与开关由前端管理（设置 → 通用 → 快捷键，见 src/composables/useShortcuts.ts）：
//! - 前端启动时若已启用则调用 `global_shortcut_apply` 恢复注册；
//! - 触发事件统一为 `global-shortcut`，payload 为动作名（toggle / next / prev）。

use tauri::{AppHandle, Emitter, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// 整体替换全局快捷键注册：先注销本应用已注册的全部热键，再逐一注册新绑定。
///
/// 任一注册失败（常见原因：组合已被其他程序占用）时回滚全部注册并返回错误，
/// 保证「要么全部生效、要么全部不生效」，避免残留半生效状态。
pub fn apply<R: Runtime>(
    app: &AppHandle<R>,
    bindings: Vec<(String, String)>,
) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    for (accel, action) in &bindings {
        let action = action.clone();
        let action_name = action.clone(); // 闭包 move 后日志仍要用，先留一份
        let accel_owned = accel.clone();
        let result = gs.on_shortcut(accel.as_str(), move |app, _shortcut, event| {
            // 仅按下沿触发一次（松开不重复广播）
            if event.state() == ShortcutState::Pressed {
                log::debug!("全局快捷键触发: {action}");
                let _ = app.emit("global-shortcut", action.clone());
            }
        });
        if let Err(e) = result {
            // 回滚：注销本次已注册成功的部分，使状态与「关闭」等价
            let _ = gs.unregister_all();
            log::warn!("全局快捷键注册失败 {accel_owned}: {e}");
            return Err(format!("{accel_owned}: {e}"));
        }
        log::info!("全局快捷键已注册: {accel_owned} -> {action_name}");
    }
    // unregister_all 会一并清掉系统媒体键：此处重新注册（若媒体键开关为启用态）
    crate::media_controls::reapply(app);
    Ok(())
}

/// 注销本应用注册的全部全局快捷键（关闭全局快捷键开关时调用）
pub fn clear<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| e.to_string())?;
    // 同上：媒体键开关若为启用态，注销后需恢复
    crate::media_controls::reapply(app);
    Ok(())
}

/// 检测快捷键当前是否已被本应用注册（true = 已注册）。
/// 只能检测本应用的注册状态；被其他程序占用只能在注册时感知（apply 返回错误）。
pub fn is_registered<R: Runtime>(app: &AppHandle<R>, shortcut: &str) -> Result<bool, String> {
    let sc = shortcut
        .parse::<tauri_plugin_global_shortcut::Shortcut>()
        .map_err(|e| format!("{shortcut}: {e}"))?;
    Ok(app.global_shortcut().is_registered(sc))
}
