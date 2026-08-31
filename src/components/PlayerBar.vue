<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref, watch } from 'vue'
import {
  ChevronDown,
  ChevronUp,
  Heart,
  ListMusic,
  Pause,
  Play,
  Repeat,
  Repeat1,
  Shuffle,
  SkipBack,
  SkipForward,
  Volume1,
  Volume2,
  VolumeX,
} from '@lucide/vue'
import { usePlayerStore, type PlayMode } from '@/stores/player'
import { useNav } from '@/composables/useNav'
import { useAmbient } from '@/composables/useAmbient'
import { activeLineIndex } from '@/utils/lrc'
import CoverImg from '@/components/CoverImg.vue'
import MarqueeText from '@/components/MarqueeText.vue'

const props = defineProps<{ nowPlayingOpen?: boolean }>()
const emit = defineEmits<{ toggleQueue: []; toggleNowPlaying: [] }>()

const player = usePlayerStore()
const nav = useNav()
const { palette } = useAmbient()

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
function goAlbum() {
  const t = player.current
  if (t?.albumId == null) return
  nav.go({ view: 'tracks', albumId: t.albumId, albumTitle: t.album ?? '未知专辑' })
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

function seekLyric() {
  if (!player.lyricsLines?.length) return
  const i = player.activeLyricIndex
  if (i >= 0) player.seek(player.lyricsLines[i].time)
}

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
  const i = activeLineIndex(lines, hoverTime.value)
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
        bar: 'border-transparent bg-transparent',
        time: 'text-white/40',
        title: 'text-white',
        artist: 'text-white/50',
        iconBtn: 'text-white/70 hover:bg-white/10 hover:text-white',
        plainBtn: 'text-white/80 hover:bg-white/10',
        playBtn: 'bg-violet-500 text-white hover:bg-violet-400',
        trackRow: '',
      }
    : {
        bar: 'border-zinc-200 bg-white/80 dark:border-zinc-800 dark:bg-zinc-900/80',
        time: 'text-zinc-400',
        title: 'text-zinc-800 dark:text-zinc-100',
        artist: 'text-zinc-500 dark:text-zinc-400',
        iconBtn:
          'text-zinc-500 hover:bg-zinc-100 hover:text-zinc-800 dark:text-zinc-400 dark:hover:bg-zinc-800 dark:hover:text-zinc-100',
        plainBtn: 'text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800',
        playBtn: 'bg-violet-500 text-white hover:bg-violet-400',
        trackRow: '',
      },
)
</script>

