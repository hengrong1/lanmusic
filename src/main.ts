import { createApp, type App as VueApp } from 'vue'
import { createPinia } from 'pinia'
import { getCurrentWindow } from '@tauri-apps/api/window'
import App from './App.vue'
import { applyStoredFont } from './composables/useAppFont'
import { installTooltip } from './directives/tooltip'
import { dragDialog } from './directives/dragDialog'
import { focusTrap } from './directives/focusTrap'
import { i18n } from './i18n'
import { errorText } from './i18n/error'
import { api } from './api/commands'
import { toast } from './composables/useToast'
import { IS_TAURI } from './utils/platform'
import './style.css'

// 应用全局字体（设置 → 外观 → 字体，各窗口共用同一份 localStorage）
applyStoredFont()

/**
 * 屏蔽 WebView2 默认行为，让应用接近原生客户端：
 * - 默认右键菜单：全局拦掉，右键统一交给应用内自定义菜单（各触发处自带
 *   preventDefault，此处兜底；歌词窗/托盘窗无自定义菜单，右键从此无响应）
 * - 浏览器加速键：刷新(F5/Ctrl+R)、打印(Ctrl+P)、另存为(Ctrl+S)、查看源码(Ctrl+U)、
 *   收藏(Ctrl+D)、查找下一个(F3/Ctrl+G)、开新标签/窗口(Ctrl+W/T/N)、缩放(Ctrl+=/-/0)、
 *   前进后退(Alt+←/→)、全屏(F11)、Ctrl+滚轮缩放——全部 preventDefault 抑制
 * - F12 / Ctrl+Shift+I 不拦：debug 构建保留 DevTools 便于调试，release 构建
 *   tauri 未启用 devtools 特性，本来就无法打开
 */
function blockBrowserDefaults() {
  window.addEventListener('contextmenu', (e) => e.preventDefault())
  window.addEventListener(
    'keydown',
    (e) => {
      const k = e.key
      const mod = e.ctrlKey || e.metaKey
      if (
        k === 'F5' ||
        k === 'F11' ||
        k === 'F3' ||
        (mod &&
          ((k === 'r' || k === 'R') || // 刷新
            k === 'p' || // 打印
            k === 's' || // 另存为
            k === 'u' || // 查看源码
            k === 'd' || // 收藏
            k === 'g' || // 查找下一个
            k === 'w' || // 关闭标签
            k === 't' || // 新建标签
            k === 'n' || // 新建窗口
            k === '=' ||
            k === '+' ||
            k === '-' ||
            k === '0')) || // 缩放
        (e.altKey && (k === 'ArrowLeft' || k === 'ArrowRight')) // 历史前进后退
      ) {
        e.preventDefault()
      }
    },
    true,
  )
  // Ctrl+滚轮缩放（passive 必须为 false 才能 preventDefault）
  window.addEventListener(
    'wheel',
    (e) => {
      if (e.ctrlKey) e.preventDefault()
    },
    { passive: false },
  )
}
// 仅桌面 WebView 内生效：纯 vite 浏览器调试保留原生右键/快捷键（检查元素、刷新等）
if (IS_TAURI) blockBrowserDefaults()

/** 挂载完成后移除启动闪屏（index.html 内联，避免首帧白屏） */
function removeSplash() {
  document.getElementById('splash')?.remove()
}

/** 错误转日志的统一文本化：Error 优先带堆栈，其余 String 化 */
function toLogText(e: unknown) {
  return e instanceof Error ? (e.stack ?? String(e)) : String(e)
}

/**
 * 前端错误转发到后端日志文件（release 版 WebView 无控制台，日志文件是唯一排查出口）。
 * 必须 catch 吞掉转发自身的失败——否则 frontendLog 的 rejection 会再次触发
 * unhandledrejection → 再次转发，形成错误风暴。
 */
function logFrontend(level: 'error' | 'warn' | 'info', message: string) {
  api.frontendLog(level, message).catch(() => {})
}

/**
 * 是否首次启动：读 DB 的 `app.onboarded` 标记（引导完成或跳过时写入，见 OnboardingView）。
 * 读取失败按「非首次」处理 —— 引导缺失不应阻断正常启动。
 */
