<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref, watch } from 'vue'
import { CloseIcon as X } from '@solar-icons/vue/linear/close'
import { dialogOverlayClass, dialogDraggable } from '@/composables/useDialogPrefs'

const props = withDefaults(
  defineProps<{
    open: boolean
    title?: string
    closable?: boolean
    size?: 'sm' | 'md' | 'lg'
  }>(),
  {
    closable: true,
    size: 'md',
  },
)

const emit = defineEmits<{ close: []; 'update:open': [value: boolean] }>()

/** 统一关闭入口：同时发出 close 与 v-model:open 的更新，两种用法都能收到 */
function close() {
  emit('close')
  emit('update:open', false)
}

const sizeClasses = {
  sm: 'max-w-sm',
  md: 'max-w-md',
  lg: 'max-w-lg',
}

function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape' && props.open) close()
}

onMounted(() => window.addEventListener('keydown', onKey))
onBeforeUnmount(() => window.removeEventListener('keydown', onKey))

// 每次打开恢复居中：清掉上次拖动留下的定位样式
const panelEl = ref<HTMLElement | null>(null)
watch(
  () => props.open,
  (v) => {
    if (v && panelEl.value) {
      panelEl.value.style.position = ''
      panelEl.value.style.left = ''
      panelEl.value.style.top = ''
      panelEl.value.style.margin = ''
      panelEl.value.style.width = ''
    }
  },
)
</script>

<template>
  <Teleport to="body">
    <Transition
      enter-active-class="transition duration-200 ease-out"
      enter-from-class="opacity-0"
      enter-to-class="opacity-100"
      leave-active-class="transition duration-150 ease-in"
      leave-from-class="opacity-100"
      leave-to-class="opacity-0"
    >
      <div v-if="open" :class="dialogOverlayClass()" @click.self="closable && close()">
        <Transition
          enter-active-class="transition duration-200 ease-out"
          enter-from-class="opacity-0 scale-95"
          enter-to-class="opacity-100 scale-100"
          leave-active-class="transition duration-150 ease-in"
          leave-from-class="opacity-100 scale-100"
          leave-to-class="opacity-0 scale-95"
        >
          <div
            v-if="open"
            ref="panelEl"
            data-dialog-panel
            class="app-surface-blur w-full rounded-2xl border border-white/15 bg-(--app-surface) shadow-2xl"
            :class="sizeClasses[size]"
          >
            <div
              v-if="title || closable"
              v-drag-dialog
              class="flex items-center justify-between border-b border-zinc-100 px-5 py-4 dark:border-zinc-700"
              :class="dialogDraggable() ? 'cursor-move select-none' : ''"
            >
              <h2 v-if="title" class="text-base font-semibold text-zinc-900 dark:text-zinc-50">
                {{ title }}
              </h2>
              <button
                v-if="closable"
                data-no-drag
                class="cursor-pointer rounded-lg p-1 text-zinc-400 transition hover:bg-zinc-100 hover:text-zinc-600 dark:hover:bg-zinc-700 dark:hover:text-zinc-200"
                @click="close()"
              >
                <X class="h-4 w-4" />
              </button>
            </div>
            <div class="p-5">
              <slot />
            </div>
            <div v-if="$slots.footer" class="flex justify-end gap-2 border-t border-zinc-100 px-5 py-4 dark:border-zinc-700">
              <slot name="footer" />
            </div>
          </div>
        </Transition>
      </div>
    </Transition>
  </Teleport>
</template>
