/// 平台检测（userAgent 判断，与 Tauri 运行环境一致）
const ua = navigator.userAgent

export const IS_WIN = ua.includes('Windows')
export const IS_MAC = ua.includes('Macintosh')

/// Windows/Linux 无边框窗口需要自绘窗口控制按钮；macOS 用原生红绿灯
export const CUSTOM_WINDOW_CONTROLS = !IS_MAC