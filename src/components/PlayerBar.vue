<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref, watch } from 'vue'
import gsap from 'gsap'
import { AltArrowDownIcon as ChevronDown } from '@solar-icons/vue/linear/alt-arrow-down'
import { AltArrowUpIcon as ChevronUp } from '@solar-icons/vue/linear/alt-arrow-up'
import { HeartIcon as Heart } from '@solar-icons/vue/linear/heart'
import { PlaylistIcon as ListMusic } from '@solar-icons/vue/linear/playlist'
import { RefreshIcon as LoaderCircle } from '@solar-icons/vue/linear/refresh'
import { PaletteIcon as Palette } from '@solar-icons/vue/linear/palette'
import { PauseIcon as Pause } from '@solar-icons/vue/bold/pause'
import { PlayIcon as Play } from '@solar-icons/vue/bold/play'
import { RepeatIcon as Repeat } from '@solar-icons/vue/linear/repeat'
import { RepeatOneIcon as Repeat1 } from '@solar-icons/vue/linear/repeat-one'
import { ShuffleIcon as Shuffle } from '@solar-icons/vue/linear/shuffle'
import { SkipPreviousIcon as SkipBack } from '@solar-icons/vue/bold/skip-previous'
import { SkipNextIcon as SkipForward } from '@solar-icons/vue/bold/skip-next'
import { HeartIcon as HeartBold } from '@solar-icons/vue/bold/heart'
import { VolumeSmallIcon as Volume1 } from '@solar-icons/vue/linear/volume-small'
import { VolumeLoudIcon as Volume2 } from '@solar-icons/vue/linear/volume-loud'
import { VolumeCrossIcon as VolumeX } from '@solar-icons/vue/linear/volume-cross'
import { usePlayerStore, type PlayMode } from '@/stores/player'
import { useNav } from '@/composables/useNav'
import { useAmbient } from '@/composables/useAmbient'
import { useSkin, useSkinOpen } from '@/composables/useSkin'
import { ensureAnalyser, readSpectrum } from '@/composables/useSpectrum'
import { activeLineIndex } from '@/utils/lrc'
import CoverImg from '@/components/CoverImg.vue'
import MarqueeText from '@/components/MarqueeText.vue'

const props = defineProps<{ nowPlayingOpen?: boolean; focusHidden?: boolean }>()
const emit = defineEmits<{ toggleQueue: []; toggleNowPlaying: [] }>()

const footerEl = ref<HTMLElement | null>(null)
/** 专注模式：底部播放条下滑隐藏 / 鼠标移动时滑回 */
watch(
  () => props.focusHidden,
  (hidden, prev) => {
    if (prev === undefined) return // 初始渲染不做动画
    if (!footerEl.value) return
    gsap.to(footerEl.value, {
      yPercent: hidden ? 100 : 0,
      duration: 0.45,
      ease: 'power3.out',
      overwrite: 'auto',
    })
  },
)

const player = usePlayerStore()
const nav = useNav()
const { palette } = useAmbient()

// ---- 皮肤：频谱开关 + 样式选择（弹层挂在音量左侧，入口仅在播放页显示） ----
const skin = useSkin()
const skinOpen = useSkinOpen()
const skinPop = ref<HTMLElement | null>(null)

watch(
  () => props.nowPlayingOpen,
  (open) => {
    if (!open) skinOpen.value = false
  },
)

function toggleSpectrum() {
  skin.value.on = !skin.value.on
  // 在用户手势内首次创建 AudioContext / AnalyserNode，避免自动播放限制
  if (skin.value.on) ensureAnalyser()
}

// 点击弹层外部关闭（footer 带 gsap transform，fixed 遮罩会被限制在条内，故用文档监听）
function onSkinDocClick(e: MouseEvent) {
  if (skinPop.value?.contains(e.target as HTMLElement)) return
  skinOpen.value = false
}
watch(skinOpen, (v) => {
  if (v) document.addEventListener('click', onSkinDocClick, true)
  else document.removeEventListener('click', onSkinDocClick, true)
})
onUnmounted(() => document.removeEventListener('click', onSkinDocClick, true))

// ---- 树状频谱：绘制在播放条上沿（随播放条一起被专注模式动画带动） ----
const treeCanvas = ref<HTMLCanvasElement | null>(null)
const treeFreq = new Uint8Array(256)
let treeRaf = 0