async function isFirstRun(): Promise<boolean> {
  try {
    return (await api.getSetting('app.onboarded')) !== '1'
  } catch {
    return false
  }
}

/** 全局兜底：渲染错误与未捕获的 Promise 拒绝统一 toast（LMERR 信封会被 errorText 解码），
 * 同时原文转发到后端日志（保留堆栈/信封原文，比 toast 文案更利于排查） */
function installErrorGuard(app: VueApp) {
  app.config.errorHandler = (err) => {
    console.error('[app]', err)
    logFrontend('error', `[app] ${toLogText(err)}`)
    toast(errorText(err), 'error')
  }
  window.addEventListener('unhandledrejection', (e) => {
    console.error('[unhandledrejection]', e.reason)
    logFrontend('error', `[unhandledrejection] ${toLogText(e.reason)}`)
    toast(errorText(String(e.reason)), 'error')
  })
}

// 各窗口共用前端 bundle：按窗口 label 区分渲染内容。
// 歌词窗 / 托盘窗与主窗口互斥且不会同时出现，按需动态加载，主窗口 bundle 不背它们的体积
const winLabel = getCurrentWindow().label
async function boot() {
  try {
    if (winLabel === 'lyrics') {
      const { default: Comp } = await import('./components/DesktopLyricsWindow.vue')
      const app = createApp(Comp).use(i18n)
      installTooltip(app)
      app.directive('drag-dialog', dragDialog)
      app.directive('focus-trap', focusTrap)
      installErrorGuard(app)
      app.mount('#app')
      removeSplash()
      // 歌词窗创建时 visible=false（Rust 端防 WebView2 白帧闪现），渲染完成后再显示；
      // 双 rAF 确保首帧已绘制，用户看到的第一眼就是渲染好的透明歌词
      requestAnimationFrame(() =>
        requestAnimationFrame(() => {
          void getCurrentWindow().show()
        }),
      )
      return
    }
    if (winLabel === 'tray') {
      // 托盘是独立 webview：主动应用主题色覆盖（模块加载即写 <html> 变量，读同一份 localStorage）
      await import('./composables/useThemeColor')
      const { default: Comp } = await import('./components/TrayMenuWindow.vue')
      const app = createApp(Comp).use(i18n)
      installTooltip(app)
      app.directive('drag-dialog', dragDialog)
      app.directive('focus-trap', focusTrap)
      installErrorGuard(app)
      app.mount('#app')
      removeSplash()
      return
    }
    // 首次启动渲染引导页（见 views/OnboardingView.vue）；已完成引导则正常进主界面。
    // 引导页不依赖 pinia store，按需动态加载，主界面 bundle 不背它的体积。
    const firstRun = await isFirstRun()
    const Root = firstRun ? (await import('./views/OnboardingView.vue')).default : App
    const app = createApp(Root)
    if (!firstRun) app.use(createPinia())
    app.use(i18n)
    installTooltip(app)
    app.directive('drag-dialog', dragDialog)
    app.directive('focus-trap', focusTrap)
    installErrorGuard(app)
    app.mount('#app')
    removeSplash()
  } catch (e) {
    // 窗口组件加载失败（包损坏/升级中断）：在闪屏上给出明确错误与重试入口，
    // 而不是让闪屏永远停留（白屏假死）
    console.error('[boot]', e)
    logFrontend('error', `[boot] ${toLogText(e)}`)
    const splash = document.getElementById('splash')
    if (splash) {
      splash.innerHTML = ''
      const box = document.createElement('div')
      box.style.cssText = 'text-align:center;color:#f87171;font:13px/1.8 system-ui,sans-serif'
      box.textContent = i18n.global.t('toast.bootFailed')
      const btn = document.createElement('button')
      btn.textContent = i18n.global.t('common.retry')
      btn.style.cssText =
        'margin-top:12px;cursor:pointer;border:none;border-radius:8px;padding:6px 16px;font:13px system-ui,sans-serif;color:#fff;background:var(--color-violet-500)'
      btn.onclick = () => location.reload()
      box.appendChild(document.createElement('br'))
      box.appendChild(btn)
      splash.appendChild(box)
    } else {
      throw e
    }
  }
}
void boot()
