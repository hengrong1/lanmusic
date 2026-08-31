<script setup lang="ts">
import { ref, watch } from 'vue'
import { ArrowLeft, Moon, Search, Sun, X } from 'lucide-vue-next'
import { useNav } from '@/composables/useNav'
import { useTheme } from '@/composables/useTheme'

const { current, back, replaceSearch, canBack } = useNav()
const { mode, resolved, setTheme } = useTheme()

const input = ref(current.value.search ?? '')
watch(
  () => current.value.search,
  (s) => (input.value = s ?? ''),
)

let timer: ReturnType<typeof setTimeout> | undefined
function onInput() {
  clearTimeout(timer)
  timer = setTimeout(() => replaceSearch(input.value.trim()), 300)
}

function clearSearch() {
  input.value = ''
  replaceSearch('')
}

function cycleTheme() {
  const order = ['dark', 'light', 'system'] as const
  const i = order.indexOf(mode.value as (typeof order)[number])
  setTheme(order[(i + 1) % order.length])
}

function focusSearch() {
  document.getElementById('search-input')?.focus()
}
defineExpose({ focusSearch })
</script>

<template>
  <header class="flex h-14 shrink-0 items-center gap-3 border-b border-zinc-200 px-4 dark:border-zinc-800">
    <button
      class="flex h-8 w-8 items-center justify-center rounded-full text-zinc-500 transition hover:bg-zinc-100 dark:hover:bg-zinc-800 disabled:opacity-30 dark:text-zinc-400"
      :disabled="!canBack"
      title="返回"
      @click="back()"
    >
      <ArrowLeft class="h-4 w-4" />
    </button>

    <div class="relative mx-auto w-full max-w-md">
      <Search class="pointer-events-none absolute top-1/2 left-3 h-4 w-4 -translate-y-1/2 text-zinc-400" />
      <input
        id="search-input"
        v-model="input"
        class="h-9 w-full rounded-full border border-transparent bg-zinc-100 pr-8 pl-9 text-sm text-zinc-800 outline-none transition placeholder:text-zinc-400 focus:border-violet-400 focus:bg-white dark:bg-zinc-800/70 dark:text-zinc-100 dark:focus:bg-zinc-800"
        placeholder="搜索歌曲、艺人、专辑 (Ctrl+F)"
        @input="onInput"
        @keydown.esc="clearSearch"
      />
      <button
        v-if="input"
        class="absolute top-1/2 right-2 flex h-5 w-5 -translate-y-1/2 items-center justify-center rounded-full text-zinc-400 hover:bg-zinc-200 hover:text-zinc-600 dark:hover:bg-zinc-700"
        @click="clearSearch"
      >
        <X class="h-3 w-3" />
      </button>
    </div>

    <button
      class="flex h-8 w-8 items-center justify-center rounded-full text-zinc-500 transition hover:bg-zinc-100 dark:text-zinc-400 dark:hover:bg-zinc-800"
      :title="`主题：${mode === 'dark' ? '深色' : mode === 'light' ? '浅色' : '跟随系统'}`"
      @click="cycleTheme"
    >
      <Sun v-if="resolved === 'dark'" class="h-4 w-4" />
      <Moon v-else class="h-4 w-4" />
    </button>
  </header>
</template>