function drawTree() {
  const c = treeCanvas.value
  if (!c) return
  const g = c.getContext('2d')
  if (!g) return
  const dpr = window.devicePixelRatio || 1
  const w = c.clientWidth
  const h = c.clientHeight
  if (!w || !h) return
  if (c.width !== Math.round(w * dpr) || c.height !== Math.round(h * dpr)) {
    c.width = Math.round(w * dpr)
    c.height = Math.round(h * dpr)
  }
  g.setTransform(dpr, 0, 0, dpr, 0, 0)
  g.clearRect(0, 0, w, h)

  const ok = readSpectrum(treeFreq)
  // 频谱仅在播放页显示，底色固定跟随封面主色
  const baseColor = palette.value?.accent ?? '#a78bfa'
  // 两端透明渐变：横向线性渐变作为描边色，与 per-bar 的 globalAlpha 相乘生效
  // 注意主色可能是 hsl() 格式，必须先归一化解析，非法 rgba 会让 addColorStop 抛错中断绘制
  const [rr, gg, bb] = colorToRgb(g, baseColor)
  const fade = g.createLinearGradient(0, 0, w, 0)
  fade.addColorStop(0, `rgba(${rr},${gg},${bb},0)`)
  fade.addColorStop(0.12, `rgba(${rr},${gg},${bb},1)`)
  fade.addColorStop(0.88, `rgba(${rr},${gg},${bb},1)`)
  fade.addColorStop(1, `rgba(${rr},${gg},${bb},0)`)
  const n = 128
  const bw = w / n
  g.lineCap = 'round'
  g.strokeStyle = fade
  for (let i = 0; i < n; i++) {
    // 低频段更密集：对数分布取样（1.25 次幂在 n=96 下严格递增，每根柱对应不同频段）
    const bi = Math.floor(Math.pow(i / n, 1.25) * treeFreq.length)
    // 高频段能量天然偏弱，按位置做增益补偿，让整条频谱都可见地跳动
    const raw = ok ? treeFreq[bi] / 255 : 0
    const amp = Math.min(1, raw * (1 + (i / n) * 1.6))
    // 暂停/无信号（频谱衰减到接近 0）时不画，避免残留一个小柱头
    if (amp < 0.02) continue
    const bh = amp * (h - 8)
    const x = i * bw + bw / 2
    // 主茎（细一点）
    g.globalAlpha = 0.35 + amp * 0.65
    g.lineWidth = Math.max(1.2, bw * 0.28)
    g.beginPath()
    g.moveTo(x, h)
    g.lineTo(x, h - bh)
    g.stroke()
  }
  g.globalAlpha = 1
}

/**
 * 将任意 CSS 颜色解析为 [r, g, b]。
 * 借助 canvas 的 fillStyle 归一化：赋值后读回会变成 '#rrggbb' 或 'rgba(...)'，
 * 因此 hex / hsl() / rgb() 等格式都能解析；失败时兜底为主题紫（violet-500）。
 */
function colorToRgb(g: CanvasRenderingContext2D, color: string): [number, number, number] {
  let s = color
  try {
    g.fillStyle = color
    s = String(g.fillStyle)
  } catch {
    /* 保底走下面的字符串解析 */
  }
  if (s.startsWith('#')) {
    let hex = s.slice(1)
    if (hex.length === 3 || hex.length === 4) hex = hex.slice(0, 3).split('').map((c) => c + c).join('')
    const num = parseInt(hex.slice(0, 6), 16)
    if (Number.isFinite(num)) return [(num >> 16) & 255, (num >> 8) & 255, num & 255]
  }
  const m = s.match(/rgba?\(([^)]+)\)/)
  if (m) {
    const parts = m[1].split(/[,\s/]+/).filter(Boolean).map(Number)
    if (parts.length >= 3 && parts.slice(0, 3).every((n) => Number.isFinite(n))) {
      return [parts[0], parts[1], parts[2]]
    }
  }
  return [139, 92, 246]
}

function loopTree() {
  try {
    drawTree()
  } catch {
    /* 单帧绘制失败不中断循环 */
  }
  treeRaf = requestAnimationFrame(loopTree)
}

