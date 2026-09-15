import type { Directive } from 'vue'

/**
 * 弹窗焦点圈：挂在弹窗面板（带 data-dialog-panel 的元素）上，把 Tab/Shift+Tab
 * 圈定在面板内，阻止焦点窜到弹窗后面的页面元素上。面板挂载时主动接管焦点
 * （焦点先落在面板本身，Tab 一次进入面板内第一个控件）。
 *
 * 用栈记录已激活的面板：多弹窗叠放（如确认框叠在编辑弹窗上）时只有最上层的
 * 圈生效；面板销毁出栈，焦点圈自动交还给下一层。
 */

/** 当前激活（未销毁）的弹窗面板栈，栈顶 = 最上层弹窗 */
const stack: HTMLElement[] = []

const FOCUSABLE_SELECTOR = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(', ')

/** 面板内可聚焦元素（过滤 display:none / 尺寸为零的隐藏项） */
function focusables(panel: HTMLElement): HTMLElement[] {
  return Array.from(panel.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
    (el) => el.offsetWidth > 0 || el.offsetHeight > 0,
  )
}

function onKeydown(e: KeyboardEvent) {
  if (e.key !== 'Tab' || stack.length === 0) return
  const panel = stack[stack.length - 1]
  const items = focusables(panel)
  if (items.length === 0) {
    // 面板里没有可聚焦元素：吃掉 Tab，焦点停在面板上
    e.preventDefault()
    return
  }
  const first = items[0]
  const last = items[items.length - 1]
  const active = document.activeElement as HTMLElement | null
  if (active === panel) {
    // 焦点还停在面板本身（刚打开）：Tab 进第一个控件，Shift+Tab 倒回最后一个
    e.preventDefault()
    ;(e.shiftKey ? last : first).focus()
  } else if (!panel.contains(active)) {
    // 焦点在面板外（弹窗刚打开 / 被外部代码挪走）：拉回圈内
    e.preventDefault()
    ;(e.shiftKey ? last : first).focus()
  } else if (e.shiftKey && active === first) {
    e.preventDefault()
    last.focus()
  } else if (!e.shiftKey && active === last) {
    e.preventDefault()
    first.focus()
  }
}

export const focusTrap: Directive<HTMLElement> = {
  mounted(el: HTMLElement) {
    stack.push(el)
    // 面板自身可聚焦但不进 Tab 序（selector 里排除了 tabindex="-1"）
    el.tabIndex = -1
    document.addEventListener('keydown', onKeydown, true)
    // 等 Transition 把面板渲染出来再接管焦点，避免焦点被过渡打断
    requestAnimationFrame(() => {
      if (stack[stack.length - 1] === el) el.focus({ preventScroll: true })
    })
  },
  unmounted(el: HTMLElement) {
    const idx = stack.indexOf(el)
    if (idx >= 0) stack.splice(idx, 1)
    if (stack.length === 0) document.removeEventListener('keydown', onKeydown, true)
  },
}
