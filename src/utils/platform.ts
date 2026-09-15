/// 平台检测（userAgent 判断，与 Tauri 运行环境一致）
const ua = navigator.userAgent

export const IS_WIN = ua.includes('Windows')
export const IS_MAC = ua.includes('Macintosh')

/// Windows/Linux 无边框窗口需要自绘窗口控制按钮；macOS 用原生红绿灯
export const CUSTOM_WINDOW_CONTROLS = !IS_MAC

/// 是否运行在 Tauri WebView 内（纯 vite 浏览器调试时为 false，
/// 用于跳过只应在桌面应用生效的全局行为，如屏蔽右键/浏览器快捷键）
export const IS_TAURI = '__TAURI_INTERNALS__' in window