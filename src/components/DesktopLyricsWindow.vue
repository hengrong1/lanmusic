<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { emit, listen } from '@tauri-apps/api/event'
import { SkipPreviousIcon as SkipBack } from '@solar-icons/vue/bold/skip-previous'
import { SkipNextIcon as SkipForward } from '@solar-icons/vue/bold/skip-next'
import { PauseIcon as Pause } from '@solar-icons/vue/bold/pause'
import { PlayIcon as Play } from '@solar-icons/vue/bold/play'
import { Rewind5SecondsBackIcon as RewindBack } from '@solar-icons/vue/linear/rewind-5-seconds-back'
import { Rewind5SecondsForwardIcon as RewindForward } from '@solar-icons/vue/linear/rewind-5-seconds-forward'
import { RestartIcon as RotateCcw } from '@solar-icons/vue/linear/restart'
import { HomeAngle2Icon as Home } from '@solar-icons/vue/linear/home-angle-2'
import { CloseIcon as X } from '@solar-icons/vue/linear/close'
import { api } from '@/api/commands'
import { EMPTY_LYRIC, type DeskControl, type DeskLyricsConfig } from '@/composables/useDesktopLyrics'
import type { QrcWord } from '@/types'
import { hexToRgba } from '@/utils/color'

// 桌面歌词浮窗：接收主窗口推送的歌词行与配置进行渲染；
// 整窗透明，按住文字区域可拖动（data-tauri-drag-region）。
// 鼠标悬停时在歌词上方浮现控制条（半透明背景）：
// 上一首 / 播放暂停 / 下一首 · 歌词校准（后退/还原/前进） · 关闭，
// 指令通过 lyrics:control 事件发回主窗口由播放器执行。
// 逐字歌词：主窗口推送当前行的词级时间轴（words）与卡拉OK时钟锚点（anchor），
// 本地 rAF 按 posMs + (now - at) × rate 插值出歌词轴位置，做双色渐变填充。

const lines = ref<string[]>([])
/** 各行译文（与 lines 下标一一对应；空串 = 该行无翻译或纯文本歌词） */
const translations = ref<string[]>([])
/** 当前播放行所在位置：0=第一行，1=第二行（双行交替滚动） */
const active = ref<0 | 1>(0)
/** 当前行的逐字时间轴；空数组 = 行级歌词（整行纯文本） */
const words = ref<QrcWord[]>([])
/** 卡拉OK时钟锚点：推送时刻的歌词轴位置 / 速率 / 是否走秒 */
const anchor = ref({ posMs: 0, rate: 1, running: false, at: Date.now() })
const config = ref<DeskLyricsConfig>({
  lines: 2,
  align: 'center',
  color: '#ffffff',
  pendingColor: '#a1a1aa',
  fontSize: 34,
  bgColor: '#000000',
  bgOpacity: 0.35,
  outline: true,
  outlineColor: '#000000',
  bold: true,
  showTranslation: true,
})
const playing = ref(false)

let unlisten: (() => void) | undefined
/** 最近一次收到的换句计数：变化 = 换句（或同文本下一句的逐字重推），滚动位移归零 */
let lastSeq = 0
onMounted(async () => {
  // 兜底透明背景（避免主题样式给浮窗加上底色）
  document.documentElement.style.background = 'transparent'
  document.body.style.background = 'transparent'
  unlisten = await listen<{
    lines: string[]
    translations?: string[]
    active?: 0 | 1
    config: DeskLyricsConfig
    playing?: boolean
    font?: string
    words?: QrcWord[] | null
    anchor?: { posMs: number; rate: number; running: boolean; at: number }
    lineTime?: { start: number; end: number } | null
    seq?: number
  }>('lyrics:sync', (e) => {
    if (Array.isArray(e.payload?.lines)) lines.value = e.payload.lines
    if (Array.isArray(e.payload?.translations)) translations.value = e.payload.translations
    if (e.payload?.active === 0 || e.payload?.active === 1) active.value = e.payload.active
    if (e.payload?.config) config.value = { ...config.value, ...e.payload.config }
    if (typeof e.payload?.playing === 'boolean') playing.value = e.payload.playing
    // 全局字体（设置页修改即时联动；空串 = 恢复默认字体栈）；字宽变了需重测超宽行
    if (typeof e.payload?.font === 'string') {
      document.body.style.fontFamily = e.payload.font
      void nextTick(measureScroll)
    }
    // 逐字数据与时钟锚点：仅在有 payload 字段时覆盖（锚点可单独推送）
    if (Array.isArray(e.payload?.words)) words.value = e.payload.words
    else if (e.payload?.words === null) words.value = []
    if (e.payload?.anchor) anchor.value = e.payload.anchor
    // 当前行时间区间（歌词轴 ms）：行级歌词按播放进度推进滚动
    lineTime.value = e.payload?.lineTime ?? null
    // seq 变化 = 换句：各行位移归零重新跟随；暂停/倍速/seek/字体等原句重推不 bump
    // seq，滚动位置保持连续（seq 先于本轮 rows 重渲染读取，此处拿到的是新行数）
    if (typeof e.payload?.seq === 'number' && e.payload.seq !== lastSeq) {
      lastSeq = e.payload.seq
      shifts.value = rows.value.map(() => 0)
    }
  })
  // 通知主窗口：浮窗已就绪，请求推送当前歌词与配置
  void emit('lyrics:ready')
  // 网络字体就绪后文本实宽可能变化，补测一次超宽行
  void document.fonts.ready.then(() => nextTick(measureScroll))
})
onBeforeUnmount(() => unlisten?.())

