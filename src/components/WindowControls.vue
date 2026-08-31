<script setup lang="ts">
/// Windows/Linux 无边框窗口的自绘窗口控制按钮（macOS 用原生红绿灯，不渲染本组件）
/// ambient=true 时用于深色环境（如播放页），文字/悬停改用白色系
import { onMounted, onUnmounted, ref } from 'vue'
import { Copy, Minus, Square, X } from '@lucide/vue'
import { getCurrentWindow } from '@tauri-apps/api/window'

defineProps<{ ambient?: boolean }>()

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
      class="group flex w-11 items-center justify-center transition"
      title="最小化"
      @click="appWindow.minimize()"
    >
      <span
        class="flex h-7 w-8 items-center justify-center rounded-md transition"
        :class="
          ambient
            ? 'text-white/70 group-hover:bg-white/10 group-hover:text-white'
            : 'text-zinc-500 group-hover:bg-zinc-200/80 dark:text-zinc-400 dark:group-hover:bg-zinc-700/80'
        "
      >
        <Minus class="h-4 w-4" />
      </span>
    </button>
    <button
      class="group flex w-11 items-center justify-center transition"
      :title="maximized ? '还原' : '最大化'"
      @click="appWindow.toggleMaximize()"
    >
      <span
        class="flex h-7 w-8 items-center justify-center rounded-md transition"
        :class="
          ambient
            ? 'text-white/70 group-hover:bg-white/10 group-hover:text-white'
            : 'text-zinc-500 group-hover:bg-zinc-200/80 dark:text-zinc-400 dark:group-hover:bg-zinc-700/80'
        "
      >
        <Copy v-if="maximized" class="h-3.5 w-3.5" />
        <Square v-else class="h-3 w-3" />
      </span>
    </button>
    <button
      class="group flex w-11 items-center justify-center transition"
      title="关闭"
      @click="appWindow.close()"
    >
      <span
        class="flex h-7 w-8 items-center justify-center rounded-md transition"
        :class="
          ambient
            ? 'text-white/70 group-hover:bg-white/10 group-hover:text-white'
            : 'text-zinc-500 group-hover:bg-red-500 group-hover:text-white dark:text-zinc-400'
        "
      >
        <X class="h-4 w-4" />
      </span>
    </button>
  </div>
</template>