<template>
  <footer
    class="relative z-20 flex h-20 shrink-0 items-center gap-4 border-t px-4 transition-colors duration-500"
    :class="theme.bar"
    :style="accentVarStyle"
  >
    <!-- 左：当前曲目 -->
    <div class="flex w-56 min-w-0 items-center gap-3">
      <button
        class="group relative rounded-lg transition"
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
          <button
            v-if="player.current"
            class="max-w-[58%] shrink-0 truncate font-medium transition-colors duration-500 hover:text-violet-500"
            :class="theme.title"
            :title="player.current.album ? `查看专辑：${player.current.album}` : ''"
            @click.stop="goAlbum"
          >{{ player.current.title }}</button>
          <span v-else class="truncate font-medium" :class="theme.title">未在播放</span>
          <span v-if="player.current" class="shrink-0 opacity-40">–</span>
          <button
            v-if="player.current && player.current.artistId != null"
            class="min-w-0 truncate transition hover:text-violet-500 hover:underline"
            :class="theme.artist"
            :title="`查看艺人：${player.current.artist ?? '未知艺人'}`"
            @click.stop="goArtist"
          >{{ player.current.artist ?? '未知艺人' }}</button>
          <span v-if="player.current && player.current.artistId == null" class="min-w-0 truncate" :class="theme.artist">
            {{ player.current.artist ?? '' }}
          </span>
        </div>
        <!-- 行2：当前歌词（过长滚动），无歌词时显示专辑名；点击跳到该句 -->
        <button
          v-if="player.current"
          class="w-full text-left transition-colors duration-500"
          :class="[theme.artist, player.lyricsLines?.length && !props.nowPlayingOpen ? 'hover:text-violet-500' : 'hover:opacity-80']"
          :style="accent && player.lyricsLines?.length ? { color: accent } : undefined"
          :title="player.lyricsLines?.length ? '点击跳转到该句' : ''"
          @click="seekLyric"
        >
          <MarqueeText :text="currentLyricLine" />
        </button>
        <span v-else class="text-xs" :class="theme.artist">LanMusic</span>
      </div>
    </div>

    <!-- 中：控制 + 进度 -->
    <div class="flex min-w-0 flex-1 flex-col items-center gap-1">
      <div class="flex items-center gap-2">
        <button
          class="relative flex h-8 w-8 items-center justify-center rounded-full transition-colors duration-500"
          :class="[theme.iconBtn, player.mode !== 'order' && !props.nowPlayingOpen ? '!text-violet-500 dark:!text-violet-400' : '']"
          :style="accent && player.mode !== 'order' ? { color: accent } : undefined"
          :title="modeMeta[player.mode].label"
          @click="cycleMode"
        >
          <component :is="modeMeta[player.mode].icon" class="h-4 w-4" />
        </button>
        <button
          class="flex h-9 w-9 items-center justify-center rounded-full transition-colors duration-500"
          :class="theme.plainBtn"
          title="上一首 (P)"
          @click="player.prev()"
        >
          <SkipBack class="h-4.5 w-4.5" fill="currentColor" stroke="none" />
        </button>
        <button
          class="relative flex h-10 w-10 items-center justify-center rounded-full shadow-lg shadow-violet-500/30 transition duration-200 hover:scale-110 active:scale-90"
          :class="theme.playBtn"
          :style="playBtnStyle"
          title="播放/暂停 (空格)"
          @click="player.toggle()"
        >
          <!-- 播放中的脉冲光环 -->
          <span
            v-if="player.playing"
            class="absolute inset-0 rounded-full bg-violet-400/50 animate-ping [animation-duration:1.8s]"
            :style="pingStyle"
          ></span>
          <Transition
            mode="out-in"
            enter-active-class="transition duration-150 ease-out"
            enter-from-class="scale-50 opacity-0"
            leave-active-class="transition duration-100 ease-in"
            leave-to-class="scale-0 opacity-0"
          >
            <Pause v-if="player.playing" key="pause" class="relative h-4.5 w-4.5" fill="currentColor" stroke="none" />
            <Play v-else key="play" class="relative ml-0.5 h-4.5 w-4.5" fill="currentColor" stroke="none" />
          </Transition>
        </button>
        <button
          class="flex h-9 w-9 items-center justify-center rounded-full transition-colors duration-500"
          :class="theme.plainBtn"
          title="下一首 (N)"
          @click="player.next()"
        >
          <SkipForward class="h-4.5 w-4.5" fill="currentColor" stroke="none" />
        </button>
        <button
          class="relative flex h-9 w-9 items-center justify-center rounded-full transition-colors duration-500"
          :class="player.current?.fav ? 'text-red-500 hover:bg-red-500/10' : theme.plainBtn"
          :title="player.current?.fav ? '取消喜欢' : '喜欢'"
          :disabled="!player.current"
          @click="player.toggleFav()"
        >
          <!-- 添加喜欢成功时的扩散光环 -->
          <span v-if="favPop" class="heart-burst pointer-events-none absolute inset-0 rounded-full bg-red-500/40"></span>
          <Heart class="h-4.5 w-4.5" :class="favPop ? 'heart-pop' : ''" :fill="player.current?.fav ? 'currentColor' : 'none'" />
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

    <!-- 右：音量 / 队列 -->
    <div class="flex w-56 items-center justify-end gap-1">
      <button
        class="flex h-8 w-8 items-center justify-center rounded-full transition-colors duration-500"
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
        class="ml-2 flex h-8 w-8 items-center justify-center rounded-full transition-colors duration-500"
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
