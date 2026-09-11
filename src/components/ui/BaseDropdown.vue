<script setup lang="ts">
import { ref, nextTick, onMounted, onBeforeUnmount, watch } from 'vue'
import type { Component } from 'vue'

export interface DropdownItem {
  label: string
  icon?: Component
  danger?: boolean
  disabled?: boolean
  action?: () => void
}

const props = withDefaults(
  defineProps<{
    items: DropdownItem[]
    placement?: 'bottom-left' | 'bottom-right'
  }>(),
  {
    placement: 'bottom-left',
  },
)

const open = ref(false)
const wrap = ref<HTMLElement | null>(null)

/** 可聚焦的菜单项（跳过 disabled），用于方向键在菜单项间移动焦点 */
function menuItems(): HTMLButtonElement[] {
  const all = wrap.value?.querySelectorAll<HTMLButtonElement>('[data-menu-item]')
  return all ? Array.from(all).filter((b) => !b.disabled) : []
}

/** 触发器：插槽内的第一个可聚焦元素（通常是调用方传入的按钮） */
function triggerEl(): HTMLElement | null {
  return wrap.value?.querySelector<HTMLElement>('button, [tabindex]') ?? null
}

// 展开后聚焦第一个菜单项，方向键/回车即可直接操作
watch(open, async (v) => {
  if (!v) return
  await nextTick()
  menuItems()[0]?.focus()
})

function close(restoreFocus = false) {
  if (!open.value) return
  open.value = false
  if (restoreFocus) triggerEl()?.focus()
}

function run(item: DropdownItem) {
  if (item.disabled) return
  item.action?.()
  close(true)
}

/** 键盘交互：Escape 关闭并还焦；方向键/Home/End 在菜单项间移动；Tab 关闭放行 */
function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && open.value) {
    e.preventDefault()
    e.stopPropagation()
    close(true)
    return
  }
  if (!open.value) {
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      open.value = true
    }
    return
  }
  const list = menuItems()
  if (!list.length) return
  const cur = list.indexOf(document.activeElement as HTMLButtonElement)
  switch (e.key) {
    case 'ArrowDown':
      e.preventDefault()
      list[(cur + 1) % list.length]?.focus()
      break
    case 'ArrowUp':
      e.preventDefault()
      list[(cur - 1 + list.length) % list.length]?.focus()
      break
    case 'Home':
      e.preventDefault()
      list[0]?.focus()
      break
    case 'End':
      e.preventDefault()
      list[list.length - 1]?.focus()
      break
    case 'Tab':
      close(false)
      break
  }
}

function onDocClick(e: MouseEvent) {
  if (wrap.value && !wrap.value.contains(e.target as Node)) {
    open.value = false
  }
}

onMounted(() => document.addEventListener('click', onDocClick))
onBeforeUnmount(() => document.removeEventListener('click', onDocClick))
</script>

<template>
  <div ref="wrap" class="relative inline-block" @keydown="onKeydown">
    <div aria-haspopup="menu" :aria-expanded="open" @click="open = !open">
      <slot />
    </div>

    <Transition
      enter-active-class="transition duration-150 ease-out"
      enter-from-class="opacity-0 -translate-y-1"
      enter-to-class="opacity-100 translate-y-0"
      leave-active-class="transition duration-100 ease-in"
      leave-from-class="opacity-100"
      leave-to-class="opacity-0"
    >
      <div
        v-if="open"
        role="menu"
        class="absolute z-50 min-w-40 rounded-lg border border-zinc-200 bg-white py-1 shadow-xl dark:border-zinc-700 dark:bg-zinc-800 max-h-60 overflow-y-auto"
        :class="placement === 'bottom-right' ? 'right-0' : 'left-0'"
        style="margin-top: 4px"
      >
        <button
          v-for="(item, i) in items"
          :key="i"
          type="button"
          role="menuitem"
          :data-menu-item="i"
          class="flex w-full cursor-pointer items-center gap-2.5 px-3 py-2 text-left text-sm transition-colors focus:outline-none focus-visible:bg-zinc-100 dark:focus-visible:bg-zinc-700/60"
          :class="[
            item.danger
              ? 'text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-500/10'
              : 'text-zinc-700 hover:bg-zinc-50 dark:text-zinc-200 dark:hover:bg-zinc-700/50',
            item.disabled ? 'opacity-40 cursor-not-allowed' : '',
          ]"
          :disabled="item.disabled"
          @click.stop="run(item)"
        >
          <component v-if="item.icon" :is="item.icon" class="h-4 w-4 opacity-70" />
          {{ item.label }}
        </button>
      </div>
    </Transition>
  </div>
</template>
