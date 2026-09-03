import { createApp } from 'vue'
import { createPinia } from 'pinia'
import { getCurrentWindow } from '@tauri-apps/api/window'
import App from './App.vue'
import DesktopLyricsWindow from './components/DesktopLyricsWindow.vue'
import { applyStoredFont } from './composables/useAppFont'
import './style.css'

// 应用全局字体（设置 → 外观 → 字体，两个窗口共用同一份 localStorage）
applyStoredFont()

// 桌面歌词浮窗与主窗口共用前端资源：按窗口 label 区分渲染内容
if (getCurrentWindow().label === 'lyrics') {
  createApp(DesktopLyricsWindow).mount('#app')
} else {
  createApp(App).use(createPinia()).mount('#app')
}
