<script setup lang="ts">
import { onBeforeUnmount, onMounted } from 'vue'
import { TriangleAlert } from '@lucide/vue'
import { useConfirmState } from '@/composables/useConfirm'

const { state, answer } = useConfirmState()

function onKey(e: KeyboardEvent) {
  if (!state.value.open) return
  if (e.key === 'Escape') answer(false)
  else if (e.key === 'Enter') answer(true)
}
onMounted(() => window.addEventListener('keydown', onKey))
onBeforeUnmount(() => window.removeEventListener('keydown', onKey))
</script>

<template>
  <Teleport to="body">
    <Transition
      enter-active-class="transition duration-150 ease-out"
      enter-from-class="opacity-0"
      enter-to-class="opacity-100"
      leave-active-class="transition duration-100 ease-in"
      leave-to-class="opacity-0"
    >
      <div
        v-if="state.open"
        class="fixed inset-0 z-[70] flex items-center justify-center bg-black/40 backdrop-blur-sm"
        @click.self="answer(false)"
      >
        <div class="w-[380px] rounded-2xl border border-zinc-200 bg-white p-5 shadow-2xl dark:border-zinc-700 dark:bg-zinc-800">
          <div class="flex items-start gap-3">
            <div
              v-if="state.danger"
              class="mt-0.5 flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-red-100 text-red-500 dark:bg-red-500/15"
            >
              <TriangleAlert class="h-4.5 w-4.5" />
            </div>
            <div class="min-w-0">
              <h2 class="text-base font-semibold text-zinc-900 dark:text-zinc-50">{{ state.title }}</h2>
              <p class="mt-1.5 whitespace-pre-line text-sm leading-relaxed text-zinc-500 dark:text-zinc-300">
                {{ state.message }}
              </p>
            </div>
          </div>
          <div class="mt-5 flex justify-end gap-2">
            <button
              class="rounded-full px-4 py-1.5 text-sm text-zinc-600 transition hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-700"
              @click="answer(false)"
            >
              {{ state.cancelText }}
            </button>
            <button
              class="rounded-full px-4 py-1.5 text-sm font-medium text-white transition hover:brightness-110"
              :class="state.danger ? 'bg-red-500' : 'bg-violet-500'"
              @click="answer(true)"
            >
              {{ state.confirmText }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
