<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { PlayIcon as Play } from '@solar-icons/vue/bold/play'
import { usePlayerStore } from '@/stores/player'
import { useI18n } from 'vue-i18n'
import KaraokeWords from '@/components/KaraokeWords.vue'

const { t } = useI18n()
const player = usePlayerStore()
const container = ref<HTMLElement | null>(null)

/** 歌词行对齐：center = 居中（经典/上下），left = 靠左（黑胶） */
withDefaults(defineProps<{ align?: 'center' | 'left' }>(), { align: 'center' })

// ---- QRC 逐字高亮：由 KaraokeWords 子组件自驱动 rAF 渐变填充，
//      本组件不再每帧重渲染（进度时钟只驱动子组件的几个词） ----

/** 活动行是否为逐字行（QRC 行携带多个带时间的单词） */
function isWordLine(i: number): boolean {
  const line = player.lyricsWordLines?.[i]
  return !!line && line.words.length > 1
}

/** 未唱色：压暗的白，与未激活行同调 */
const UNSUNG = 'rgba(255, 255, 255, 0.38)'
/** 已唱色：跟随播放页主题色（--np-accent 由 App 随环境色下发） */
const SUNG = 'var(--np-accent, #ffffff)'

/** 用户手动滚动后暂停自动跟随 8s（翻看歌词不被拽回）；切歌/歌词重载时立即恢复跟随 */
const FOLLOW_RESUME_MS = 8000
let lastManualScroll = 0
function onUserScroll() {
  lastManualScroll = Date.now()
}

const hasSynced = computed(() => !!player.lyricsLines?.length)

/** 时间戳文字：秒 → m:ss（与播放条时间显示一致） */
function fmt(s: number) {
  if (!Number.isFinite(s) || s < 0) return '0:00'
  const m = Math.floor(s / 60)
  const sec = Math.floor(s % 60)
  return `${m}:${String(sec).padStart(2, '0')}`
}

/** 跳转提示（带参翻译，模板 \`$t\` 无带参重载） */
const jumpTip = (time: number, text?: string) =>
  text
    ? t('lyrics.jumpTo', { time: fmt(time), text })
    : t('lyrics.jumpToInterlude', { time: fmt(time) })

/**
 * 歌词字号随与当前行的距离阶梯递减（px）：[当前行, ±1, ±2, ±3, ±4]，更远的行保持末档。
 * 行高仍由 text-base 固定，滚动布局不随字号变化跳动。
 */
const LYRIC_FONT_STEPS = [20, 18, 16.5, 15.5, 15]
const LYRIC_BASE_PX = 16
function lyricFontSize(i: number, hasText: string | undefined): string | undefined {
  if (!hasText) return undefined // 间奏占位行保持原有小字号
  const active = player.activeLyricIndex
  if (active < 0) return `${LYRIC_BASE_PX}px` // 尚无激活行（未开播）：全部基础字号
  const d = Math.abs(i - active)
  const px = d < LYRIC_FONT_STEPS.length ? LYRIC_FONT_STEPS[d] : LYRIC_FONT_STEPS[LYRIC_FONT_STEPS.length - 1]
  return `${px}px`
}

/** 上下留白 = 容器半高：首行歌词正好从垂直中心开始，滚动连续无跳变 */
const pad = ref(0)
let resizeObserver: ResizeObserver | null = null
function updatePad() {
  if (container.value) {
    pad.value = Math.max(0, container.value.clientHeight / 2 - 20)
  }
}
onMounted(() => {
  updatePad()
  resizeObserver = new ResizeObserver(updatePad)
  if (container.value) resizeObserver.observe(container.value)
})
onBeforeUnmount(() => resizeObserver?.disconnect())

function scrollToActive() {
  // 用户 8s 内手动滚动过：暂停自动跟随，避免想往上翻看歌词时被拽回来
  if (Date.now() - lastManualScroll < FOLLOW_RESUME_MS) return
  const idx = player.activeLyricIndex
  if (idx < 0 || !container.value || !player.lyricsLines) return
  const el = container.value.querySelector(`[data-idx="${idx}"]`) as HTMLElement | null
  if (!el) return
  const target = el.offsetTop - container.value.clientHeight / 2 + el.clientHeight / 2
  container.value.scrollTo({ top: Math.max(0, target), behavior: 'smooth' })
}

watch(() => player.activeLyricIndex, () => void nextTick(scrollToActive))
// 歌词行加载完成 / 切歌后立即定位到当前行（并恢复自动跟随）
watch(
  () => player.lyricsLines,
  () => {
    lastManualScroll = 0
    void nextTick(scrollToActive)
  },
)
onMounted(() => void nextTick(scrollToActive))
</script>

