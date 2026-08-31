<script setup lang="ts">
/// Windows/Linux 无边框窗口的自绘窗口控制按钮（macOS 用原生红绿灯，不渲染本组件）
import { onMounted, onUnmounted, ref } from 'vue'
import { Copy, Minus, Square, X } from '@lucide/vue'
import { getCurrentWindow } from '@tauri-apps/api/window'

const appWindow = getCurrentWindow()
const maximized = ref(false)

let unlisten: (() => void) | undefined
onMounted(async () => {
  maximized.value = await appWindow.isMaximized()
  unlisten = await appWindow.onResized(async () => {
    maximized.value = await appWindow.isMaximized()
  })
})
onUnmounted(() => unlisten?.())
</script>

<template>
  <div class="flex h-full items-stretch">
    <button
      class="flex w-11 items-center justify-center text-zinc-500 transition hover:bg-zinc-200/80 dark:text-zinc-400 dark:hover:bg-zinc-700/80"
      title="最小化"
      @click="appWindow.minimize()"
    >
      <Minus class="h-4 w-4" />
    </button>
    <button
      class="flex w-11 items-center justify-center text-zinc-500 transition hover:bg-zinc-200/80 dark:text-zinc-400 dark:hover:bg-zinc-700/80"
      :title="maximized ? '还原' : '最大化'"
      @click="appWindow.toggleMaximize()"
    >
      <Copy v-if="maximized" class="h-3.5 w-3.5" />
      <Square v-else class="h-3 w-3" />
    </button>
    <button
      class="flex w-11 items-center justify-center text-zinc-500 transition hover:bg-red-500 hover:text-white dark:text-zinc-400"
      title="关闭"
      @click="appWindow.close()"
    >
      <X class="h-4 w-4" />
    </button>
  </div>
</template>