watch(
  [() => props.nowPlayingOpen, () => skin.value.on, () => skin.value.style, treeCanvas],
  ([open, on, style, el]) => {
    cancelAnimationFrame(treeRaf)
    if (open && on && style === 'tree' && el) {
      ensureAnalyser()
      treeRaf = requestAnimationFrame(loopTree)
    }
  },
  { immediate: true },
)
onUnmounted(() => cancelAnimationFrame(treeRaf))

// 播放页展开时：进度/音量条填充色与播放按钮跟随封面主色
const accentVarStyle = computed(() => {
  if (!props.nowPlayingOpen) return undefined
  return { '--accent': palette.value?.accent ?? '#a78bfa' }
})
const playBtnStyle = computed(() => {
  if (!props.nowPlayingOpen) return undefined
  const soft = palette.value?.accentSoft ?? 'rgba(167, 139, 250, 0.45)'
  return { backgroundColor: palette.value?.accent ?? '#a78bfa', boxShadow: `0 8px 24px -8px ${soft}` }
})
const pingStyle = computed(() => {
  if (!props.nowPlayingOpen) return undefined
  return { backgroundColor: palette.value?.accentSoft ?? 'rgba(167, 139, 250, 0.5)' }
})
/** 播放页展开时的主色（封面提取）， null = 未展开 */
const accent = computed(() => (props.nowPlayingOpen ? palette.value?.accent ?? '#a78bfa' : null))

const coverRingStyle = computed(() =>
  props.nowPlayingOpen ? { boxShadow: `0 0 0 2px ${palette.value?.accent ?? '#a78bfa'}` } : undefined,
)

/** 播放条内导航：若播放页展开着，导航后收起，让用户看到目标页面 */
function goArtist() {
  const t = player.current
  if (t?.artistId == null) return
  nav.go({ view: 'tracks', artistId: t.artistId, artistName: t.artist ?? '未知艺人' })
  if (props.nowPlayingOpen) emit('toggleNowPlaying')
}

/** 当前歌词行（无时间轴歌词/无歌词时退化为专辑名） */
const currentLyricLine = computed(() => {
  if (player.lyricsLines?.length) {
    const i = player.activeLyricIndex
    return (i >= 0 ? player.lyricsLines[i].text : '') || '···'
  }
  return player.current?.album ?? 'LanMusic'
})

const pct = computed(() => (player.duration > 0 ? (player.position / player.duration) * 100 : 0))
const volPct = computed(() => (player.muted ? 0 : player.volume * 100))
const volDisplay = computed(() => Math.round(player.volume * 100))
/** 音量条/图标悬停时显示音量数字气泡 */
const volHover = ref(false)

// ---- 音量条悬停气泡：跟随鼠标位置显示音量数字 ----
const volBubbleLeftPx = ref(48)
/** 鼠标悬停位置对应的音量百分比（0-100） */
const volPreview = ref(player.volume * 100)
/** 气泡显示的文本：悬停时用悬停位置的预览值，否则用当前音量 */
const volBubbleText = computed(() => (volHover ? Math.round(volPreview.value) : volDisplay.value))
const volBubbleStyle = computed(() => ({
  left: `${volBubbleLeftPx.value}px`,
  transform: 'translateX(-50%)',
}))
function onVolMove(e: MouseEvent) {
  const el = e.currentTarget as HTMLInputElement
  const rect = el.getBoundingClientRect()
  if (rect.width <= 0) return
  const local = e.clientX - rect.left
  volPreview.value = Math.min(100, Math.max(0, (local / rect.width) * 100))
  volBubbleLeftPx.value = Math.min(Math.max(28, local), rect.width - 28)
}

// ---- 进度条悬停气泡：时间 + 对应歌词 ----
const progressHover = ref(false)
const hoverPct = ref(0)
/** 悬停位置对应的时间（秒）；未悬停时退化为当前播放进度 */
const hoverTime = computed(() =>
  progressHover.value && player.duration > 0 ? (hoverPct.value / 100) * player.duration : player.position,
)
/** 悬停时间对应的歌词文本；无时间轴歌词时退化为专辑名 */
const hoverLyric = computed(() => {
  const lines = player.lyricsLines
  if (!lines?.length) return player.current?.album ?? '暂无歌词'
  const i = activeLineIndex(lines, hoverTime.value - player.lyricOffset)
  if (i < 0) return lines[0]?.text || '···'
  return lines[i].text || '···'
})
/** 气泡水平位置（像素，相对滑条容器左边），跟随鼠标；两端收边避免溢出到时间文字上 */
const bubbleLeftPx = ref(48)
const bubblePosStyle = computed(() => ({
  left: `${bubbleLeftPx.value}px`,
  transform: 'translateX(-50%)',
}))
function onProgressMove(e: MouseEvent) {
  const el = e.currentTarget as HTMLInputElement
  const rect = el.getBoundingClientRect()
  if (rect.width <= 0) return
  const local = e.clientX - rect.left
  hoverPct.value = Math.min(100, Math.max(0, (local / rect.width) * 100))
  // clamp 到 [60, 宽-60]，让气泡完整留在滑条内
  bubbleLeftPx.value = Math.min(Math.max(60, local), rect.width - 60)
}

