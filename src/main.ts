import { createApp } from 'vue'
import { createPinia } from 'pinia'
import { getCurrentWindow } from '@tauri-apps/api/window'
import App from './App.vue'
import DesktopLyricsWindow from './components/DesktopLyricsWindow.vue'
import TrayMenuWindow from './components/TrayMenuWindow.vue'
import { applyStoredFont } from './composables/useAppFont'
import { installTooltip } from './directives/tooltip'
import { i18n } from './i18n'
import './style.css'

// 应用全局字体（设置 → 外观 → 字体，各窗口共用同一份 localStorage）
applyStoredFont()

/** 挂载完成后移除启动闪屏（index.html 内联，避免首帧白屏） */
function removeSplash() {
  document.getElementById('splash')?.remove()
}

// 各窗口共用前端 bundle：按窗口 label 区分渲染内容
const winLabel = getCurrentWindow().label
if (winLabel === 'lyrics') {
  const app = createApp(DesktopLyricsWindow).use(i18n)
  installTooltip(app)
  app.mount('#app')
  removeSplash()
} else if (winLabel === 'tray') {
  const app = createApp(TrayMenuWindow).use(i18n)
  installTooltip(app)
  app.mount('#app')
  removeSplash()
} else {
  const app = createApp(App).use(createPinia()).use(i18n)
  installTooltip(app)
  app.mount('#app')
  removeSplash()
}
