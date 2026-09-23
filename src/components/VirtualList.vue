<script setup lang="ts" generic="T">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'

const props = withDefaults(
  defineProps<{
    items: T[]
    itemHeight: number
    buffer?: number
    itemKey?: (item: T, index: number) => string | number
    /** 滚动容器内部顶部/底部留白（px）：给首尾行的圆角卡片光晕留呼吸空间，虚拟滚动数学已含偏移 */
    padTop?: number
    padBottom?: number
  }>(),
  { buffer: 8, padTop: 0, padBottom: 0 },
)

const emit = defineEmits<{ nearEnd: []; scroll: [e: Event] }>()

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
      }
    })
    resizeObserver.observe(container.value)
  }
})
onBeforeUnmount(() => resizeObserver?.disconnect())

/**
 * nearEnd 边沿检测：进入底部 500px 区域只发射一次，离开后重新武装。
 * 不做边沿的话滚动期间每个 scroll 事件都会触发追加加载，在途请求被第二次
 * page++ 顶掉后中间整页会被静默跳过。
 */
let nearEndArmed = true

function onScroll(e: Event) {
  if (!container.value) return
  scrollTop.value = container.value.scrollTop
  const el = container.value
  const near = el.scrollHeight - el.scrollTop - el.clientHeight < 500
  if (near && nearEndArmed) {
    nearEndArmed = false
    emit('nearEnd')
  } else if (!near) {
    nearEndArmed = true
  }
  emit('scroll', e)
}

const totalHeight = computed(() => props.items.length * props.itemHeight)
const start = computed(() => Math.max(0, Math.floor((scrollTop.value - props.padTop) / props.itemHeight) - props.buffer))
const end = computed(() =>
  Math.min(
    props.items.length,
    Math.ceil((scrollTop.value - props.padTop + viewportHeight.value) / props.itemHeight) + props.buffer,
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
function scrollToIndex(index: number, align: 'top' | 'center' = 'center', behavior: ScrollBehavior = 'smooth') {
  if (!container.value) return
  const target = props.padTop + index * props.itemHeight
  const top =
    align === 'center'
      ? target - container.value.clientHeight / 2 + props.itemHeight / 2
      : target
  container.value.scrollTo({ top: Math.max(0, top), behavior })
}

/** 指定行当前是否完整落在可视区内（±1px 容差）：
 *  切歌自动跟随的判据——可见就不动，不可见才滚（纯 scrollTop 数学，不依赖行是否已渲染） */
function isIndexVisible(index: number): boolean {
  if (!container.value) return false
  const top = props.padTop + index * props.itemHeight
  const st = container.value.scrollTop
  return top >= st - 1 && top + props.itemHeight <= st + container.value.clientHeight + 1
}
defineExpose({ scrollToTop, scrollToIndex, isIndexVisible, getScrollEl: () => container.value })
</script>

<template>
  <div
    ref="container"
    class="h-full overflow-y-auto"
    :style="{ paddingTop: padTop + 'px', paddingBottom: padBottom + 'px', scrollbarGutter: 'stable' }"
    @scroll.passive="onScroll"
  >
    <div :style="{ height: totalHeight + padTop + padBottom + 'px', position: 'relative' }">
      <div :style="{ position: 'absolute', top: start * itemHeight + padTop + 'px', left: 0, right: 0 }">
        <div v-for="entry in visible" :key="keyOf(entry.item, entry.index)" :style="{ height: itemHeight + 'px' }">
          <slot :item="entry.item" :index="entry.index" />
        </div>
      </div>
    </div>
  </div>
</template>
