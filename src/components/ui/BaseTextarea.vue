<script setup lang="ts">
import { useId } from 'vue'

const props = withDefaults(
  defineProps<{
    modelValue: string
    placeholder?: string
    disabled?: boolean
    readonly?: boolean
    error?: string
    hint?: string
    rows?: number
    resize?: boolean
    maxlength?: number
  }>(),
  {
    disabled: false,
    readonly: false,
    rows: 3,
    resize: false,
  },
)

const emit = defineEmits<{ 'update:modelValue': [value: string] }>()

const id = useId()

function onInput(e: Event) {
  emit('update:modelValue', (e.target as HTMLTextAreaElement).value)
}
</script>

<template>
  <div class="w-full">
    <div
      class="relative rounded-lg border transition-all duration-150"
      :class="[
        error
          ? 'border-red-400 bg-red-50/50 dark:border-red-500/50 dark:bg-red-500/5'
          : 'border-zinc-200 bg-white focus-within:border-violet-500 focus-within:ring-2 focus-within:ring-violet-500/10 dark:border-zinc-700 dark:bg-zinc-800 dark:focus-within:border-violet-500',
        // 悬停主色描边：错误态（红）与禁用态不参与，见 style.css
        !error && !disabled ? 'hover-accent-border' : '',
        disabled ? 'opacity-50 cursor-not-allowed' : '',
      ]"
    >
      <textarea
        :id="id"
        :value="modelValue"
        :placeholder="placeholder"
        :disabled="disabled"
        :readonly="readonly"
        :rows="rows"
        :maxlength="maxlength"
        class="w-full bg-transparent px-3.5 py-2 text-sm text-zinc-900 outline-none placeholder:text-zinc-400 dark:text-zinc-100"
        :class="[resize ? 'resize-y' : 'resize-none']"
        @input="onInput"
      />
    </div>
    <p v-if="error" class="mt-1 text-xs text-red-500">{{ error }}</p>
    <p v-else-if="hint" class="mt-1 text-xs text-zinc-400">{{ hint }}</p>
  </div>
</template>
