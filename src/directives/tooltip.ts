import type { App, Directive, DirectiveBinding } from 'vue'

/**
 * 全局 tooltip 指令：把悬停提示统一成应用内自绘气泡（替代原生 `title`）。
 *
 * 用法：
 * - `v-tooltip="text"`         —— 光标上方居中（默认）
 * - `v-tooltip:right="text"`   —— 元素右侧垂直居中（收起的侧栏：贴住条目右边缘、对齐光标高度）
 * - `v-tooltip:bottom="text"`  —— 光标下方居中
 * - 文本为空 / null / undefined 时不显示，便于 `v-tooltip="cond ? tip : ''"` 做条件提示
 *
 * 定位锚点是**光标**而不是元素外框：歌曲行、truncate 文本这类元素可能很宽，
 * 按元素居中会让气泡跑到离鼠标很远的地方（原生 title 是跟着光标的，这里对齐它）。
 * `right` 是例外，用元素右边缘 + 光标高度——收起的侧栏需要贴着条目弹出。
 *
 * 为什么用指令而不是包一层组件：项目里绝大多数提示挂在 flex/grid 的子项、`truncate`
 * 文本，或 `BaseButton` 这类单根组件上。包 wrapper 会多出一个盒子、改变布局（flex 项、
 * grid 项、间距都会变），也没法作用到组件的根元素；指令直接挂在元素本身，零布局影响。
 *
 * 维护：全项目只有这一个提示实现；**新增提示一律用 v-tooltip，不要再写 title**。
 * 例外：`EmptyState` / `BaseModal` / `BaseColorPicker` 的 `title` 是组件 prop（标题文案），
 * 不是 tooltip，不要替换。
 */

const DELAY_MS = 450
/** 气泡与光标的间距（跟手，所以留够一点） */
const GAP = 16
/** 气泡与元素边缘的间距（right 模式贴边，用原来的 8px 口径） */
const EDGE_GAP = 8
const EDGE = 4

type Side = 'top' | 'right' | 'bottom'

interface Binding {
  side: Side
  text: () => string
  /** 上次 updated 见到的文案/方向：高频重渲下去重用（见 updated 内注释） */
  lastText: string
  lastSide: Side
  /** 最近一次鼠标位置：延迟触发时用它定位，避免鼠标在元素内移动后气泡落在旧位置 */
  x: number
  y: number
  onEnter: (e: MouseEvent) => void
  onMove: (e: MouseEvent) => void
  onLeave: () => void
}

const bindings = new WeakMap<HTMLElement, Binding>()

let bubble: HTMLDivElement | null = null
let timer: number | null = null
/** 已排定延迟显示的元素（与 shownOn 一起用于「最内层优先」仲裁） */
let pendingOn: HTMLElement | null = null
let shownOn: HTMLElement | null = null
let globalsBound = false

function ensureBubble(): HTMLDivElement {
  if (bubble) return bubble
  const el = document.createElement('div')
  el.setAttribute('role', 'tooltip')
  el.className =
    'pointer-events-none fixed z-[9999] max-w-[320px] break-words rounded-md bg-zinc-800 px-2 py-1 text-xs leading-5 text-zinc-50 shadow-lg dark:bg-zinc-700'
  el.style.display = 'none'
  el.style.opacity = '0'
  el.style.transition = 'opacity .15s ease-out'
  document.body.appendChild(el)
  bubble = el
  return el
}

function hide() {
  if (timer != null) {
    window.clearTimeout(timer)
    timer = null
  }
  pendingOn = null
  shownOn = null
  if (bubble) {
    bubble.style.opacity = '0'
    bubble.style.display = 'none'
  }
}

/** 光标本位定位：首选方向放不下就翻到另一侧，最后统一贴边收拢，保证气泡始终可见 */
function place(el: HTMLElement, side: Side, text: string, cx: number, cy: number) {
  const b = ensureBubble()
  b.textContent = text
  // 先进入布局才能量到尺寸（display:none 时量出来是 0）
  b.style.display = 'block'
  b.style.opacity = '0'
  const br = b.getBoundingClientRect()

  let top: number
  let left: number
  if (side === 'right') {
    // 贴住元素右边缘，垂直对齐光标（收起的侧栏）
    const r = el.getBoundingClientRect()
    left = r.right + EDGE_GAP
    top = cy - br.height / 2
    if (left + br.width > window.innerWidth - EDGE) left = r.left - br.width - EDGE_GAP
  } else {
    left = cx - br.width / 2
    let above = side === 'top'
    const topY = cy - br.height - GAP
    const bottomY = cy + GAP
    if (above && topY < EDGE) above = false
    else if (!above && bottomY + br.height > window.innerHeight - EDGE) above = true
    top = above ? topY : bottomY
  }

  left = Math.min(Math.max(EDGE, left), Math.max(EDGE, window.innerWidth - br.width - EDGE))
  top = Math.min(Math.max(EDGE, top), Math.max(EDGE, window.innerHeight - br.height - EDGE))

  b.style.left = `${Math.round(left)}px`
  b.style.top = `${Math.round(top)}px`
  b.style.opacity = '1'
}

