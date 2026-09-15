<script setup lang="ts">
import { onBeforeUnmount, onMounted } from 'vue'
import { DangerTriangleIcon as TriangleAlert } from '@solar-icons/vue/linear/danger-triangle'
import { BaseButton } from '@/components/ui'
import { useConfirmState } from '@/composables/useConfirm'
import { dialogOverlayClass, dialogPanelTransition, dialogDraggable } from '@/composables/useDialogPrefs'

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
      leave-from-class="opacity-100"
      leave-to-class="opacity-0"
    >
      <div v-if="state.open" :class="dialogOverlayClass()" @click.self="answer(false)">
        <Transition v-bind="dialogPanelTransition">
          <div
            v-if="state.open"
            v-focus-trap
            data-dialog-panel
            class="app-surface-blur w-[380px] rounded-2xl border border-white/15 bg-(--app-surface) p-5 shadow-2xl"
          >
            <div
              v-drag-dialog
              class="flex items-start gap-3"
              :class="dialogDraggable() ? 'cursor-move select-none' : ''"
            >
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
              <BaseButton variant="ghost" size="sm" @click="answer(false)">
                {{ state.cancelText }}
              </BaseButton>
              <BaseButton
                size="sm"
                variant="primary"
                :tone="state.danger ? 'danger' : 'default'"
                @click="answer(true)"
              >
                {{ state.confirmText }}
              </BaseButton>
            </div>
          </div>
        </Transition>
      </div>
    </Transition>
  </Teleport>
</template>