function control(action: DeskControl) {
  void emit('lyrics:control', action)
}

const rootStyle = computed(() => ({
  alignItems:
    config.value.align === 'left'
      ? 'flex-start'
      : config.value.align === 'right'
        ? 'flex-end'
        : config.value.align === 'split'
          ? 'stretch' // 左右分离：两行各占满行宽，由各行自己的 text-align 控制对齐
          : 'center',
  // 背景默认隐藏，鼠标悬停浮窗时才显示（见样式表 .dl-root:hover）；
  // 这里只提供悬停时要用的颜色，不透明度 0 = 悬停也无背景。
  '--dl-bg':
    config.value.bgOpacity > 0 ? hexToRgba(config.value.bgColor, config.value.bgOpacity) : 'transparent',
}))
/** 描边阴影串：多层同色阴影模拟描边；关闭时无阴影 */
const textShadow = computed(() => {
  if (!config.value.outline) return 'none'
  const c = config.value.outlineColor
  return [
    `0 0 3px ${hexToRgba(c, 0.9)}`,
    `0 1px 3px ${hexToRgba(c, 0.85)}`,
    `0 0 10px ${hexToRgba(c, 0.5)}`,
    `0 0 22px ${hexToRgba(c, 0.35)}`,
  ].join(', ')
})
/**
 * 按行位置生成样式：分离模式下对齐固定跟随行位置（第一行永远左对齐在左上、
 * 第二行永远右对齐在右下），播放状态只决定颜色（播放行/未播放行两色）。
 */
const rowStyle = (row: 0 | 1) => {
  const textAlign =
    config.value.align === 'split'
      ? row === 0
        ? 'left'
        : 'right'
      : config.value.align
  return {
    color: active.value === row ? config.value.color : config.value.pendingColor,
    fontSize: `${config.value.fontSize}px`,
    fontWeight: config.value.bold ? 700 : 500,
    textShadow: textShadow.value,
    textAlign,
  }
}
/**
 * 逐字行的描边：渐变填充走 background-clip:text（透明文字），渐变属于背景层，
 * 画在一切文字绘制之前——行内 -webkit-text-stroke 无论 paint-order 如何都盖在
 * 渐变上把文字变成描边色（2026-09-29 实录），text-shadow 也会透过透明 fill 叠
 * 在渐变上压暗颜色。因此词行一律不吃阴影/描边，描边改由 .dl-stroke::before
 * 垫底伪元素绘制（见样式表）：行内只提供描边宽度与颜色两个 CSS 变量。
 */
