<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref, watch } from 'vue'
import gsap from 'gsap'
import { AltArrowDownIcon as ChevronDown } from '@solar-icons/vue/linear/alt-arrow-down'
import { AltArrowUpIcon as ChevronUp } from '@solar-icons/vue/linear/alt-arrow-up'
import { HeartIcon as Heart } from '@solar-icons/vue/linear/heart'
import { PlaylistMinimalistic3Icon as ListMusic } from '@solar-icons/vue/linear/playlist-minimalistic-3'
import { RefreshIcon as LoaderCircle } from '@solar-icons/vue/linear/refresh'
import { TShirtIcon as TShirt } from '@solar-icons/vue/linear/t-shirt'
import { PauseIcon as Pause } from '@solar-icons/vue/bold/pause'
import { PlayIcon as Play } from '@solar-icons/vue/bold/play'
import { ListIcon as List } from '@solar-icons/vue/linear/list'
import { RepeatIcon as Repeat } from '@solar-icons/vue/linear/repeat'
import { RepeatOneIcon as Repeat1 } from '@solar-icons/vue/linear/repeat-one'
import { ShuffleIcon as Shuffle } from '@solar-icons/vue/linear/shuffle'
import { SkipPreviousIcon as SkipBack } from '@solar-icons/vue/bold/skip-previous'
import { SkipNextIcon as SkipForward } from '@solar-icons/vue/bold/skip-next'
import { HeartIcon as HeartBold } from '@solar-icons/vue/bold/heart'
import { VolumeSmallIcon as Volume1 } from '@solar-icons/vue/linear/volume-small'
import { VolumeLoudIcon as Volume2 } from '@solar-icons/vue/linear/volume-loud'
import { MutedIcon as VolumeX } from '@solar-icons/vue/linear/muted'
import { SubtitlesIcon as Subtitles } from '@solar-icons/vue/linear/subtitles'
import { usePlayerStore, PLAYBACK_RATES, type PlayMode } from '@/stores/player'
import { useDesktopLyrics } from '@/composables/useDesktopLyrics'
import { useNav } from '@/composables/useNav'
import { useAmbient } from '@/composables/useAmbient'
import { useSkin, useSkinOpen, useSpectrumMode } from '@/composables/useSkin'
import { useNowPlayingStyle } from '@/composables/useNowPlayingStyle'
import { CloseIcon as X } from '@solar-icons/vue/linear/close'
import { ensureAnalyser, readSpectrum } from '@/composables/useSpectrum'
import { activeLineIndex } from '@/utils/lrc'
import CoverImg from '@/components/CoverImg.vue'
import MarqueeText from '@/components/MarqueeText.vue'
import { useI18n } from 'vue-i18n'

const props = defineProps<{ nowPlayingOpen?: boolean; focusHidden?: boolean }>()
const emit = defineEmits<{ toggleQueue: []; toggleNowPlaying: [] }>()

const footerEl = ref<HTMLElement | null>(null)
/** 专注模式：底部播放条下滑隐藏 / 鼠标移动时滑回 */
watch(
  () => props.focusHidden,
  (hidden, prev) => {
    if (prev === undefined) return // 初始渲染不做动画
    if (!footerEl.value) return
    // 显示要快（0.4s 减速曲线，一出专注就能用）；隐藏稍缓（0.6s 渐隐，保持沉浸感）
    gsap.to(footerEl.value, {
      yPercent: hidden ? 200 : 0,
      marginBottom: hidden ? -92 : 0,
      duration: hidden ? 0.6 : 0.4,
      ease: hidden ? 'power2.in' : 'power3.out',
      overwrite: 'auto',
    })
  },
)

const { t: tr } = useI18n()
const player = usePlayerStore()
const nav = useNav()
const { palette } = useAmbient()

// ---- 装扮：播放页布局 + 频谱三态（大面板挂在音量左侧，入口仅在播放页显示） ----
const skin = useSkin()
const skinOpen = useSkinOpen()
// 播放页布局预设：装扮面板里先可选可存，NowPlayingView 应用布局待下一步接入
const npStyle = useNowPlayingStyle()
// 频谱三态：无 / 圆形粒子 / 树状（写回 skin.on + skin.style）
const spectrumMode = useSpectrumMode()