<template>
  <div
    ref="container"
    class="no-scrollbar relative h-full scroll-smooth px-6"
    :class="hasSynced ? 'overflow-y-auto' : 'flex flex-col items-center justify-center overflow-hidden'"
    :style="hasSynced ? { paddingTop: pad + 'px', paddingBottom: pad + 'px' } : undefined"
    @wheel.passive="onUserScroll"
    @touchmove.passive="onUserScroll"
  >
    <!-- 加载中 -->
    <p v-if="player.lyricsLoading" class="text-center text-sm text-zinc-500">{{ $t('lyrics.loading') }}</p>

    <!-- 加密/不支持的歌词格式：明确提示，不展示乱码 -->
    <p v-else-if="player.lyricsUnsupported" class="text-center text-sm text-zinc-500">
      {{ $t('lyrics.unsupported') }}
    </p>

    <!-- 时间轴歌词 -->
    <template v-else-if="player.lyricsLines?.length">
      <div
        v-for="(line, i) in player.lyricsLines"
        :key="i"
        :data-idx="i"
        class="group relative flex items-center"
        :class="line.text ? 'py-2.5' : 'py-0.5'"
      >
        <!-- 左侧跳转按钮：悬停到该行时才显示（淡入 + 右滑入），点击跳转；歌词文字本身不响应点击。
             按钮形状底色/描边用歌词强调色半透明，hover 加深，见 .lyric-jump 样式。
             绝对定位不占布局空间：歌词行真正居中，与上方歌曲名对齐 -->
        <button
          class="lyric-jump pointer-events-none absolute top-1/2 z-10 flex h-7 w-16 -translate-y-1/2 cursor-pointer items-center justify-center gap-1 rounded-lg border px-1 font-mono text-[11px] leading-none text-[var(--np-accent,#fff)] opacity-0 transition-[opacity,transform,background-color,border-color] duration-200 ease-out group-hover:pointer-events-auto group-hover:opacity-100"
          :class="
            align === 'left'
              ? 'right-0 -translate-x-2 group-hover:translate-x-0'
              : 'left-0 translate-x-2 group-hover:translate-x-0'
          "
          v-tooltip="jumpTip(line.time + player.lyricOffset, line.text)"
          @click="player.seek(line.time + player.lyricOffset)"
        >
          <Play class="h-3 w-3 shrink-0" />
          {{ fmt(line.time) }}
        </button>
        <p
          class="min-w-0 w-full transition-[color,font-size,transform,text-shadow] duration-300 ease-out"
          :class="[
            align === 'left' ? 'text-left' : 'text-center',
            line.text ? 'text-base' : 'text-xs leading-none',
            i === player.activeLyricIndex
              ? 'scale-[1.07] font-semibold'
              : 'text-zinc-400/80 dark:text-zinc-500',
          ]"
          :style="
            i === player.activeLyricIndex
              ? {
                  fontSize: lyricFontSize(i, line.text),
                  color: 'var(--np-accent, #ffffff)',
                  textShadow: '0 0 22px var(--np-accent, #ffffff)',
                }
              : { fontSize: lyricFontSize(i, line.text) }
          "
        >
          <!-- 音译副行：与歌词文件的行序一致——音译在上、原文居中、译文在下。
               字号按 em 跟随主行的阶梯字号自动缩小，颜色给次级白
               （活动行主色由父级 line 设置，这里必须显式覆盖）。
               开关关闭、或该行没有对应内容时不渲染，不占位 -->
          <span
            v-if="player.lyricTransliteration && line.transliteration"
            class="mb-0.5 block font-normal leading-snug transition-colors duration-300"
            :class="i === player.activeLyricIndex ? 'text-white/70' : 'text-zinc-500'"
            :style="{ fontSize: '0.72em' }"
          >{{ line.transliteration }}</span>
          <!-- QRC 逐字行（活动行）：单词按播放进度渐变填充（子组件自驱动 rAF）；其余行显示整行文本 -->
          <template v-if="i === player.activeLyricIndex && isWordLine(i)">
            <KaraokeWords :words="player.lyricsWordLines?.[i]?.words ?? []" :sung="SUNG" :unsung="UNSUNG" />
          </template>
          <template v-else-if="line.text">{{ line.text }}</template>
          <!-- 间奏占位：折叠后的一行，极简 -->
          <span v-else class="tracking-[0.5em] opacity-30">···</span>
          <!-- 译文副行（原文下方） -->
          <span
            v-if="player.lyricTranslation && line.translation"
            class="mt-0.5 block font-normal leading-snug transition-colors duration-300"
            :class="i === player.activeLyricIndex ? 'text-white/60' : 'text-zinc-500/80'"
            :style="{ fontSize: '0.72em' }"
          >{{ line.translation }}</span>
        </p>
      </div>
    </template>

    <!-- 纯文本歌词 -->
    <template v-else-if="player.lyricsPlain?.length">
      <p
        v-for="(line, i) in player.lyricsPlain"
        :key="i"
        class="py-1.5 text-sm leading-relaxed text-zinc-500 dark:text-zinc-400"
        :class="align === 'left' ? 'text-left' : 'text-center'"
      >
        {{ line }}
      </p>
    </template>

    <!-- 无歌词 -->
    <p v-else class="text-center text-sm text-zinc-500">
      {{ $t('lyrics.noLyrics') }}<br /><span class="text-xs opacity-70">{{ $t('lyrics.noLyricsLocal') }}</span>
    </p>
  </div>
</template>

<style scoped>
/* 歌词区域自身滚动，不显示滚动条 */
.no-scrollbar {
  scrollbar-width: none;
}
.no-scrollbar::-webkit-scrollbar {
  display: none;
}

/* 跳转按钮形状：强调色半透明底 + 描边，hover 加深（色值跟随 --np-accent，与活动歌词行同源） */
.lyric-jump {
  background-color: color-mix(in srgb, var(--np-accent, #ffffff) 14%, transparent);
  border-color: color-mix(in srgb, var(--np-accent, #ffffff) 35%, transparent);
}
.lyric-jump:hover {
  background-color: color-mix(in srgb, var(--np-accent, #ffffff) 26%, transparent);
  border-color: color-mix(in srgb, var(--np-accent, #ffffff) 60%, transparent);
}
</style>
