<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { emit, listen } from '@tauri-apps/api/event'
import { SkipPreviousIcon as SkipBack } from '@solar-icons/vue/bold/skip-previous'
import { SkipNextIcon as SkipForward } from '@solar-icons/vue/bold/skip-next'
import { PauseIcon as Pause } from '@solar-icons/vue/bold/pause'
import { PlayIcon as Play } from '@solar-icons/vue/bold/play'
import { Rewind5SecondsBackIcon as RewindBack } from '@solar-icons/vue/linear/rewind-5-seconds-back'
import { Rewind5SecondsForwardIcon as RewindForward } from '@solar-icons/vue/linear/rewind-5-seconds-forward'
import { RestartIcon as RotateCcw } from '@solar-icons/vue/linear/restart'
import { CloseIcon as X } from '@solar-icons/vue/linear/close'
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
  }>('lyrics:sync', (e) => {
    if (Array.isArray(e.payload?.lines)) lines.value = e.payload.lines
    if (Array.isArray(e.payload?.translations)) translations.value = e.payload.translations
    if (e.payload?.active === 0 || e.payload?.active === 1) active.value = e.payload.active
    if (e.payload?.config) config.value = { ...config.value, ...e.payload.config }
    if (typeof e.payload?.playing === 'boolean') playing.value = e.payload.playing
    // 全局字体（设置页修改即时联动；空串 = 恢复默认字体栈）
    if (typeof e.payload?.font === 'string') document.body.style.fontFamily = e.payload.font
    // 逐字数据与时钟锚点：仅在有 payload 字段时覆盖（锚点可单独推送）
    if (Array.isArray(e.payload?.words)) words.value = e.payload.words
    else if (e.payload?.words === null) words.value = []
    if (e.payload?.anchor) anchor.value = e.payload.anchor
  })
  // 通知主窗口：浮窗已就绪，请求推送当前歌词与配置
  void emit('lyrics:ready')
})
onBeforeUnmount(() => unlisten?.())

/** 卡拉OK当前歌词轴位置（毫秒）：按锚点 + 本地时钟插值，避免与主窗口逐帧通信 */
const karaokeMs = ref(0)
let karaokeRaf = 0
watch(
  words,
  (w) => {
    cancelAnimationFrame(karaokeRaf)
    if (!w.length) return
    const tick = () => {
      const a = anchor.value
      karaokeMs.value = a.running ? a.posMs + (Date.now() - a.at) * a.rate : a.posMs
      karaokeRaf = requestAnimationFrame(tick)
    }
    karaokeRaf = requestAnimationFrame(tick)
  },
  { immediate: true },
)
onBeforeUnmount(() => cancelAnimationFrame(karaokeRaf))

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
 * 逐字行的描边：渐变填充走 background-clip:text（透明文字），text-shadow 会叠在
 * 渐变上把颜色压暗，改用文字描边（paint-order: stroke 让描边垫在填充下方）。
 */
const strokeOutline = computed(() => {
  if (!config.value.outline) return { textShadow: 'none' }
  return {
    textShadow: 'none',
    WebkitTextStroke: `${Math.max(2, config.value.fontSize * 0.08)}px ${config.value.outlineColor}`,
    paintOrder: 'stroke',
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
 * 值允许 undefined（strokeOutline 未命中分支的透传，Vue 行内样式忽略 undefined） */
type DeskRow = { text: string; words?: QrcWord[]; style: Record<string, string | number | undefined> }
/** 渲染行：开启翻译时只显示当前句——第一行歌词、第二行该句的翻译（两行固定，
 * 不随 active 交替换位，也不再显示下一句预告）；关闭翻译时恢复原逻辑：
 * 单行只有播放行，双行两行位置固定（对齐固定）只交换文字与高亮。
 * 逐字行（words 非空）额外携带词级时间轴与描边覆盖，模板里按字渲染渐变 */
const rows = computed<DeskRow[]>(() => {
  const wordsFor = (row: 0 | 1) => (active.value === row && words.value.length > 1 ? words.value : undefined)
  // 翻译模式：当前句永远渲染在第一行（歌词）+ 第二行（翻译），不能复用 rowStyle 的
  // active 换色逻辑（active 交替会让固定行位置的颜色抖动），颜色手动指定
  if (config.value.showTranslation) {
    const w = words.value.length > 1 ? words.value : undefined
    const alignOf = (row: 0 | 1) =>
      config.value.align === 'split' ? (row === 0 ? 'left' : 'right') : config.value.align
    // 歌词行基础样式；逐字行用描边覆盖替代阴影（与 rowStyle 路径一致），二选一避免重复声明
    const mainBase = {
      color: config.value.color,
      fontSize: `${config.value.fontSize}px`,
      fontWeight: config.value.bold ? 700 : 500,
      textAlign: alignOf(0),
    }
    return [
      {
        text: lines.value[active.value] || EMPTY_LYRIC,
        words: w,
        style: { ...mainBase, ...(w ? strokeOutline.value : { textShadow: textShadow.value }) },
      },
      {
        // 该句无翻译时以空行占位，保持两行结构稳定（避免逐句跳动）
        text: translations.value[active.value] || '\u00A0',
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
        style: { ...rowStyle(0), color: config.value.color, ...(w ? strokeOutline.value : {}) },
      },
    ]
  }
  return [
    {
      text: lines.value[0] || (active.value === 0 ? EMPTY_LYRIC : '\u00A0'),
      words: wordsFor(0),
      style: wordsFor(0) ? { ...rowStyle(0), ...strokeOutline.value } : rowStyle(0),
    },
    {
      text: lines.value[1] || '\u00A0',
      words: wordsFor(1),
      style: wordsFor(1) ? { ...rowStyle(1), ...strokeOutline.value } : rowStyle(1),
    },
  ]
})
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
      <button class="dl-btn dl-close" v-tooltip="$t('tray.disableDesktopLyrics')" @click="control('close')">
        <X class="h-4 w-4" />
      </button>
    </div>
    <!-- 歌词（背景铺满整个面板）：播放行主样式，另一行次样式，双行交替滚动；
         逐字行按词渲染双色渐变（空白用 whitespace-pre 保留），否则整行纯文本 -->
    <p v-for="(row, i) in rows" :key="i" class="dl-line" data-tauri-drag-region :style="row.style">
      <template v-if="row.words">
        <span v-for="(w, wi) in row.words" :key="wi" class="dl-word" :style="wordStyle(w)">{{ w.word }}</span>
      </template>
      <template v-else>{{ row.text }}</template>
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
  text-overflow: ellipsis;
}
/* 逐字单词：保留词内/词间空白（连续空格词不被折叠） */
.dl-word {
  white-space: pre;
}
</style>