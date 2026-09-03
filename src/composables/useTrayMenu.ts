/**
 * 系统托盘菜单（主窗口侧）：把当前曲目 / 播放 / 喜欢 / 桌面歌词状态推送给
 * tray 弹窗，并处理弹窗发来的系统级指令（显示主窗口 / 设置 / 退出）。
 * 播放控制（上一首/播放暂停/下一首/喜欢）复用既有 'tray' 事件由 player store 处理。
 */
import { watch } from 'vue'
import { emit, listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { api } from '@/api/commands'
import { usePlayerStore } from '@/stores/player'
import { useDesktopLyrics } from '@/composables/useDesktopLyrics'
import { getAppFont } from '@/composables/useAppFont'
import { useNav } from '@/composables/useNav'

/** 主窗口 → tray 弹窗的同步载荷 */
export interface TraySyncPayload {
  title: string
  artist: string
  albumId: number | null
  playing: boolean
  fav: boolean
  deskLyrics: boolean
  font: string
}

type TrayAction = 'show' | 'lyrics' | 'settings' | 'quit'

let started = false

/** 显示并聚焦主窗口（从托盘菜单打开） */
export function showMainWindow() {
  const appWindow = getCurrentWindow()
  void appWindow.show().catch(() => {})
  void appWindow.unminimize().catch(() => {})
  void appWindow.setFocus().catch(() => {})
}

/** 在 App.vue 调用一次：向 tray 弹窗同步状态 + 监听弹窗指令 */
export function useTrayMenu() {
  const player = usePlayerStore()
  const dl = useDesktopLyrics()
  const nav = useNav()

  if (started) return
  started = true

  const push = () => {
    const t = player.current
    const payload: TraySyncPayload = {
      title: t?.title ?? '',
      artist: t?.artist ?? '',
      albumId: t?.albumId ?? null,
      playing: player.playing,
      fav: t?.fav ?? false,
      deskLyrics: dl.enabled.value,
      font: getAppFont(),
    }
    void emit('tray:sync', payload).catch(() => {})
  }

  // 曲目 / 播放 / 喜欢 / 桌面歌词切换时同步（current 深度 watch，含 fav 字段变化）
  watch(
    [() => player.current, () => player.playing, () => dl.enabled.value],
    () => push(),
    { deep: true },
  )

  // tray 弹窗就绪（窗口刚创建）时立即补推一次
  void listen('tray:ready', () => push())

  // 全局字体变更（设置页）：立即同步给 tray 弹窗
  void listen<string>('font:changed', () => push())

  // 弹窗系统级指令
  void listen<TrayAction>('tray:action', (e) => {
    switch (e.payload) {
      case 'show':
        showMainWindow()
        break
      case 'lyrics':
        dl.toggle()
        break
      case 'settings':
        nav.go({ view: 'settings' })
        showMainWindow()
        break
      case 'quit':
        api.exitApp()
        break
    }
  })
}