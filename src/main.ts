import { createApp, type App as VueApp } from 'vue'
import { createPinia } from 'pinia'
import { getCurrentWindow } from '@tauri-apps/api/window'
import App from './App.vue'
import { applyStoredFont } from './composables/useAppFont'
import { installTooltip } from './directives/tooltip'
import { dragDialog } from './directives/dragDialog'
import { i18n } from './i18n'
import { errorText } from './i18n/error'
import { toast } from './composables/useToast'
import './style.css'

// 应用全局字体（设置 → 外观 → 字体，各窗口共用同一份 localStorage）
applyStoredFont()

/** 挂载完成后移除启动闪屏（index.html 内联，避免首帧白屏） */
function removeSplash() {
  document.getElementById('splash')?.remove()
}

/** 全局兜底：渲染错误与未捕获的 Promise 拒绝统一 toast（LMERR 信封会被 errorText 解码） */
function installErrorGuard(app: VueApp) {
  app.config.errorHandler = (err) => {
    console.error('[app]', err)
    toast(errorText(err), 'error')
  }
  window.addEventListener('unhandledrejection', (e) => {
    console.error('[unhandledrejection]', e.reason)
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
      installErrorGuard(app)
      app.mount('#app')
      removeSplash()
      return
    }
    const app = createApp(App).use(createPinia()).use(i18n)
    installTooltip(app)
    app.directive('drag-dialog', dragDialog)
    installErrorGuard(app)
    app.mount('#app')
    removeSplash()
  } catch (e) {
    // 窗口组件加载失败（包损坏/升级中断）：在闪屏上给出明确错误与重试入口，
    // 而不是让闪屏永远停留（白屏假死）
    console.error('[boot]', e)
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