function bindGlobals() {
  if (globalsBound) return
  globalsBound = true
  // 滚动 / 缩放 / 按下时立刻收起：气泡是 fixed 定位、不跟随元素，留着就是错位
  window.addEventListener('scroll', hide, true)
  window.addEventListener('resize', hide)
  window.addEventListener('mousedown', hide, true)
  window.addEventListener('blur', hide)
}

export const tooltip: Directive<HTMLElement, string | null | undefined> = {
  mounted(el, binding: DirectiveBinding<string | null | undefined>) {
    bindGlobals()
    const onEnter = (e: MouseEvent) => {
      const b = bindings.get(el)
      if (!b) return
      const text = b.text()
      if (!text) return
      b.x = e.clientX
      b.y = e.clientY
      // 抢占用：进入更内层的元素时取消外层待显示的气泡（如 BaseColorPicker 里悬停色块），
      // 全局只有一个气泡，谁最后进入谁显示。
      if (timer != null) window.clearTimeout(timer)
      pendingOn = el
      timer = window.setTimeout(() => {
        timer = null
        pendingOn = null
        shownOn = el
        const cur = bindings.get(el)
        if (cur) place(el, cur.side, cur.text(), cur.x, cur.y)
      }, DELAY_MS)
    }
    const onMove = (e: MouseEvent) => {
      const b = bindings.get(el)
      if (!b) return
      // 延迟期间跟着光标更新落点；已经显示后不再移动（与原生 title 一致，避免抖动）
      b.x = e.clientX
      b.y = e.clientY
    }
    const onLeave = () => {
      // 只收起自己名下（待显示或正在显示）的气泡；mouseleave 不会因进入子元素而触发，
      // 所以外层不会误清内层的气泡
      if (shownOn === el || pendingOn === el) hide()
    }
    bindings.set(el, {
      side: (binding.arg as Side) || 'top',
      text: () => (binding.value ?? '').toString().trim(),
      lastText: (binding.value ?? '').toString().trim(),
      lastSide: (binding.arg as Side) || 'top',
      x: 0,
      y: 0,
      onEnter,
      onMove,
      onLeave,
    })
    el.addEventListener('mouseenter', onEnter)
    el.addEventListener('mousemove', onMove)
    el.addEventListener('mouseleave', onLeave)
    el.addEventListener('click', hide)
  },
  updated(el, binding) {
    const b = bindings.get(el)
    if (!b) return
    const side = (binding.arg as Side) || 'top'
    const text = (binding.value ?? '').toString().trim()
    b.side = side
    b.text = () => text
    const changed = text !== b.lastText || side !== b.lastSide
    b.lastText = text
    b.lastSide = side
    // 正在显示且内容真变了才重摆。播放条这类组件随 timeupdate 约每 250ms 重渲一次
    // （逐字歌词时更是 60fps），binding.value 绝大多数时候没变；若每次都 place()，
    // 「置 0 → 强力量尺寸（把 0 提交进样式系统）→ 置 1」会让 opacity 0→1 的淡入
    // 反复重启——气泡一闪一闪，且只在播放时发生（暂停不重渲）。就是「播放时
    // tooltip 闪烁」的根因，文案没变时不碰已显示的气泡。
    if (shownOn === el && changed) {
      if (text) place(el, b.side, text, b.x, b.y)
      else hide()
    }
  },
  unmounted(el) {
    const b = bindings.get(el)
    if (b) {
      el.removeEventListener('mouseenter', b.onEnter)
      el.removeEventListener('mousemove', b.onMove)
      el.removeEventListener('mouseleave', b.onLeave)
      el.removeEventListener('click', hide)
    }
    bindings.delete(el)
    // 正在显示或已排定延迟显示都要收起：后者不清的话，定时器到期会对已脱离 DOM 的
    // 元素弹气泡，且该元素再也不会有 mouseleave 来收起它
    if (shownOn === el || pendingOn === el) hide()
  },
}

/** 主窗口与各独立窗口（歌词 / 托盘）都要注册一次 */
export function installTooltip(app: App) {
  app.directive('tooltip', tooltip)
}
