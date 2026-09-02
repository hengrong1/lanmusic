<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { ChevronRight } from '@lucide/vue'
import type { Component } from 'vue'

export interface MenuItem {
  label: string
  icon?: Component
  danger?: boolean
  disabled?: boolean
  action?: () => void
  children?: MenuItem[]
}

const props = defineProps<{ x: number; y: number; items: MenuItem[] }>()
const emit = defineEmits<{ close: [] }>()

const el = ref<HTMLElement | null>(null)

watch(
  () => [props.x, props.y],
  () => {
    // 视口边缘保护
    requestAnimationFrame(() => {
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
      class="fixed z-50 min-w-44 rounded-lg border border-zinc-200 bg-white py-1 shadow-xl dark:border-zinc-700 dark:bg-zinc-800"
      :style="{ left: x + 'px', top: y + 'px' }"
    >
      <template v-for="(item, i) in items" :key="i">
        <!-- 子菜单 -->
        <div v-if="item.children?.length" class="group relative">
          <button
            class="flex w-full cursor-pointer items-center gap-2.5 px-3 py-1.5 text-left text-sm text-zinc-700 hover:bg-zinc-100 disabled:opacity-40 dark:text-zinc-200 dark:hover:bg-zinc-700/60"
            @click.stop
          >
            <component :is="item.icon" v-if="item.icon" class="h-4 w-4 opacity-70" />
            <span class="flex-1">{{ item.label }}</span>
            <ChevronRight class="h-3.5 w-3.5 opacity-50" />
          </button>
          <div
            class="absolute top-0 left-full z-10 ml-0.5 hidden min-w-40 rounded-lg border border-zinc-200 bg-white py-1 shadow-xl group-hover:block dark:border-zinc-700 dark:bg-zinc-800"
          >
            <button
              v-for="(child, j) in item.children"
              :key="j"
              class="flex w-full max-w-56 cursor-pointer items-center px-3 py-1.5 text-left text-sm text-zinc-700 hover:bg-zinc-100 disabled:opacity-40 dark:text-zinc-200 dark:hover:bg-zinc-700/60"
              :disabled="child.disabled"
              @click.stop="run(child)"
            >
              <span class="truncate">{{ child.label }}</span>
            </button>
          </div>
        </div>
        <!-- 普通项 -->
        <button
          v-else
          class="flex w-full cursor-pointer items-center gap-2.5 px-3 py-1.5 text-left text-sm text-zinc-700 hover:bg-zinc-100 disabled:opacity-40 dark:text-zinc-200 dark:hover:bg-zinc-700/60"
          :class="item.danger ? 'text-red-600 dark:text-red-400' : ''"
          :disabled="item.disabled"
          @click.stop="run(item)"
        >
          <component :is="item.icon" v-if="item.icon" class="h-4 w-4 opacity-70" />
          {{ item.label }}
        </button>
      </template>
    </div>
  </Teleport>
</template>
