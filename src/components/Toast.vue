<script setup lang="ts">
import { DangerCircleIcon as CircleAlert } from '@solar-icons/vue/linear/danger-circle'
import { InfoCircleIcon as Info } from '@solar-icons/vue/linear/info-circle'
import { useToast } from '@/composables/useToast'

const { toasts } = useToast()
</script>

<template>
  <Teleport to="body">
    <div class="pointer-events-none fixed bottom-24 left-1/2 z-[60] flex -translate-x-1/2 flex-col items-center gap-2">
      <TransitionGroup
        enter-active-class="transition duration-200 ease-out"
        enter-from-class="opacity-0 translate-y-2"
        leave-active-class="transition duration-150 ease-in"
        leave-to-class="opacity-0"
      >
        <div
          v-for="t in toasts"
          :key="t.id"
          class="app-float pointer-events-auto flex max-w-md items-start gap-2 rounded-2xl border border-white/15 bg-(--app-overlay) px-4 py-2 text-sm shadow-lg dark:border-zinc-700 dark:bg-zinc-800/98 dark:text-zinc-100"
        >
          <!-- 长文案（如带完整曲名的提示）换行完整显示：
               原来 truncate 会把尾部截成省略号，rounded-full 的胶囊圆弧还会啃掉换行后的首尾文字 -->
          <CircleAlert v-if="t.kind === 'error'" class="mt-0.5 h-4 w-4 shrink-0 text-red-500" />
          <Info v-else class="mt-0.5 h-4 w-4 shrink-0 text-violet-500" />
          <span class="min-w-0 break-words">{{ t.text }}</span>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>