const modeMeta: Record<PlayMode, { label: string; icon: typeof Repeat }> = {
  order: { label: '顺序播放', icon: Repeat },
  loop: { label: '列表循环', icon: Repeat },
  one: { label: '单曲循环', icon: Repeat1 },
  shuffle: { label: '随机播放', icon: Shuffle },
}

function cycleMode() {
  const order: PlayMode[] = ['order', 'loop', 'one', 'shuffle']
  const i = order.indexOf(player.mode)
  player.mode = order[(i + 1) % order.length]
}

/** 添加喜欢成功（fav false → true，后端确认后）触发心形弹跳 + 扩散光环动画 */
const favPop = ref(false)
let favPopTimer: ReturnType<typeof setTimeout> | undefined
watch(
  () => player.current?.fav,
  (fav, old) => {
    if (!fav || old) return
    favPop.value = false
    void nextTick(() => (favPop.value = true)) // 先移除类再挂回，保证连续触发时动画重放
    clearTimeout(favPopTimer)
    favPopTimer = setTimeout(() => (favPop.value = false), 600)
  },
)
onUnmounted(() => clearTimeout(favPopTimer))

function fmt(s: number) {
  if (!Number.isFinite(s) || s < 0) return '0:00'
  const m = Math.floor(s / 60)
  const sec = Math.floor(s % 60)
  return `${m}:${String(sec).padStart(2, '0')}`
}

/** 播放页打开时切换为深色玻璃主题，与播放页背景衔接 */
const theme = computed(() =>
  props.nowPlayingOpen
    ? {
        bar: 'bg-transparent',
        time: 'text-white/40',
        title: 'text-white',
        artist: 'text-white/50',
        iconBtn: 'text-white/70 hover:bg-white/10 hover:text-white',
        plainBtn: 'text-white/80 hover:bg-white/10',
        playBtn: 'bg-violet-500 text-white hover:bg-violet-400',
        trackRow: '',
      }
    : {
        bar: 'bg-zinc-100 dark:bg-zinc-900',
        time: 'text-zinc-400',
        title: 'text-zinc-800 dark:text-zinc-100',
        artist: 'text-zinc-500 dark:text-zinc-400',
        iconBtn:
          'text-zinc-500 hover:bg-zinc-200/70 hover:text-zinc-800 dark:text-zinc-400 dark:hover:bg-zinc-800 dark:hover:text-zinc-100',
        plainBtn: 'text-zinc-600 hover:bg-zinc-200/70 dark:text-zinc-300 dark:hover:bg-zinc-800',
        playBtn: 'bg-violet-500 text-white hover:bg-violet-400',
        trackRow: '',
      },
)
</script>

