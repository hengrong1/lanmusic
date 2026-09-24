<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from 'vue'
import { usePlayerStore } from '@/stores/player'
import type { QrcWord } from '@/types'

/**
 * QRC 逐字行渲染：rAF 直读 audio.currentTime 驱动双色渐变填充（已唱色 → 未唱色）。
 * 独立成组件的原因：进度时钟每帧变化，拆出来后 60fps 的重渲染只发生在这几个词的
 * span 上，不再拖着 PlayerBar / 歌词面板整个组件树每帧做 diff。
 */
const props = defineProps<{
  words: QrcWord[]
  /** 已唱色（支持 CSS 变量字符串） */
  sung: string
  /** 未唱色 */
  unsung: string
}>()

const player = usePlayerStore()
const karaokeMs = ref(0)
let raf = 0

function loop() {
  karaokeMs.value = (player.audio.currentTime - player.lyricOffset) * 1000
  raf = requestAnimationFrame(loop)
}

watch(
  () => props.words,
  (w) => {
    cancelAnimationFrame(raf)
    if (w.length) raf = requestAnimationFrame(loop)
  },
  { immediate: true },
)
onBeforeUnmount(() => cancelAnimationFrame(raf))

/** 单词样式：按播放进度做双色渐变填充（已唱色 → 未唱色） */
function wordStyle(w: QrcWord): Record<string, string> {
  const dur = Math.max(1, w.endTime - w.startTime)
  const p = Math.min(1, Math.max(0, (karaokeMs.value - w.startTime) / dur))
  if (p >= 1) return { color: props.sung }
  if (p <= 0) return { color: props.unsung }
  return {
    background: `linear-gradient(90deg, ${props.sung} ${p * 100}%, ${props.unsung} ${p * 100}%)`,
    WebkitBackgroundClip: 'text',
    backgroundClip: 'text',
    color: 'transparent',
  }
}
</script>

<template>
  <span
    v-for="(w, i) in words"
    :key="i"
    class="whitespace-pre-wrap"
    :style="wordStyle(w)"
  >{{ w.word }}</span>
</template>
