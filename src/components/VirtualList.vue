<script setup lang="ts" generic="T">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'

const props = withDefaults(
  defineProps<{
    items: T[]
    itemHeight: number
    buffer?: number
    itemKey?: (item: T, index: number) => string | number
  }>(),
  { buffer: 8 },
)

const emit = defineEmits<{ nearEnd: []; range: [start: number, end: number] }>()

const container = ref<HTMLElement | null>(null)
const scrollTop = ref(0)
const viewportHeight = ref(600)
let resizeObserver: ResizeObserver | null = null

onMounted(() => {
  if (container.value) {
    viewportHeight.value = container.value.clientHeight
    resizeObserver = new ResizeObserver(() => {
      if (container.value) {
        viewportHeight.value = container.value.clientHeight
        emitRange()
      }
    })
    resizeObserver.observe(container.value)
    emitRange()
  }
})
onBeforeUnmount(() => resizeObserver?.disconnect())

function onScroll() {
  if (!container.value) return
  scrollTop.value = container.value.scrollTop
  const el = container.value
  if (el.scrollHeight - el.scrollTop - el.clientHeight < 500) emit('nearEnd')
  emitRange()
}

/** 上报当前无缓冲的可视行范围（供"定位当前播放"判断可见性） */
function emitRange() {
  const start = Math.floor(scrollTop.value / props.itemHeight)
  const end = Math.ceil((scrollTop.value + viewportHeight.value) / props.itemHeight)
  emit('range', start, end)
}

watch(
  () => props.items,
  () => void nextTickFrame(),
)
function nextTickFrame() {
  requestAnimationFrame(emitRange)
}

const totalHeight = computed(() => props.items.length * props.itemHeight)
const start = computed(() => Math.max(0, Math.floor(scrollTop.value / props.itemHeight) - props.buffer))
const end = computed(() =>
  Math.min(
    props.items.length,
    Math.ceil((scrollTop.value + viewportHeight.value) / props.itemHeight) + props.buffer,
  ),
)
const visible = computed(() =>
  props.items.slice(start.value, end.value).map((item, i) => ({ item, index: start.value + i })),
)

function keyOf(item: T, index: number) {
  return props.itemKey ? props.itemKey(item, index) : index
}

function scrollToTop() {
  container.value?.scrollTo({ top: 0 })
}

/** 平滑滚动到指定行（默认垂直居中） */
function scrollToIndex(index: number, align: 'top' | 'center' = 'center') {
  if (!container.value) return
  const target = index * props.itemHeight
  const top =
    align === 'center'
      ? target - container.value.clientHeight / 2 + props.itemHeight / 2
      : target
  container.value.scrollTo({ top: Math.max(0, top), behavior: 'smooth' })
}
defineExpose({ scrollToTop, scrollToIndex })
</script>

<template>
  <div ref="container" class="h-full overflow-y-auto" @scroll.passive="onScroll">
    <div :style="{ height: totalHeight + 'px', position: 'relative' }">
      <div :style="{ position: 'absolute', top: start * itemHeight + 'px', left: 0, right: 0 }">
        <div v-for="entry in visible" :key="keyOf(entry.item, entry.index)" :style="{ height: itemHeight + 'px' }">
          <slot :item="entry.item" :index="entry.index" />
        </div>
      </div>
    </div>
  </div>
</template>