const strokeOutline = computed(() => {
  if (!config.value.outline) return { textShadow: 'none' }
  return {
    textShadow: 'none',
    '--dl-stroke-w': `${Math.max(2, config.value.fontSize * 0.08)}px`,
    '--dl-stroke-c': config.value.outlineColor,
  }
})
/** 单词样式：按播放进度双色渐变（已唱=播放行颜色，未唱=未播放行颜色） */
const wordStyle = (w: QrcWord) => {
  const dur = Math.max(1, w.endTime - w.startTime)
  const p = Math.min(1, Math.max(0, (karaokeMs.value - w.startTime) / dur))
  if (p >= 1) return { color: config.value.color }
  if (p <= 0) return { color: config.value.pendingColor }
  return {
    background: `linear-gradient(90deg, ${config.value.color} ${p * 100}%, ${config.value.pendingColor} ${p * 100}%)`,
    WebkitBackgroundClip: 'text',
    backgroundClip: 'text',
    color: 'transparent',
  }
}
/** 控制条背景：与面板同色系，略微加深以便在面板上浮起；无面板背景时用默认深色 */
const controlsStyle = computed(() => ({
  background:
    config.value.bgOpacity > 0
      ? hexToRgba(config.value.bgColor, Math.min(0.9, config.value.bgOpacity + 0.2))
      : 'rgba(24, 24, 27, 0.55)',
}))
/** 渲染条目：歌词行或翻译行（翻译行无逐字数据，样式由 style 全量携带）；
 * 值允许 undefined（strokeOutline 未命中分支的透传，Vue 行内样式忽略 undefined）。
 * live：该行是否为正在播放的句子（超宽时跟随高亮/播放进度滚动）；
 * 缺省 = 静止（下一句预告 / 译文副行，超宽只裁剪不滚） */
type DeskRow = {
  text: string
  words?: QrcWord[]
  live?: boolean
  style: Record<string, string | number | undefined>
}
/** 渲染行：勾「译」且当前句带译文时固定「第一行歌词、第二行该句翻译」——译文必须
 * 紧跟原文，不随 active 交替换位；当前句无译文时不走此分支，落到下方双行交替
 * 逻辑（lines 已由主窗口按 active 交换位置）：单语歌词勾「译」若仍把当前句钉在
 * 第一行，会失去交替滚动的节奏（每次都是上面播放行）也看不到下一句预告。
 * 逐字行（words 非空）额外携带词级时间轴与描边覆盖，模板里按字渲染渐变 */
const rows = computed<DeskRow[]>(() => {
  const wordsFor = (row: 0 | 1) => (active.value === row && words.value.length > 1 ? words.value : undefined)
  // 翻译模式：仅当前句带译文时接管渲染——固定「歌词(播放) + 译文」，颜色手动指定，
  // 不能复用 rowStyle 的 active 换色逻辑（active 交替会让固定行位置的颜色抖动）；
  // 该句无译文时不 return，落到下方双行交替逻辑（与关闭翻译同一条路径）：
  // 播放行按奇偶在两行间交替，另一行显示下一句预告，保住滚动节奏与信息量
  if (config.value.showTranslation && translations.value[active.value]) {
    const w = words.value.length > 1 ? words.value : undefined
    const alignOf = (row: 0 | 1) =>
      config.value.align === 'split' ? (row === 0 ? 'left' : 'right') : config.value.align
    return [
      {
        text: lines.value[active.value] || EMPTY_LYRIC,
        words: w,
        live: true,
        style: {
          color: config.value.color,
          fontSize: `${config.value.fontSize}px`,
          fontWeight: config.value.bold ? 700 : 500,
          textAlign: alignOf(0),
          ...(w ? strokeOutline.value : { textShadow: textShadow.value }),
        },
      },
      {
        text: translations.value[active.value],
        style: {
          color: config.value.color,
          opacity: 0.72,
          fontSize: `${config.value.fontSize}px`,
          fontWeight: 500,
          textShadow: textShadow.value,
          textAlign: alignOf(1),
        },
      },
    ]
  }
  if (config.value.lines === 1) {
    // 单行只有播放行（无论交替到哪个位置），始终用播放行颜色并携带逐字数据
    const w = words.value.length > 1 ? words.value : undefined
    return [
      {
        text: lines.value[active.value] || EMPTY_LYRIC,
        words: w,
        live: true,
        style: { ...rowStyle(0), color: config.value.color, ...(w ? strokeOutline.value : {}) },
      },
    ]
  }
  return [
    {
      text: lines.value[0] || (active.value === 0 ? EMPTY_LYRIC : '\u00A0'),
      words: wordsFor(0),
      live: active.value === 0,
      style: wordsFor(0) ? { ...rowStyle(0), ...strokeOutline.value } : rowStyle(0),
    },
    {
      text: lines.value[1] || '\u00A0',
      words: wordsFor(1),
      live: active.value === 1,
      style: wordsFor(1) ? { ...rowStyle(1), ...strokeOutline.value } : rowStyle(1),
    },
  ]
})

