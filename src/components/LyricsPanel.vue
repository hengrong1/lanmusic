<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { usePlayerStore } from '@/stores/player'

const player = usePlayerStore()
const container = ref<HTMLElement | null>(null)

const hasSynced = computed(() => !!player.lyricsLines?.length)

/** 时间戳文字：秒 → m:ss（与播放条时间显示一致） */
function fmt(s: number) {
  if (!Number.isFinite(s) || s < 0) return '0:00'
  const m = Math.floor(s / 60)
  const sec = Math.floor(s % 60)
  return `${m}:${String(sec).padStart(2, '0')}`
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
  const idx = player.activeLyricIndex
  if (idx < 0 || !container.value || !player.lyricsLines) return
  const el = container.value.querySelector(`[data-idx="${idx}"]`) as HTMLElement | null
  if (!el) return
  const target = el.offsetTop - container.value.clientHeight / 2 + el.clientHeight / 2
  container.value.scrollTo({ top: Math.max(0, target), behavior: 'smooth' })
}

watch(() => player.activeLyricIndex, () => void nextTick(scrollToActive))
// 歌词行加载完成 / 切歌后立即定位到当前行
watch(
  () => player.lyricsLines,
  () => void nextTick(scrollToActive),
)
onMounted(() => void nextTick(scrollToActive))
</script>

<template>
  <div
    ref="container"
    class="no-scrollbar relative h-full scroll-smooth px-6"
    :class="hasSynced ? 'overflow-y-auto' : 'flex flex-col items-center justify-center overflow-hidden'"
    :style="hasSynced ? { paddingTop: pad + 'px', paddingBottom: pad + 'px' } : undefined"
  >
    <!-- 加载中 -->
    <p v-if="player.lyricsLoading" class="text-center text-sm text-zinc-500">歌词加载中…</p>

    <!-- 时间轴歌词 -->
    <template v-else-if="player.lyricsLines?.length">
      <div
        v-for="(line, i) in player.lyricsLines"
        :key="i"
        :data-idx="i"
        class="group relative flex items-center"
        :class="line.text ? 'py-2.5' : 'py-0.5'"
      >
        <!-- 左侧跳转入口：跳转 文字 + 时间，点击才跳转到该句（歌词文字本身不响应点击） -->
        <button
          class="flex h-7 w-16 shrink-0 items-center justify-end gap-1 rounded-md pr-1 font-mono text-[11px] leading-none transition-colors duration-300 ease-out"
          :class="[
            i === player.activeLyricIndex
              ? 'text-[var(--np-accent,#fff)]'
              : 'text-zinc-400/70 group-hover:text-[var(--np-accent,#fff)] dark:text-zinc-500',
          ]"
          :title="line.text ? `跳转到 ${fmt(line.time)}：${line.text}` : `跳转到 ${fmt(line.time)}（间奏）`"
          @click="player.seek(line.time)"
        >
          <span class="font-sans">跳转</span>
          {{ fmt(line.time) }}
        </button>
        <p
          class="min-w-0 flex-1 text-center transition-[color,transform,text-shadow] duration-300 ease-out"
          :class="[
            line.text ? 'text-base' : 'text-xs leading-none',
            i === player.activeLyricIndex
              ? 'scale-[1.07] font-semibold'
              : 'text-zinc-400/80 dark:text-zinc-500',
          ]"
          :style="
            i === player.activeLyricIndex
              ? { color: 'var(--np-accent, #ffffff)', textShadow: '0 0 22px var(--np-accent, #ffffff)' }
              : undefined
          "
        >
          <template v-if="line.text">{{ line.text }}</template>
          <!-- 间奏占位：折叠后的一行，极简 -->
          <span v-else class="tracking-[0.5em] opacity-30">···</span>
        </p>
      </div>
    </template>

    <!-- 纯文本歌词 -->
    <template v-else-if="player.lyricsPlain?.length">
      <p
        v-for="(line, i) in player.lyricsPlain"
        :key="i"
        class="py-1.5 text-center text-sm leading-relaxed text-zinc-500 dark:text-zinc-400"
      >
        {{ line }}
      </p>
    </template>

    <!-- 无歌词 -->
    <p v-else class="text-center text-sm text-zinc-500">
      暂无歌词<br /><span class="text-xs opacity-70">支持 .lrc 同名文件或内嵌歌词</span>
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
</style>
