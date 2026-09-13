import type { Directive } from 'vue'
import { dialogDraggable } from '@/composables/useDialogPrefs'

/**
 * 弹窗拖动指令：挂在弹窗的标题栏元素上，按住拖动其带 `data-dialog-panel` 属性的
 * 定位祖先（弹窗面板）。受「设置 → 外观 → 弹窗可拖动」开关控制；面板上的按钮、
 * 输入框等交互元素不触发拖动。弹窗关闭重开时面板重建，位置自动恢复居中。
 */
export const dragDialog: Directive<HTMLElement> = {
  mounted(el: HTMLElement) {
    el.addEventListener('mousedown', (e: MouseEvent) => {
      if (!dialogDraggable()) return
      const target = e.target as HTMLElement
      if (target.closest('button, input, textarea, select, [data-no-drag]')) return
      const panel = el.closest('[data-dialog-panel]') as HTMLElement | null
      if (!panel) return
      e.preventDefault()

      const rect = panel.getBoundingClientRect()
      const offX = e.clientX - rect.left
      const offY = e.clientY - rect.top
      // 从 flex 居中切换为固定定位（以当前视觉位置为准），拖动期间禁用指针事件穿透
      panel.style.position = 'fixed'
      panel.style.left = `${rect.left}px`
      panel.style.top = `${rect.top}px`
      panel.style.margin = '0'
      panel.style.width = `${rect.width}px`

      const move = (ev: MouseEvent) => {
        // 面板至少保留 80px 在视口内，避免整个拖出屏幕找不回来
        const left = Math.min(Math.max(ev.clientX - offX, 80 - panel.offsetWidth), window.innerWidth - 80)
        const top = Math.min(Math.max(ev.clientY - offY, 0), window.innerHeight - 40)
        panel.style.left = `${left}px`
        panel.style.top = `${top}px`
      }
      const up = () => {
        window.removeEventListener('mousemove', move)
        window.removeEventListener('mouseup', up)
        document.body.style.userSelect = ''
      }
      document.body.style.userSelect = 'none'
      window.addEventListener('mousemove', move)
      window.addEventListener('mouseup', up)
    })
  },
}