// ---- 布局预览：用当前曲目真实封面/文案渲染示例，无曲目或无歌词时回退示例文案 ----
const previewTitle = computed(() => player.current?.title ?? tr('nowPlaying.sampleTitle'))
const previewArtist = computed(() => {
  const cur = player.current
  if (cur?.artists?.length) return cur.artists.map((a) => a.name).join(' / ')
  return cur?.artist ?? tr('nowPlaying.sampleArtist')
})
/** 歌词三行 [上一句, 当前句, 下一句]：跟随播放进度取窗口；首句之前「上一句」留空 */
const previewLyrics = computed(() => {
  const lines = player.lyricsLines
  if (lines?.length) {
    const i = player.activeLyricIndex >= 0 ? player.activeLyricIndex : 0
    return {
      prev: i > 0 ? (lines[i - 1]?.text ?? '') : '',
      now: lines[i]?.text ?? '',
      next: lines[i + 1]?.text ?? '',
    }
  }
  return {
    prev: tr('nowPlaying.sampleLyricPrev'),
    now: tr('nowPlaying.sampleLyricNow'),
    next: tr('nowPlaying.sampleLyricNext'),
  }
})
/** 预览小屏的环境光晕：跟随当前专辑环境色 */
const previewGlow = computed(() => palette.value?.accent ?? '#8b5cf6')

// ---- 装扮面板跟随播放页主题：背景用环境渐变，强调色取专辑主色（无封面回退默认紫） ----
// 面板只在播放页打开时出现，始终浮于环境背景之上，因此固定用深色玻璃 + 动态强调色
const skinPanelBg = computed(() => {
  const p = palette.value
  return p
    ? `linear-gradient(to bottom, ${p.glow} 0%, ${p.deep} 55%, #09090b 100%)`
    : 'linear-gradient(to bottom, #2e1065 0%, #09090b 55%, #09090b 100%)'
})
const skinAccent = computed(() => palette.value?.accent ?? '#a78bfa')
const skinAccentSoft = computed(() => palette.value?.accentSoft ?? 'rgba(139, 92, 246, 0.2)')
/** 选中态选项：边框/底色/文字统一取环境强调色（未选中走默认 class，返回 undefined） */
function skinActiveStyle(active: boolean) {
  return active
    ? { borderColor: skinAccent.value, backgroundColor: skinAccentSoft.value, color: skinAccent.value }
    : undefined
}

const skinPop = ref<HTMLElement | null>(null)

watch(
  () => props.nowPlayingOpen,
  (open) => {
    if (!open) skinOpen.value = false
  },
)

