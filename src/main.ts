import { createApp } from 'vue'
import { createPinia } from 'pinia'
import { getCurrentWindow } from '@tauri-apps/api/window'
import App from './App.vue'
import DesktopLyricsWindow from './components/DesktopLyricsWindow.vue'
import TrayMenuWindow from './components/TrayMenuWindow.vue'
import VideoPlayer from './views/VideoPlayer.vue'
import { applyStoredFont } from './composables/useAppFont'
import { i18n } from './i18n'
import './style.css'

// 应用全局字体（设置 → 外观 → 字体，各窗口共用同一份 localStorage）
applyStoredFont()

// 各窗口共用前端 bundle：按窗口 label 区分渲染内容
const winLabel = getCurrentWindow().label
if (winLabel === 'lyrics') {
  createApp(DesktopLyricsWindow).use(i18n).mount('#app')
} else if (winLabel === 'tray') {
  createApp(TrayMenuWindow).use(i18n).mount('#app')
} else if (winLabel.startsWith('mv-')) {
  createApp(VideoPlayer).mount('#app')
} else {
  createApp(App).use(createPinia()).use(i18n).mount('#app')
}
