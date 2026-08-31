<script setup lang="ts">
import { CircleAlert, Info } from '@lucide/vue'
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
          class="pointer-events-auto flex max-w-md items-center gap-2 rounded-full border border-zinc-200 bg-white/98 px-4 py-2 text-sm shadow-lg dark:border-zinc-700 dark:bg-zinc-800/98 dark:text-zinc-100"
        >
          <CircleAlert v-if="t.kind === 'error'" class="h-4 w-4 shrink-0 text-red-500" />
          <Info v-else class="h-4 w-4 shrink-0 text-violet-500" />
          <span class="truncate">{{ t.text }}</span>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>