/** 各行溢出量：null = 未超宽不滚动；数字 = 需左移的像素（内容实宽 - 行容器宽） */
const scroll = ref<(number | null)[]>([])
/** 各行当前位移（px）：rAF 平滑逼近目标，仅播放行会被推进（预告行/译文行静止裁剪） */
const shifts = ref<number[]>([])
/** 逐字行各词的词尾 x（相对内容起点 px）：按 karaokeMs 插值出高亮位置供滚动跟随 */
const wordXs = ref<number[]>([])
/** 当前行时间区间（歌词轴 ms，主窗口推送）：行级歌词按播放进度推进滚动 */
const lineTime = ref<{ start: number; end: number } | null>(null)
const lineEls = ref<Array<HTMLParagraphElement | null>>([])
const setLineEl = (i: number) => (el: unknown) => {
  lineEls.value[i] = (el as HTMLParagraphElement | null) ?? null
}
/** 测量布局：各行溢出量 + 逐字行词尾 x。歌词/配置变化重算 rows 后、字体切换与
 * 字体就绪时调用；载体 .dl-inner 是行内唯一元素子节点，实宽用 getBoundingClientRect（含小数） */
function measureScroll() {
  const next: (number | null)[] = []
  for (let i = 0; i < rows.value.length; i++) {
    const p = lineEls.value[i]
    const inner = p?.firstElementChild as HTMLElement | null
    const overflow = p && inner ? inner.getBoundingClientRect().width - p.clientWidth : 0
    next.push(overflow > 2 ? overflow : null)
  }
  const cur = scroll.value
  if (next.length !== cur.length || next.some((v, i) => v !== cur[i])) scroll.value = next
  if (shifts.value.length !== rows.value.length) shifts.value = rows.value.map(() => 0)
  // 词尾 x：逐字行（rows 中至多一行携带 words）各词 span 的右缘，offsetParent 即 .dl-inner
  const wi = rows.value.findIndex((r) => r.words)
  const inner = wi >= 0 ? ((lineEls.value[wi]?.firstElementChild as HTMLElement | null) ?? null) : null
  wordXs.value = inner
    ? Array.from(inner.children, (c) => (c as HTMLElement).offsetLeft + (c as HTMLElement).offsetWidth)
    : []
}
// 歌词/对齐/字号/加粗等变化都会重算 rows，统一在其后重测；字体切换与字体就绪另行补测
watch(rows, () => void nextTick(measureScroll), { immediate: true })

/** 卡拉OK当前歌词轴位置（毫秒）：按锚点 + 本地时钟插值，避免与主窗口逐帧通信 */
const karaokeMs = ref(0)
/** 逐字高亮当前位置（px，相对内容起点）：按 karaokeMs 在词时间轴内插值；
 * 词间空隙停在上一词词尾，句前返回 0，句后停在末词词尾 */
function karaokeX(): number {
  const ws = words.value
  const xs = wordXs.value
  if (!ws.length || xs.length !== ws.length) return 0
  const t = karaokeMs.value
  if (t <= ws[0].startTime) return 0
  for (let i = 0; i < ws.length; i++) {
    if (t < ws[i].endTime) {
      const startX = i ? xs[i - 1] : 0
      const p = Math.min(1, Math.max(0, (t - ws[i].startTime) / Math.max(1, ws[i].endTime - ws[i].startTime)))
      return startX + (xs[i] - startX) * p
    }
  }
  return xs[xs.length - 1]
}
/** 推进各行位移目标：逐字行把高亮位置钉在视口约 70% 处（句首/句尾夹在 [0, 溢出]），
 * 行级行按播放进度线性推进到句末；非播放行恒为 0（静止）。
 * 位移以指数平滑（τ≈120ms）逼近目标，跳字/seek 不突变；已收敛（<0.5px）不写入，
 * 避免无效重渲染 */
