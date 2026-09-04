<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
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

// 桌面歌词浮窗：接收主窗口推送的歌词行与配置进行渲染；
// 整窗透明，按住文字区域可拖动（data-tauri-drag-region）。
// 鼠标悬停时在歌词上方浮现控制条（半透明背景）：
// 上一首 / 播放暂停 / 下一首 · 歌词校准（后退/还原/前进） · 关闭，
// 指令通过 lyrics:control 事件发回主窗口由播放器执行。
const lines = ref<string[]>([])
/** 当前播放行所在位置：0=第一行，1=第二行（双行交替滚动） */
const active = ref<0 | 1>(0)
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
})
const playing = ref(false)

let unlisten: (() => void) | undefined
onMounted(async () => {
  // 兜底透明背景（避免主题样式给浮窗加上底色）
  document.documentElement.style.background = 'transparent'
  document.body.style.background = 'transparent'
  unlisten = await listen<{
    lines: string[]
    active?: 0 | 1
    config: DeskLyricsConfig
    playing?: boolean
    font?: string
  }>('lyrics:sync', (e) => {
    if (Array.isArray(e.payload?.lines)) lines.value = e.payload.lines
    if (e.payload?.active === 0 || e.payload?.active === 1) active.value = e.payload.active
    if (e.payload?.config) config.value = { ...config.value, ...e.payload.config }
    if (typeof e.payload?.playing === 'boolean') playing.value = e.payload.playing
    // 全局字体（设置页修改即时联动；空串 = 恢复默认字体栈）
    if (typeof e.payload?.font === 'string') document.body.style.fontFamily = e.payload.font
  })
  // 通知主窗口：浮窗已就绪，请求推送当前歌词与配置
  void emit('lyrics:ready')
})
onBeforeUnmount(() => unlisten?.())

function control(action: DeskControl) {
  void emit('lyrics:control', action)
}

/** hex 颜色 + 不透明度 → rgba() */
function hexToRgba(hex: string, alpha: number): string {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim())
  if (!m) return `rgba(0, 0, 0, ${alpha})`
  const n = parseInt(m[1], 16)
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${alpha})`
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
/** 控制条背景：与面板同色系，略微加深以便在面板上浮起；无面板背景时用默认深色 */
const controlsStyle = computed(() => ({
  background:
    config.value.bgOpacity > 0
      ? hexToRgba(config.value.bgColor, Math.min(0.9, config.value.bgOpacity + 0.2))
      : 'rgba(24, 24, 27, 0.55)',
}))
/** 渲染行：单行只显示播放行；双行两行位置固定（对齐固定），只交换文字与高亮 */
const rows = computed(() => {
  if (config.value.lines === 1) {
    // 单行只有播放行，始终用播放行颜色
    return [{ text: lines.value[active.value] || EMPTY_LYRIC, style: { ...rowStyle(0), color: config.value.color } }]
  }
  return [
    { text: lines.value[0] || (active.value === 0 ? EMPTY_LYRIC : '\u00A0'), style: rowStyle(0) },
    { text: lines.value[1] || '\u00A0', style: rowStyle(1) },
  ]
})
</script>

<template>
  <div class="dl-root" data-tauri-drag-region :style="rootStyle">
    <!-- 控制条：悬停浮现，背景与面板同色系（略微加深） -->
    <div class="dl-controls" :style="controlsStyle">
      <button class="dl-btn" title="上一首" @click="control('prev')"><SkipBack class="h-4 w-4" /></button>
      <button class="dl-btn" :title="playing ? '暂停' : '播放'" @click="control('toggle')">
        <Pause v-if="playing" class="h-4.5 w-4.5" />
        <Play v-else class="h-4.5 w-4.5" />
      </button>
      <button class="dl-btn" title="下一首" @click="control('next')"><SkipForward class="h-4 w-4" /></button>
      <span class="dl-divider"></span>
      <button
        class="dl-btn"
        title="歌词后退 0.5 秒（延后显示，歌词显示快了用这个）"
        @click="control('calib-back')"
      >
        <RewindBack class="h-4 w-4" />
      </button>
      <button class="dl-btn" title="还原为默认时间轴" @click="control('calib-reset')">
        <RotateCcw class="h-3.5 w-3.5" />
      </button>
      <button
        class="dl-btn"
        title="歌词前进 0.5 秒（提前显示，歌词显示慢了用这个）"
        @click="control('calib-forward')"
      >
        <RewindForward class="h-4 w-4" />
      </button>
      <span class="dl-divider"></span>
      <button class="dl-btn dl-close" title="关闭桌面歌词" @click="control('close')">
        <X class="h-4 w-4" />
      </button>
    </div>
    <!-- 歌词（背景铺满整个面板）：播放行主样式，另一行次样式，双行交替滚动 -->
    <p v-for="(row, i) in rows" :key="i" class="dl-line" data-tauri-drag-region :style="row.style">
      {{ row.text }}
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
  padding: 5px 10px;
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
</style>