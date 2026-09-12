<script setup lang="ts">
/// Windows/Linux 无边框窗口的自绘窗口控制按钮（macOS 用原生红绿灯，不渲染本组件）
/// ambient=true 时用于深色环境（如播放页），文字/悬停改用白色系
import { onMounted, onUnmounted, ref } from 'vue'
import { FullScreenIcon as Maximize } from '@solar-icons/vue/linear/full-screen'
import { MinusIcon as Minus } from '@solar-icons/vue/linear/minus'
import { QuitFullScreenIcon as Minimize } from '@solar-icons/vue/linear/quit-full-screen'
import { CloseIcon as X } from '@solar-icons/vue/linear/close'
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
      class="group flex w-11 cursor-pointer items-center justify-center transition"
      v-tooltip="$t('common.minimize')"
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
      class="group flex w-11 cursor-pointer items-center justify-center transition"
      v-tooltip="maximized ? $t('common.restore') : $t('common.maximize')"
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
        <Minimize v-if="maximized" class="h-4 w-4" />
        <Maximize v-else class="h-4 w-4" />
      </span>
    </button>
    <button
      class="group flex w-11 cursor-pointer items-center justify-center transition"
      v-tooltip="$t('common.close')"
      @click="appWindow.close()"
    >
      <span
        class="flex h-7 w-8 items-center justify-center rounded-md transition"
        :class="
          ambient
            ? 'text-white/70 group-hover:bg-red-500 group-hover:text-white'
            : 'text-zinc-500 group-hover:bg-red-500 group-hover:text-white dark:text-zinc-400'
        "
      >
        <X class="h-4 w-4" />
      </span>
    </button>
  </div>
</template>

