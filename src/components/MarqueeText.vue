<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'

const props = defineProps<{ text: string }>()

const wrap = ref<HTMLElement | null>(null)
const el = ref<HTMLElement | null>(null)
const scrolling = ref(false)
const shift = ref(0)

function measure() {
  if (!wrap.value || !el.value) return
  const overflow = el.value.scrollWidth - wrap.value.clientWidth
  scrolling.value = overflow > 2
  shift.value = Math.max(0, overflow)
}

let ro: ResizeObserver | null = null
onMounted(() => {
  void nextTick(measure)
  ro = new ResizeObserver(measure)
  if (wrap.value) ro.observe(wrap.value)
})
onBeforeUnmount(() => ro?.disconnect())
watch(
  () => props.text,
  () => void nextTick(measure),
)
</script>

<template>
  <div ref="wrap" class="overflow-hidden whitespace-nowrap">
    <span
      ref="el"
      class="inline-block will-change-transform"
      :class="scrolling ? 'marquee' : ''"
      :style="scrolling ? { '--marquee-shift': `-${shift}px` } : undefined"
    >{{ text }}</span>
  </div>
</template>

<style scoped>
/* 文本超宽时来回滚动展示 */
.marquee {
  animation: marquee 9s linear infinite alternate;
}
.marquee:hover {
  animation-play-state: paused;
}
@keyframes marquee {
  to {
    transform: translateX(var(--marquee-shift, -50%));
  }
}
</style>