<template>
  <footer
    ref="footerEl"
    class="relative z-20 flex h-20 shrink-0 items-center gap-4 px-4 transition-colors duration-500"
    :class="theme.bar"
    :style="accentVarStyle"
  >
    <!-- 树状频谱：悬于播放条上沿，占 2/3 宽并居中（不遮挡播放条内容） -->
    <canvas
      v-if="props.nowPlayingOpen && skin.on && skin.style === 'tree'"
      ref="treeCanvas"
      class="pointer-events-none absolute bottom-full left-1/2 h-10 w-2/3 -translate-x-1/2"
    ></canvas>
    <!-- 左：当前曲目 -->
    <div class="flex w-56 min-w-0 items-center gap-3">
      <button
        class="group relative cursor-pointer rounded-lg transition"
        :class="props.nowPlayingOpen ? '' : 'hover:opacity-90'"
        :style="coverRingStyle"
        :title="props.nowPlayingOpen ? '收起播放页' : '展开播放页'"
        @click="$emit('toggleNowPlaying')"
      >
        <CoverImg :album-id="player.current?.albumId ?? null" class="h-12 w-12 shrink-0" rounded="rounded-lg" />
        <!-- 悬停提示：展开 / 收起 -->
        <span
          class="absolute inset-0 hidden items-center justify-center rounded-lg bg-black/45 text-white group-hover:flex"
        >
          <ChevronDown v-if="props.nowPlayingOpen" class="h-5 w-5" />
          <ChevronUp v-else class="h-5 w-5" />
        </span>
      </button>
      <div class="flex min-w-0 flex-col justify-center gap-0.5">
        <!-- 行1：歌名 – 歌手 -->
        <div class="flex min-w-0 items-baseline gap-1.5 text-sm">
          <span
            v-if="player.current"
            class="max-w-[58%] shrink-0 truncate font-medium transition-colors duration-500"
            :class="theme.title"
          >{{ player.current.title }}</span>
          <span v-else class="truncate font-medium" :class="theme.title">未在播放</span>
          <span v-if="player.current" class="shrink-0 opacity-40">–</span>
          <button
            v-if="player.current && player.current.artistId != null"
            class="min-w-0 cursor-pointer truncate transition hover:text-violet-500 hover:underline"
            :class="theme.artist"
            :title="`查看艺人：${player.current.artist ?? '未知艺人'}`"
            @click.stop="goArtist"
          >{{ player.current.artist ?? '未知艺人' }}</button>
          <span v-if="player.current && player.current.artistId == null" class="min-w-0 truncate" :class="theme.artist">
            {{ player.current.artist ?? '' }}
          </span>
        </div>
        <!-- 行2：当前歌词（过长滚动），无歌词时显示专辑名；纯展示，不响应点击 -->
        <div
          v-if="player.current"
          class="w-full text-left transition-colors duration-500"
          :class="theme.artist"
          :style="accent && player.lyricsLines?.length ? { color: accent } : undefined"
        >
          <MarqueeText :text="currentLyricLine" />
        </div>
        <span v-else class="text-xs" :class="theme.artist">LanMusic</span>
      </div>
    </div>

    <!-- 中：控制 + 进度 -->
    <div class="flex min-w-0 flex-1 flex-col items-center gap-1">
      <div class="flex items-center gap-2">
        <button
          class="relative flex h-8 w-8 cursor-pointer items-center justify-center rounded-full transition-colors duration-500"
          :class="[theme.iconBtn, player.mode !== 'order' && !props.nowPlayingOpen ? '!text-violet-500 dark:!text-violet-400' : '']"
          :style="accent && player.mode !== 'order' ? { color: accent } : undefined"
          :title="modeMeta[player.mode].label"
          @click="cycleMode"
        >
          <component :is="modeMeta[player.mode].icon" class="h-4 w-4" />
        </button>
        <button
          class="flex h-9 w-9 cursor-pointer items-center justify-center rounded-full transition-colors duration-500"
          :class="theme.plainBtn"
          title="上一首 (P)"
          @click="player.prev()"
        >
          <SkipBack class="h-4.5 w-4.5" />
        </button>
        <button
          class="relative flex h-10 w-10 cursor-pointer items-center justify-center rounded-full shadow-lg shadow-violet-500/30 transition duration-200 hover:scale-110 active:scale-90"
          :class="theme.playBtn"
          :style="playBtnStyle"
          :title="player.buffering ? '缓冲中…' : '播放/暂停 (空格)'"
          @click="player.toggle()"
        >
          <!-- 播放中的脉冲光环 -->
          <span
            v-if="player.playing && !player.buffering"
            class="absolute inset-0 rounded-full bg-violet-400/50 animate-ping [animation-duration:1.8s]"
            :style="pingStyle"
          ></span>
          <Transition
            enter-active-class="transition duration-150 ease-out"
            enter-from-class="scale-50 opacity-0"
            leave-active-class="transition duration-100 ease-in"
            leave-to-class="scale-0 opacity-0"
          >
            <!-- 加载中：旋转的圆环 spinner（不显示播放/暂停图标） -->
            <LoaderCircle
              v-if="player.buffering"
              key="buffering"
              class="absolute top-1/2 left-1/2 h-5 w-5 -translate-x-1/2 -translate-y-1/2 animate-spin text-white"
            />
            <Pause
              v-else-if="player.playing"
              key="pause"
              class="absolute top-1/2 left-1/2 h-4.5 w-4.5 -translate-x-1/2 -translate-y-1/2"
            />
            <Play
              v-else
              key="play"
              class="absolute top-1/2 left-1/2 ml-0.5 h-4.5 w-4.5 -translate-x-1/2 -translate-y-1/2"
            />
          </Transition>
        </button>
        <button
          class="flex h-9 w-9 cursor-pointer items-center justify-center rounded-full transition-colors duration-500"
          :class="theme.plainBtn"
          title="下一首 (N)"
          @click="player.next()"
        >
          <SkipForward class="h-4.5 w-4.5" />
        </button>
        <button
          class="relative flex h-9 w-9 cursor-pointer items-center justify-center rounded-full transition-colors duration-500 disabled:cursor-not-allowed"
          :class="player.current?.fav ? 'text-red-500 hover:bg-red-500/10' : theme.plainBtn"
          :title="player.current?.fav ? '取消喜欢' : '喜欢'"
          :disabled="!player.current"
          @click="player.toggleFav()"
        >
          <!-- 添加喜欢成功时的扩散光环 -->
          <span v-if="favPop" class="heart-burst pointer-events-none absolute inset-0 rounded-full bg-red-500/40"></span>
          <HeartBold v-if="player.current?.fav" class="h-4.5 w-4.5" :class="favPop ? 'heart-pop' : ''" />
          <Heart v-else class="h-4.5 w-4.5" :class="favPop ? 'heart-pop' : ''" />
        </button>
      </div>
      <div class="flex w-full max-w-xl items-center gap-2">
        <span class="w-10 text-right font-mono text-[11px] tabular-nums transition-colors duration-500" :class="theme.time">{{ fmt(player.position) }}</span>
        <div class="relative flex min-w-0 flex-1 items-center">
          <!-- 进度条悬停气泡：时间 + 对应歌词，随鼠标平移 -->
          <div
            class="pointer-events-none absolute -top-9 z-10 flex items-baseline gap-1.5 rounded-md bg-zinc-800 px-2 py-1 shadow-lg transition-opacity duration-150 dark:bg-zinc-700"
            :class="progressHover ? 'opacity-100' : 'opacity-0'"
            :style="bubblePosStyle"
          >
            <span class="shrink-0 font-mono text-[11px] leading-none text-white tabular-nums">{{ fmt(hoverTime) }}</span>
            <span class="max-w-[240px] truncate text-[11px] leading-none text-white/70" :title="hoverLyric">{{ hoverLyric }}</span>
          </div>
          <input
            type="range"
            class="slider w-full"
            min="0"
            :max="Math.max(player.duration, 0.1)"
            step="0.1"
            :value="player.position"
            :style="{ '--fill': pct + '%' }"
            :disabled="!player.current"
            @mouseenter="progressHover = true"
            @mouseleave="progressHover = false"
            @mousemove="onProgressMove"
            @input="player.seek(Number(($event.target as HTMLInputElement).value))"
          />
        </div>
        <span class="w-10 font-mono text-[11px] tabular-nums transition-colors duration-500" :class="theme.time">{{ fmt(player.duration) }}</span>
      </div>
    </div>

    <!-- 右：皮肤 / 音量 / 队列 -->
    <div class="flex w-56 items-center justify-end gap-1">
      <!-- 皮肤：频谱开关 + 样式选择（音量左侧；入口仅在播放页显示） -->
      <div v-if="props.nowPlayingOpen" ref="skinPop" class="relative">
        <button
          class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-full transition-colors duration-500"
          :class="theme.iconBtn"
          title="皮肤"
          @click="skinOpen = !skinOpen"
        >
          <Palette class="h-4 w-4" />
        </button>
        <Transition
          enter-active-class="transition duration-150 ease-out"
          enter-from-class="translate-y-1 scale-95 opacity-0"
          leave-active-class="transition duration-100 ease-in"
          leave-to-class="translate-y-1 opacity-0"
        >
          <div
            v-if="skinOpen"
            class="absolute right-0 bottom-full z-40 mb-2 w-44 rounded-xl border border-zinc-200 bg-white p-2 shadow-xl dark:border-zinc-800 dark:bg-zinc-900"
          >
            <button
              class="flex w-full cursor-pointer items-center justify-between rounded-lg px-2 py-1.5 text-sm text-zinc-700 transition hover:bg-zinc-100 dark:text-zinc-200 dark:hover:bg-zinc-800"
              @click="toggleSpectrum"
            >
              <span>频谱</span>
              <span
                class="relative h-4 w-7 shrink-0 rounded-full transition-colors"
                :class="skin.on ? 'bg-violet-500' : 'bg-zinc-300 dark:bg-zinc-600'"
              >
                <span
                  class="absolute top-0.5 h-3 w-3 rounded-full bg-white shadow transition-all"
                  :class="skin.on ? 'left-3.5' : 'left-0.5'"
                ></span>
              </span>
            </button>
            <p class="px-2 pt-1 pb-0.5 text-[11px] text-zinc-400">频谱样式</p>
            <div class="grid grid-cols-2 gap-1">
              <button
                class="cursor-pointer rounded-lg px-2 py-1.5 text-xs transition"
                :class="
                  skin.style === 'particles'
                    ? 'bg-violet-100 font-medium text-violet-700 dark:bg-violet-500/15 dark:text-violet-300'
                    : 'text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800'
                "
                @click="skin.style = 'particles'"
              >
                圆形粒子
              </button>
              <button
                class="cursor-pointer rounded-lg px-2 py-1.5 text-xs transition"
                :class="
                  skin.style === 'tree'
                    ? 'bg-violet-100 font-medium text-violet-700 dark:bg-violet-500/15 dark:text-violet-300'
                    : 'text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800'
                "
                @click="skin.style = 'tree'"
              >
                树状
              </button>
            </div>
          </div>
        </Transition>
      </div>

      <button
        class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-full transition-colors duration-500"
        :class="theme.iconBtn"
        :title="player.muted ? `已静音（音量 ${volDisplay}%）` : `音量 ${volDisplay}%`"
        @click="player.toggleMute()"
      >
        <VolumeX v-if="player.muted" class="h-4 w-4" />
        <Volume1 v-else-if="player.volume < 0.5" class="h-4 w-4" />
        <Volume2 v-else class="h-4 w-4" />
      </button>
      <div class="relative flex w-24 items-center">
        <!-- 音量数字气泡：跟随鼠标位置显示音量数字 -->
        <div
          class="pointer-events-none absolute -top-8 z-10 rounded-md bg-zinc-800 px-2 py-1 font-mono text-[11px] leading-none text-white opacity-0 shadow-lg transition-opacity duration-150 dark:bg-zinc-700"
          :class="{ 'opacity-100': volHover }"
          :style="volBubbleStyle"
        >
          {{ volBubbleText }}%
        </div>
        <input
          type="range"
          class="slider w-full"
          min="0"
          max="1"
          step="0.01"
          :value="player.muted ? 0 : player.volume"
          :style="{ '--fill': volPct + '%' }"
          @mouseenter="volHover = true"
          @mouseleave="volHover = false"
          @mousemove="onVolMove"
          @input="player.setVolume(Number(($event.target as HTMLInputElement).value))"
        />
      </div>
      <button
        data-queue-toggle
        class="ml-2 flex h-8 w-8 cursor-pointer items-center justify-center rounded-full transition-colors duration-500"
        :class="theme.iconBtn"
        title="播放队列"
        @click="$emit('toggleQueue')"
      >
        <ListMusic class="h-4 w-4" />
      </button>
    </div>
  </footer>
</template>

<style scoped>
/* 添加喜欢成功：心形弹跳（scale 过冲回弹） */
.heart-pop {
  animation: heart-pop 0.45s cubic-bezier(0.22, 1, 0.36, 1);
}
@keyframes heart-pop {
  0% {
    transform: scale(0.4);
  }
  45% {
    transform: scale(1.4);
  }
  70% {
    transform: scale(0.9);
  }
  100% {
    transform: scale(1);
  }
}
/* 同步的扩散光环，播完自动消失（forwards） */
.heart-burst {
  animation: heart-burst 0.5s ease-out forwards;
}
@keyframes heart-burst {
  from {
    transform: scale(0.5);
    opacity: 0.7;
  }
  to {
    transform: scale(1.9);
    opacity: 0;
  }
}
</style>