let lastFrame = 0
function advanceShifts() {
  const now = performance.now()
  const k = 1 - Math.exp(-(now - lastFrame) / 120)
  lastFrame = now
  for (let i = 0; i < rows.value.length; i++) {
    const ov = scroll.value[i]
    const p = lineEls.value[i]
    if (ov == null || !p) continue
    const row = rows.value[i]
    let target = 0
    if (row.live) {
      if (row.words && wordXs.value.length) {
        target = Math.min(Math.max(karaokeX() - p.clientWidth * 0.7, 0), ov)
      } else if (lineTime.value) {
        const { start, end } = lineTime.value
        const progress = Math.min(1, Math.max(0, (karaokeMs.value - start) / Math.max(1, end - start)))
        target = ov * progress
      }
    }
    const cur = shifts.value[i] ?? 0
    const next = Math.abs(target - cur) < 0.5 ? target : cur + (target - cur) * k
    if (next !== cur) shifts.value[i] = next
  }
}
/** 是否需要逐帧时钟：逐字渐变（有词）或超宽播放行的滚动跟随（逐字/有行时间轴） */
const animating = computed(
  () =>
    words.value.length > 0 ||
    rows.value.some((r, i) => r.live && scroll.value[i] != null && (!!r.words || !!lineTime.value)),
)
let karaokeRaf = 0
watch(
  animating,
  (on) => {
    cancelAnimationFrame(karaokeRaf)
    if (!on) return
    lastFrame = performance.now()
    const tick = () => {
      const a = anchor.value
      karaokeMs.value = a.running ? a.posMs + (Date.now() - a.at) * a.rate : a.posMs
      advanceShifts()
      karaokeRaf = requestAnimationFrame(tick)
    }
    karaokeRaf = requestAnimationFrame(tick)
  },
  { immediate: true },
)
onBeforeUnmount(() => cancelAnimationFrame(karaokeRaf))

/** 滚动行内联位移：超宽行按 shifts 平移（非播放行恒 0，等效静止） */
const innerStyle = (i: number) => {
  if (scroll.value[i] == null) return undefined
  return { transform: `translateX(${-(shifts.value[i] ?? 0)}px)` }
}
</script>

<template>
  <div class="dl-root" data-tauri-drag-region :style="rootStyle">
    <!-- 控制条：悬停浮现，背景与面板同色系（略微加深） -->
    <div class="dl-controls" :style="controlsStyle">
      <button class="dl-btn" v-tooltip="$t('player.prev')" @click="control('prev')"><SkipBack class="h-4 w-4" /></button>
      <button class="dl-btn" v-tooltip="playing ? $t('player.pause') : $t('player.play')" @click="control('toggle')">
        <Pause v-if="playing" class="h-4.5 w-4.5" />
        <Play v-else class="h-4.5 w-4.5" />
      </button>
      <button class="dl-btn" v-tooltip="$t('player.next')" @click="control('next')"><SkipForward class="h-4 w-4" /></button>
      <span class="dl-divider"></span>
      <button
        class="dl-btn"
        v-tooltip="$t('desktopLyrics.backHint')"
        @click="control('calib-back')"
      >
        <RewindBack class="h-4 w-4" />
      </button>
      <button class="dl-btn" v-tooltip="$t('player.lyricResetHint')" @click="control('calib-reset')">
        <RotateCcw class="h-3.5 w-3.5" />
      </button>
      <button
        class="dl-btn"
        v-tooltip="$t('desktopLyrics.forwardHint')"
        @click="control('calib-forward')"
      >
        <RewindForward class="h-4 w-4" />
      </button>
      <span class="dl-divider"></span>
      <!-- 显示翻译：容器与其他控制条按钮统一（dl-btn 圆形），内容为文字「译」；
           开启时常亮背景区分状态，指令发回主窗口改 config（自动持久化并回推） -->
      <button
        class="dl-btn text-[13px] font-medium"
        :class="{ 'dl-btn-active': config.showTranslation }"
        v-tooltip="config.showTranslation ? $t('lyrics.translationHide') : $t('lyrics.translationShow')"
        @click="control('toggle-translation')"
      >
        译
      </button>
      <span class="dl-divider"></span>
      <!-- 显示主界面：唤起/前置主窗口（隐藏或最小化时还原），走 Rust 侧
           show_main_window（复用托盘左键逻辑，顺带收起托盘菜单弹窗） -->
      <button class="dl-btn" v-tooltip="$t('desktopLyrics.showMainHint')" @click="api.showMainWindow()">
        <Home class="h-4 w-4" />
      </button>
      <button class="dl-btn dl-close" v-tooltip="$t('tray.disableDesktopLyrics')" @click="control('close')">
        <X class="h-4 w-4" />
      </button>
    </div>
    <!-- 歌词（背景铺满整个面板）：播放行主样式，另一行次样式，双行交替滚动；
         超宽行不再 ellipsis 截断：播放行由 .dl-inner 载体跟随滚动（逐字行跟高亮、
         行级行跟播放进度；预告/译文行静止裁剪），p 只负责裁剪，超宽时强制左对齐
         让内容从行首露出；逐字行按词渲染双色渐变（空白用 whitespace-pre 保留），
         否则整行纯文本 -->
    <p
      v-for="(row, i) in rows"
      :key="i"
      :ref="setLineEl(i)"
      class="dl-line"
      data-tauri-drag-region
      :style="scroll[i] != null ? { ...row.style, textAlign: 'left' } : row.style"
    >
      <span
        :key="row.text"
        class="dl-inner"
        :class="{ 'dl-stroke': !!(row.words && config.outline) }"
        :data-text="row.text"
        data-tauri-drag-region
        :style="innerStyle(i)"
      >
        <template v-if="row.words">
          <span v-for="(w, wi) in row.words" :key="wi" class="dl-word" :style="wordStyle(w)">{{ w.word }}</span>
        </template>
        <template v-else>{{ row.text }}</template>
      </span>
    </p>
  </div>
