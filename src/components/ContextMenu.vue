<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
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

/** placement='top'：菜单显示在 y 点上方（底边距 y 8px），供底部操作栏向上弹出 */
const props = defineProps<{ x: number; y: number; placement?: 'top'; items: MenuItem[] }>()
const emit = defineEmits<{ close: [] }>()

const el = ref<HTMLElement | null>(null)

let clampRaf = 0
watch(
  () => [props.x, props.y, props.placement],
  () => {
    // 视口边缘保护（下一帧测量，此时 DOM 已挂载）
    if (clampRaf) cancelAnimationFrame(clampRaf)
    clampRaf = requestAnimationFrame(() => {
      clampRaf = 0
      if (!el.value) return
      const rect = el.value.getBoundingClientRect()
      let left = props.x
      if (left + rect.width > window.innerWidth - 8) left = window.innerWidth - rect.width - 8
      if (left < 8) left = 8
      // 'top' 从 y 点向上展开（底边距 y 8px），默认从 y 点向下展开；越界一律收回视口内
      let top = props.placement === 'top' ? props.y - rect.height - 8 : props.y
      if (top + rect.height > window.innerHeight - 8) top = window.innerHeight - rect.height - 8
      if (top < 8) top = 8
      el.value.style.left = `${left}px`
      el.value.style.top = `${top}px`
    })
  },
  { immediate: true },
)

// ---- 二级菜单：必须 Teleport 到 body。主菜单根的 backdrop-filter 会形成 backdrop root，
// 把内部后代的 backdrop 采样隔离在根内（根底透明）——子菜单留在 DOM 里就算挂上
// app-surface-blur 玻璃也采不到底图，照样透背景（实测）。Teleport 出来后玻璃才真生效，
// 且白字映射（html.has-bg .app-surface-blur .text-zinc-…）依旧命中：按钮仍是子菜单根的后代。
const subEl = ref<HTMLElement | null>(null)
/** 当前展开的子菜单：项下标 + 父项几何（翻转定位用）。null = 收起 */
const subOpen = ref<{ index: number; hostLeft: number; hostRight: number } | null>(null)
const subPos = ref({ x: 0, y: 0 })
const subChildren = computed(() => {
  const open = subOpen.value
  if (!open) return []
  return props.items[open.index]?.children ?? []
})

let subRaf = 0
let subCloseTimer = 0

function openSubmenu(index: number, e: MouseEvent) {
  window.clearTimeout(subCloseTimer)
  const host = (e.currentTarget as HTMLElement).getBoundingClientRect()
  subOpen.value = { index, hostLeft: host.left, hostRight: host.right }
  // 初始贴父项右上角（对应旧版 top-0 left-full + ml-0.5），下一帧测量后修正翻转/越界
  subPos.value = { x: host.right + 2, y: host.top - 4 }
  placeSubmenu()
}
function placeSubmenu() {
  if (subRaf) cancelAnimationFrame(subRaf)
  subRaf = requestAnimationFrame(() => {
    subRaf = 0
    if (!subEl.value || !subOpen.value) return
    const rect = subEl.value.getBoundingClientRect()
    let x = subPos.value.x
    // 右侧放不下 → 翻到父项左侧
    if (x + rect.width > window.innerWidth - 8) {
      x = Math.max(8, subOpen.value.hostLeft - rect.width - 2)
    }
    let y = subPos.value.y
    if (y + rect.height > window.innerHeight - 8) y = window.innerHeight - rect.height - 8
    if (y < 8) y = 8
    subEl.value.style.left = `${x}px`
    subEl.value.style.top = `${y}px`
  })
}
/** 父项/子菜单 mouseleave：延迟收起，给「穿过 2px 间隙」留时间 */
function scheduleSubClose() {
  window.clearTimeout(subCloseTimer)
  subCloseTimer = window.setTimeout(() => {
    subOpen.value = null
  }, 120)
}
function keepSubOpen() {
  window.clearTimeout(subCloseTimer)
}

function onDocClick(e: MouseEvent) {
  const target = e.target as Node
  // 点在子菜单（Teleport 到 body，不在主菜单根内）也不关，与旧行为一致
  if (subEl.value?.contains(target)) return
  if (el.value && !el.value.contains(target)) emit('close')
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
  window.clearTimeout(subCloseTimer)
  if (subRaf) cancelAnimationFrame(subRaf)
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
    <Transition
      appear
      enter-active-class="transition duration-150 ease-out"
      enter-from-class="opacity-0 scale-95"
      leave-active-class="transition duration-100 ease-in"
      leave-to-class="opacity-0 scale-95"
    >
      <div
        ref="el"
        class="app-surface-blur fixed z-50 min-w-44 rounded-lg border border-white/15 bg-(--app-surface) py-1 shadow-xl"
        :class="placement === 'top' ? 'origin-bottom-left' : 'origin-top-left'"
        :style="{ left: x + 'px', top: y + 'px' }"
      >
        <template v-for="(item, i) in items" :key="i">
          <!-- 有子菜单的项：hover 开子菜单（JS 管理，子菜单 Teleport 在下方） -->
          <button
            v-if="item.children?.length"
            class="transition-colors duration-150 flex w-full cursor-pointer items-center gap-2.5 px-3 py-1.5 text-left text-sm text-zinc-700 hover:bg-violet-500/10 disabled:opacity-40 dark:text-zinc-200"
            @mouseenter="openSubmenu(i, $event)"
            @mouseleave="scheduleSubClose"
            @click.stop
          >
            <component :is="item.icon" v-if="item.icon" class="h-4 w-4 opacity-70" :class="item.iconClass" />
            <span class="flex-1">{{ item.label }}</span>
            <ChevronRight class="h-3.5 w-3.5 opacity-50" />
          </button>
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
    </Transition>

    <!-- 二级菜单（Teleport 到 body，见 script 内说明）：与主菜单同款深玻璃 -->
    <Transition
      appear
      enter-active-class="transition duration-150 ease-out"
      enter-from-class="opacity-0 scale-95"
      leave-active-class="transition duration-100 ease-in"
      leave-to-class="opacity-0 scale-95"
    >
      <div
        v-if="subOpen"
        ref="subEl"
        class="app-surface-blur fixed z-50 min-w-40 rounded-lg border border-white/15 bg-(--app-surface) py-1 shadow-xl origin-top-left"
        :style="{ left: subPos.x + 'px', top: subPos.y + 'px' }"
        @mouseenter="keepSubOpen"
        @mouseleave="scheduleSubClose"
      >
        <button
          v-for="(child, j) in subChildren"
          :key="j"
          class="transition-colors duration-150 flex w-full max-w-56 cursor-pointer items-center px-3 py-1.5 text-left text-sm text-zinc-700 hover:bg-violet-500/10 disabled:opacity-40 dark:text-zinc-200"
          :disabled="child.disabled"
          @click.stop="run(child)"
        >
          <span class="truncate">{{ child.label }}</span>
        </button>
      </div>
    </Transition>
  </Teleport>
</template>
