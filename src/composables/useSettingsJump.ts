/**
 * 跨窗口跳转设置页：桌面歌词浮窗的「设置」按钮经 'app:open-settings' 事件通知
 * 主窗口——导航到设置页对应分区并前置主窗口（与托盘 settings 指令同链路）。
 * 同窗口内的跳转（搜索框齿轮 / 播放页均衡器钮）直接 nav.go，不走事件。
 */
import { listen } from '@tauri-apps/api/event'
import { useNav } from '@/composables/useNav'
import { showMainWindow } from '@/composables/useTrayMenu'
import type { SettingsSectionId } from '@/types'

/** 在 App.vue 调用一次（仅主窗口） */
export function useSettingsJump() {
  const nav = useNav()
  void listen<SettingsSectionId | undefined>('app:open-settings', (e) => {
    nav.go({ view: 'settings', section: e.payload })
    showMainWindow()
  })
}
