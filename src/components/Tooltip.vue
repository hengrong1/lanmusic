<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from 'vue'

/**
 * 轻量 Tooltip：悬停后在元素旁弹出提示气泡。
 * - 气泡 Teleport 到 body 并用 fixed 定位，不受父级 overflow-hidden 裁切
 * - side="right" 在元素右侧垂直居中（适合收起的侧栏）
 * - disabled 为 true 时不显示（如侧栏展开态文字已可见）
 */
const props = withDefaults(
  defineProps<{
    text: string
    side?: 'right' | 'top'
    disabled?: boolean
    delay?: number
  }>(),
  { side: 'right', disabled: false, delay: 450 },
)

const wrap = ref<HTMLElement | null>(null)
const visible = ref(false)
const pos = ref({ top: 0, left: 0 })
let timer: number | null = null

function cancelTimer() {
  if (timer != null) {
    window.clearTimeout(timer)
    timer = null
  }
}

function show() {
  if (props.disabled || !props.text) return
  cancelTimer()
  timer = window.setTimeout(() => {
    const el = wrap.value
    if (!el) return
    const r = el.getBoundingClientRect()
    if (props.side === 'right') {
      pos.value = { top: r.top + r.height / 2, left: r.right + 8 }
    } else {
      pos.value = { top: r.top - 6, left: r.left + r.width / 2 }
    }
    visible.value = true
  }, props.delay)
}

function hide() {
  cancelTimer()
  visible.value = false
}

watch(
  () => props.disabled,
  (d) => {
    if (d) hide()
  },
)

onBeforeUnmount(() => {
  cancelTimer()
  visible.value = false
})
</script>

<template>
  <span ref="wrap" class="block" @mouseenter="show" @mouseleave="hide" @click="hide">
    <slot />
  </span>
  <Teleport to="body">
    <Transition name="tooltip-fade">
      <div
        v-if="visible"
        class="pointer-events-none fixed z-[9999] whitespace-nowrap rounded-md bg-zinc-800 px-2 py-1 text-xs leading-5 text-zinc-50 shadow-lg dark:bg-zinc-700"
        :class="side === 'right' ? '-translate-y-1/2' : '-translate-x-1/2 -translate-y-full'"
        :style="{ top: `${pos.top}px`, left: `${pos.left}px` }"
      >
        {{ text }}
        <span
          v-if="side === 'right'"
          class="absolute top-1/2 -left-1 h-2 w-2 -translate-y-1/2 rotate-45 rounded-[1px] bg-zinc-800 dark:bg-zinc-700"
        ></span>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.tooltip-fade-enter-active,
.tooltip-fade-leave-active {
  transition: opacity 0.15s ease-out;
}
.tooltip-fade-enter-from,
.tooltip-fade-leave-to {
  opacity: 0;
}
</style>
