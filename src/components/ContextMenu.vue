<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { AltArrowRightIcon as ChevronRight } from '@solar-icons/vue/linear/alt-arrow-right'
import type { Component } from 'vue'

export interface MenuItem {
  label: string
  icon?: Component
  /** 图标附加色类（如喜欢项固定红心：text-red-500）；缺省跟随菜单文字色 */
  iconClass?: string
  danger?: boolean
  disabled?: boolean
  action?: () => void
  children?: MenuItem[]
}

const props = defineProps<{ x: number; y: number; items: MenuItem[] }>()
const emit = defineEmits<{ close: [] }>()

const el = ref<HTMLElement | null>(null)

let clampRaf = 0
watch(
  () => [props.x, props.y],
  () => {
    // 视口边缘保护
    if (clampRaf) cancelAnimationFrame(clampRaf)
    clampRaf = requestAnimationFrame(() => {
      clampRaf = 0
      if (!el.value) return
      const rect = el.value.getBoundingClientRect()
      if (rect.right > window.innerWidth - 8) el.value.style.left = `${window.innerWidth - rect.width - 8}px`
      if (rect.bottom > window.innerHeight - 8) el.value.style.top = `${window.innerHeight - rect.height - 8}px`
    })
  },
  { immediate: true },
)

function onDocClick(e: MouseEvent) {
  if (el.value && !el.value.contains(e.target as Node)) emit('close')
}
function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') emit('close')
}
onMounted(() => {
  window.addEventListener('click', onDocClick, true)
  window.addEventListener('contextmenu', onDocClick, true)
  window.addEventListener('keydown', onKey)
})
onBeforeUnmount(() => {
  if (clampRaf) cancelAnimationFrame(clampRaf)
  window.removeEventListener('click', onDocClick, true)
  window.removeEventListener('contextmenu', onDocClick, true)
  window.removeEventListener('keydown', onKey)
})

function run(item: MenuItem) {
  if (item.disabled || item.children?.length) return
  item.action?.()
  emit('close')
}
</script>

<template>
  <Teleport to="body">
    <div
      ref="el"
      class="app-surface-blur fixed z-50 min-w-44 rounded-lg border border-white/15 bg-(--app-surface) py-1 shadow-xl"
      :style="{ left: x + 'px', top: y + 'px' }"
    >
      <template v-for="(item, i) in items" :key="i">
        <!-- 子菜单 -->
        <div v-if="item.children?.length" class="group relative">
          <button
            class="transition-colors duration-150 flex w-full cursor-pointer items-center gap-2.5 px-3 py-1.5 text-left text-sm text-zinc-700 hover:bg-violet-500/10 disabled:opacity-40 dark:text-zinc-200"
            @click.stop
          >
            <component :is="item.icon" v-if="item.icon" class="h-4 w-4 opacity-70" :class="item.iconClass" />
            <span class="flex-1">{{ item.label }}</span>
            <ChevronRight class="h-3.5 w-3.5 opacity-50" />
          </button>
          <div
            class="absolute top-0 left-full z-10 ml-0.5 hidden min-w-40 rounded-lg border border-zinc-200 bg-white py-1 shadow-xl group-hover:block dark:border-zinc-700 dark:bg-zinc-800"
          >
            <button
              v-for="(child, j) in item.children"
              :key="j"
              class="transition-colors duration-150 flex w-full max-w-56 cursor-pointer items-center px-3 py-1.5 text-left text-sm text-zinc-700 hover:bg-violet-500/10 disabled:opacity-40 dark:text-zinc-200"
              :disabled="child.disabled"
              @click.stop="run(child)"
            >
              <span class="truncate">{{ child.label }}</span>
            </button>
          </div>
        </div>
        <!-- 普通项。⚠️ 危险项的 text/hover 色必须与默认色互斥地写进 :class——
             若与静态 text-zinc-700/hover:bg-violet-500/10 并存，同特异性下
             按样式表顺序后者（zinc/violet）会压过 red，danger 红色静默失效 -->
        <button
          v-else
          class="transition-colors duration-150 flex w-full cursor-pointer items-center gap-2.5 px-3 py-1.5 text-left text-sm disabled:opacity-40"
          :class="item.danger
            ? 'text-red-600 hover:bg-red-500/10 dark:text-red-400'
            : 'text-zinc-700 hover:bg-violet-500/10 dark:text-zinc-200'"
          :disabled="item.disabled"
          @click.stop="run(item)"
        >
          <component :is="item.icon" v-if="item.icon" class="h-4 w-4 opacity-70" :class="item.iconClass" />
          {{ item.label }}
        </button>
      </template>
    </div>
  </Teleport>
</template>