// ---- 播放倍速：点击循环切换（0.5x → … → 2x → 0.5x），状态与持久化在 player store ----
function cycleRate() {
  const i = PLAYBACK_RATES.indexOf(player.rate)
  player.setRate(PLAYBACK_RATES[(i + 1) % PLAYBACK_RATES.length])
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

/** 当前曲目的艺人列表：优先用后端拆分的多艺人，回退到合并字符串 */
const currentArtistLinks = computed<{ id: number | null; name: string }[]>(() => {
  const t = player.current
  if (!t) return []
  if (t.artists?.length) return t.artists.map((a) => ({ id: a.id, name: a.name }))
  if (t.artist) return [{ id: t.artistId, name: t.artist }]
  return [{ id: null, name: tr('artist.unknownArtist') }]
})

/** 播放条内导航：若播放页展开着，导航后收起，让用户看到目标页面 */
function goArtist(artist: { id: number | null; name: string }) {
  if (artist.id == null) return
  nav.go({ view: 'tracks', artistId: artist.id, artistName: artist.name })
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

// ---- 桌面歌词开关（音量之后） ----
const { enabled: dlEnabled, toggle: dlToggle } = useDesktopLyrics()
/** 音量条/图标悬停时显示音量数字气泡 */
const volHover = ref(false)

// ---- 音量条悬停气泡：跟随鼠标位置显示音量数字 ----
const volBubbleLeftPx = ref(48)
/** 鼠标悬停位置对应的音量百分比（0-100） */
const volPreview = ref(player.volume * 100)
/** 气泡显示的文本：悬停时用悬停位置的预览值，否则用当前音量 */
const volBubbleText = computed(() => (volHover.value ? Math.round(volPreview.value) : volDisplay.value))
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

// ---- 滚轮调节音量：图标与滑块上滚动，每格 5%，调节时短暂显示音量气泡 ----
let volWheelTimer: ReturnType<typeof setTimeout> | undefined
function onVolumeWheel(e: WheelEvent) {
  const step = 0.05
  const base = player.muted ? 0 : player.volume
  const next = Math.min(1, Math.max(0, Math.round((base + (e.deltaY < 0 ? step : -step)) * 100) / 100))
  player.setVolume(next)
  // 气泡反馈：跟随鼠标所在音量位置（滑块 w-24 ≈ 96px，clamp 到与 onVolMove 一致的边距）
  volHover.value = true
  volPreview.value = next * 100
  volBubbleLeftPx.value = Math.min(Math.max(28, next * 96), 96 - 28)
  clearTimeout(volWheelTimer)
  volWheelTimer = setTimeout(() => {
    volHover.value = false
  }, 600)
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
  if (!lines?.length) return player.current?.album ?? tr('player.noLyrics')
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

const modeMeta = computed<Record<PlayMode, { label: string; icon: typeof Repeat | typeof List }>>(() => ({
  // 顺序播放（列表播放）用 list-linear，与列表循环的 repeat 区分
  order: { label: tr('player.sequence'), icon: List },
  loop: { label: tr('player.repeat'), icon: Repeat },
  one: { label: tr('player.repeatOne'), icon: Repeat1 },
  shuffle: { label: tr('player.shuffle'), icon: Shuffle },
}))

/** 带参提示文案（模板 `$t` 无带参重载，统一在 setup 内生成） */
const artistTip = (name: string) => tr('artist.viewArtist', { name })
const rateTip = computed(() => tr('player.rateHint', { rate: player.rate }))
const volTip = computed(() =>
  player.muted
    ? tr('player.mutedHint', { vol: volDisplay.value })
    : tr('player.volumeHint', { vol: volDisplay.value }),
)

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
// 滚轮音量气泡的复位定时器：卸载后不再写已卸载组件的 ref
onUnmounted(() => clearTimeout(volWheelTimer))

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
        iconBtn: 'text-white/70 hover:bg-white/10 hover:text-[var(--accent)]',
        plainBtn: 'text-white/80 hover:bg-white/10 hover:text-[var(--accent)]',
        playBtn: 'bg-violet-500 text-white hover:bg-violet-400',
        trackRow: '',
      }
    : {
        bar: 'bg-white dark:bg-zinc-900',
        time: 'text-zinc-400',
        title: 'text-zinc-800 dark:text-zinc-100',
        artist: 'text-zinc-500 dark:text-zinc-400',
        iconBtn:
          'text-zinc-500 hover:bg-zinc-200/70 hover:text-violet-600 dark:text-zinc-400 dark:hover:bg-zinc-800 dark:hover:text-violet-300',
        plainBtn:
          'text-zinc-600 hover:bg-zinc-200/70 hover:text-violet-600 dark:text-zinc-300 dark:hover:bg-zinc-800 dark:hover:text-violet-300',
        playBtn: 'bg-violet-500 text-white hover:bg-violet-400',
        trackRow: '',
      },
)
</script>

<template>
  <footer
    ref="footerEl"
    class="relative z-20 flex h-20 shrink-0 items-center gap-4 rounded-2xl px-4 transition-colors duration-500"
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
        v-tooltip="props.nowPlayingOpen ? $t('player.collapseNowPlaying') : $t('player.expandNowPlaying')"
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
          <span v-else class="truncate font-medium" :class="theme.title">{{ $t('player.notPlaying') }}</span>
          <span v-if="player.current" class="shrink-0 opacity-40">–</span>
          <!-- 多艺人：每个名字独立可点击（区分每一个艺人） -->
          <template v-for="(a, i) in currentArtistLinks" :key="a.id ?? `na-${i}`">
            <button
              v-if="a.id != null"
              class="min-w-0 cursor-pointer truncate transition hover:text-violet-500 hover:underline"
              :class="theme.artist"
              v-tooltip="artistTip(a.name)"
              @click.stop="goArtist(a)"
            >{{ a.name }}</button>
            <span v-else class="min-w-0 truncate" :class="theme.artist">{{ a.name }}</span>
            <span v-if="i < currentArtistLinks.length - 1" class="shrink-0 opacity-40"> / </span>
          </template>
        </div>
        <!-- 行2：当前歌词（过长滚动），无歌词时显示专辑名；纯展示，不响应点击 -->
        <div
          v-if="player.current"
          class="w-full text-left text-xs leading-tight transition-colors duration-500 [-webkit-font-smoothing:antialiased]"
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
          class="relative flex h-8 w-8 cursor-pointer items-center justify-center rounded-full transition-colors duration-500 hover:duration-200"
          :class="[theme.iconBtn, player.mode !== 'order' && !props.nowPlayingOpen ? '!text-violet-500 dark:!text-violet-400' : '']"
          :style="accent && player.mode !== 'order' ? { color: accent } : undefined"
          v-tooltip="modeMeta[player.mode].label"
          @click="cycleMode"
        >
          <component :is="modeMeta[player.mode].icon" class="h-4 w-4" />
        </button>
        <button
          class="flex h-9 w-9 cursor-pointer items-center justify-center rounded-full transition-colors duration-500 hover:duration-200"
          :class="theme.plainBtn"
          v-tooltip="$t('player.prev') + ' (P)'"
          @click="player.prev()"
        >
          <SkipBack class="h-4.5 w-4.5" />
        </button>
        <button
          class="relative flex h-10 w-10 cursor-pointer items-center justify-center rounded-full shadow-lg shadow-violet-500/30 transition duration-200 hover:scale-110 active:scale-90"
          :class="theme.playBtn"
          :style="playBtnStyle"
          v-tooltip="player.buffering ? $t('player.buffering') : $t('player.playPauseHint')"
          @click="player.toggle()"
        >
          <!-- 播放中的脉冲光环（专注模式隐藏，避免光环从屏幕底部边缘露出） -->
          <span
            v-if="player.playing && !player.buffering && !props.focusHidden"
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
          class="flex h-9 w-9 cursor-pointer items-center justify-center rounded-full transition-colors duration-500 hover:duration-200"
          :class="theme.plainBtn"
          v-tooltip="$t('player.next') + ' (N)'"
          @click="player.next()"
        >
          <SkipForward class="h-4.5 w-4.5" />
        </button>
        <button
          class="relative flex h-9 w-9 cursor-pointer items-center justify-center rounded-full transition-colors duration-500 hover:duration-200 disabled:cursor-not-allowed"
          :class="player.current?.fav ? 'text-red-500 hover:bg-red-500/10' : theme.plainBtn"
          v-tooltip="player.current?.fav ? $t('library.unlike') : $t('library.like')"
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
            <span class="max-w-[240px] truncate text-[11px] leading-none text-white/70" v-tooltip="hoverLyric">{{ hoverLyric }}</span>
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

    <!-- 右：倍速 / 皮肤 / 音量 / 队列 -->
    <div class="flex w-64 items-center justify-end gap-1">
      <!-- 倍速循环按钮：非 1x 时高亮提示当前处于变速播放 -->
      <button
        class="flex h-8 w-9 shrink-0 cursor-pointer items-center justify-center rounded-full text-xs font-semibold tabular-nums transition-colors duration-500 hover:duration-200"
        :class="player.rate === 1 ? theme.iconBtn : 'text-violet-500 hover:bg-violet-500/10'"
        v-tooltip="rateTip"
        @click="cycleRate"
      >
        {{ player.rate }}x
      </button>
      <!-- 皮肤：频谱开关 + 样式选择（音量左侧；入口仅在播放页显示） -->
      <div v-if="props.nowPlayingOpen" ref="skinPop" class="relative">
        <button
          class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-full transition-colors duration-500 hover:duration-200"
          :class="theme.iconBtn"
          v-tooltip="$t('nowPlaying.skin')"
          @click="skinOpen = !skinOpen"
        >
          <TShirt class="h-4 w-4" />
        </button>
        <Transition
          enter-active-class="transition duration-200 ease-out"
          enter-from-class="translate-y-3 scale-[0.97] opacity-0"
          leave-active-class="transition duration-150 ease-in"
          leave-to-class="translate-y-3 scale-[0.97] opacity-0"
        >
          <div
            v-if="skinOpen"
            class="fixed right-2 bottom-[88px] z-50 flex max-h-[calc(100vh-120px)] w-80 origin-bottom-right flex-col overflow-hidden rounded-xl border border-white/10 shadow-2xl"
            :style="{ background: skinPanelBg }"
          >
            <header class="flex shrink-0 items-center justify-between border-b border-white/10 px-4 py-3">
              <p class="text-sm font-medium text-white/90">{{ $t('nowPlaying.skin') }}</p>
              <button
                class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-lg text-white/50 transition hover:bg-white/10 hover:text-white"
                @click="skinOpen = false"
              >
                <X class="h-4 w-4" />
              </button>
            </header>
            <div class="min-h-0 flex-1 overflow-y-auto p-3">
              <!-- 播放页布局：三档大预览，纵向排列 -->
              <p class="px-1 pb-2 text-[11px] font-medium text-white/40">{{ $t('nowPlaying.layoutStyle') }}</p>
              <div class="space-y-2">
                <!-- 经典：左方形封面 + 右居中歌词 -->
                <button
                  class="w-full cursor-pointer rounded-xl border border-white/10 p-2 text-left transition hover:border-white/20"
                  :style="skinActiveStyle(npStyle === 'side')"
                  @click="npStyle = 'side'"
                >
                  <span class="relative flex h-24 w-full items-center gap-3 overflow-hidden rounded-lg bg-zinc-950 p-2.5">
                    <span
                      class="pointer-events-none absolute -bottom-12 left-8 h-24 w-44 rounded-full opacity-30 blur-2xl"
                      :style="{ background: previewGlow }"
                    ></span>
                    <span class="relative aspect-square h-full shrink-0 overflow-hidden rounded-md">
                      <CoverImg :album-id="player.current?.albumId ?? null" class="h-full w-full" rounded="rounded-md" />
                    </span>
                    <span class="relative flex min-w-0 flex-1 flex-col items-center gap-[3px] text-center leading-none">
                      <span class="w-full truncate text-[10px] font-semibold text-white">{{ previewTitle }}</span>
                      <span class="w-full truncate text-[8px] text-white/50">{{ previewArtist }}</span>
                      <span class="mt-1.5 w-full truncate text-[8px] text-white/40">{{ previewLyrics.prev }}</span>
                      <span class="w-full truncate text-[9px] font-medium text-white">{{ previewLyrics.now }}</span>
                      <span class="w-full truncate text-[8px] text-white/40">{{ previewLyrics.next }}</span>
                    </span>
                  </span>
                  <span class="mt-2 flex items-center justify-between px-0.5">
                    <span
                      class="text-sm"
                      :class="npStyle === 'side' ? 'font-medium' : 'text-white/60'"
                      :style="npStyle === 'side' ? { color: skinAccent } : undefined"
                    >{{ $t('nowPlaying.layoutSide') }}</span>
                    <span
                      v-if="npStyle === 'side'"
                      class="h-2 w-2 rounded-full"
                      :style="{ background: skinAccent }"
                    ></span>
                  </span>
                </button>
                <!-- 上下：封面居上 + 歌词居下 -->
                <button
                  class="w-full cursor-pointer rounded-xl border border-white/10 p-2 text-left transition hover:border-white/20"
                  :style="skinActiveStyle(npStyle === 'stacked')"
                  @click="npStyle = 'stacked'"
                >
                  <span
                    class="relative flex h-24 w-full flex-col items-center justify-between overflow-hidden rounded-lg bg-zinc-950 p-2.5"
                  >
                    <span
                      class="pointer-events-none absolute -bottom-12 left-1/2 h-24 w-44 -translate-x-1/2 rounded-full opacity-30 blur-2xl"
                      :style="{ background: previewGlow }"
                    ></span>
                    <span class="relative aspect-square h-3/5 overflow-hidden rounded-md">
                      <CoverImg :album-id="player.current?.albumId ?? null" class="h-full w-full" rounded="rounded-md" />
                    </span>
                    <span class="relative flex w-full flex-col items-center gap-[3px] text-center leading-none">
                      <span class="w-full truncate text-[9px] font-semibold text-white">{{ previewTitle }}</span>
                      <span class="w-full truncate text-[8px] text-white/40">{{ previewLyrics.prev }}</span>
                      <span class="w-full truncate text-[9px] font-medium text-white">{{ previewLyrics.now }}</span>
                    </span>
                  </span>
                  <span class="mt-2 flex items-center justify-between px-0.5">
                    <span
                      class="text-sm"
                      :class="npStyle === 'stacked' ? 'font-medium' : 'text-white/60'"
                      :style="npStyle === 'stacked' ? { color: skinAccent } : undefined"
                    >{{ $t('nowPlaying.layoutStacked') }}</span>
                    <span
                      v-if="npStyle === 'stacked'"
                      class="h-2 w-2 rounded-full"
                      :style="{ background: skinAccent }"
                    ></span>
                  </span>
                </button>
              </div>
              <!-- 频谱样式：无 / 圆形粒子 / 树状 三选一 -->
              <p class="px-1 pt-4 pb-2 text-[11px] font-medium text-white/40">{{ $t('nowPlaying.spectrumStyle') }}</p>
              <div class="grid grid-cols-3 gap-1.5">
                <button
                  class="cursor-pointer rounded-lg px-2 py-2 text-xs transition"
                  :class="spectrumMode === 'none' ? 'font-medium' : 'text-white/60 hover:bg-white/10'"
                  :style="skinActiveStyle(spectrumMode === 'none')"
                  @click="spectrumMode = 'none'"
                >
                  {{ $t('nowPlaying.spectrumNone') }}
                </button>
                <button
                  class="cursor-pointer rounded-lg px-2 py-2 text-xs transition"
                  :class="spectrumMode === 'particles' ? 'font-medium' : 'text-white/60 hover:bg-white/10'"
                  :style="skinActiveStyle(spectrumMode === 'particles')"
                  @click="spectrumMode = 'particles'"
                >
                  {{ $t('nowPlaying.skinParticlesRound') }}
                </button>
                <button
                  class="cursor-pointer rounded-lg px-2 py-2 text-xs transition"
                  :class="spectrumMode === 'tree' ? 'font-medium' : 'text-white/60 hover:bg-white/10'"
                  :style="skinActiveStyle(spectrumMode === 'tree')"
                  @click="spectrumMode = 'tree'"
                >
                  {{ $t('nowPlaying.skinTreeShape') }}
                </button>
              </div>
            </div>
          </div>
        </Transition>
      </div>

      <button
        class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-full transition-colors duration-500 hover:duration-200"
        :class="theme.iconBtn"
        v-tooltip="volTip"
        @click="player.toggleMute()"
        @wheel.prevent="onVolumeWheel"
      >
        <VolumeX v-if="player.muted" class="h-4 w-4" />
        <Volume1 v-else-if="player.volume < 0.5" class="h-4 w-4" />
        <Volume2 v-else class="h-4 w-4" />
      </button>
      <div class="relative flex w-24 items-center" @wheel.prevent="onVolumeWheel">
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
      <!-- 桌面歌词开关：音量之后 -->
      <button
        class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-full transition-colors duration-500 hover:duration-200"
        :class="dlEnabled ? 'text-violet-500 hover:bg-violet-500/10' : theme.iconBtn"
        v-tooltip="dlEnabled ? $t('tray.disableDesktopLyrics') : $t('tray.enableDesktopLyrics')"
        @click="dlToggle()"
      >
        <Subtitles class="h-4 w-4" />
      </button>
      <button
        data-queue-toggle
        class="ml-2 flex h-8 w-8 cursor-pointer items-center justify-center rounded-full transition-colors duration-500 hover:duration-200"
        :class="theme.iconBtn"
        v-tooltip="$t('queue.title')"
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