</template>

<style>
/* 仅桌面歌词浮窗挂载该组件；全局样式但选择器不会命中主窗口元素 */
.dl-root {
  position: fixed;
  inset: 0;
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 4px;
  padding: 8px 32px;
  cursor: move;
  user-select: none;
  overflow: hidden;
  /* 背景默认隐藏：整窗透明，只有歌词文字（靠描边保证可读）浮在桌面上；
     鼠标悬停浮窗时才淡入设置的背景色（--dl-bg 由根元素行内样式提供，
     背景不透明度为 0 时悬停也不显示背景）。 */
  background: transparent;
  transition: background 0.2s ease;
}
.dl-root:hover {
  background: var(--dl-bg, transparent);
}
/* 控制条：默认隐藏，悬停窗口时浮现 */
.dl-controls {
  position: absolute;
  top: 6px;
  left: 50%;
  transform: translateX(-50%);
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 10px;
  border-radius: 9999px;
  backdrop-filter: blur(10px);
  box-shadow: 0 4px 14px rgba(0, 0, 0, 0.3);
  opacity: 0;
  transition: opacity 0.18s ease;
}
.dl-root:hover .dl-controls {
  opacity: 1;
}
.dl-btn {
  display: flex;
  cursor: pointer;
  align-items: center;
  justify-content: center;
  border: 0;
  border-radius: 9999px;
  width: 28px;
  height: 28px;
  padding: 0;
  color: #fff;
  background: transparent;
  transition: background 0.15s ease;
}
.dl-btn:hover {
  background: rgba(255, 255, 255, 0.16);
}
/* 切换类按钮的开启态：常亮（略高于 hover 亮度），提示当前状态 */
.dl-btn-active,
.dl-btn-active:hover {
  background: rgba(255, 255, 255, 0.28);
}
/* 歌词背景容器：宽度随文字自适应（对齐由父级 align-items 控制） */
.dl-divider {
  width: 1px;
  height: 16px;
  margin: 0 2px;
  background: rgba(255, 255, 255, 0.22);
}
.dl-close:hover {
  background: rgba(239, 68, 68, 0.45);
}
.dl-line {
  margin: 0;
  max-width: 100%;
  font-weight: 700;
  line-height: 1.25;
  white-space: nowrap;
  overflow: hidden;
}
/* 滚动载体：inline-block 取内容实宽供测量，relative 供描边伪元素定位；位移由
   rAF 写在 transform 上（逐字行跟随高亮、行级行跟随播放进度），描边/渐变随之
   同步移动，p 保持固定不动只负责裁剪 */
.dl-inner {
  position: relative;
  display: inline-block;
}
/* 逐字单词：保留词内/词间空白（连续空格词不被折叠） */
.dl-word {
  white-space: pre;
}
/* 逐字行描边：渐变填充是背景层（画在一切文字绘制之前），行内 -webkit-text-stroke
   无论 paint-order 如何都画在渐变上把文字变成描边色（2026-09-29 实录）⇒ 描边改由
   垫底伪元素画：attr(data-text) 复刻整行文字（white-space: pre 与词行空格语义
   一致，保证逐像素对齐）、stroke 宽度取 2×w（内半环被上层渐变盖住，外露 w）、
   fill 透明只留描边环，z:-1 垫在词渐变之下、悬停背景之上；
   伪元素挂在 .dl-inner 上与词渐变同层位移，跑马灯滚动时保持逐像素对齐 */
.dl-inner.dl-stroke::before {
  content: attr(data-text);
  position: absolute;
  inset: 0;
  z-index: -1;
  color: transparent;
  white-space: pre;
  -webkit-text-stroke: calc(var(--dl-stroke-w, 2px) * 2) var(--dl-stroke-c, #000);
  pointer-events: none;
}
</style>