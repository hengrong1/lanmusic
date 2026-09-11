<script setup lang="ts">
const props = withDefaults(
  defineProps<{
    modelValue: string
    disabled?: boolean
    title?: string
  }>(),
  {
    disabled: false,
  },
)

const emit = defineEmits<{ 'update:modelValue': [value: string] }>()

function onInput(e: Event) {
  emit('update:modelValue', (e.target as HTMLInputElement).value)
}
</script>

<template>
  <label
    class="relative inline-flex h-8 w-14 shrink-0 cursor-pointer items-center justify-center overflow-hidden rounded-lg border border-zinc-200 bg-white transition-all duration-150 hover:border-violet-400 hover:ring-2 hover:ring-violet-500/10 focus-within:border-violet-500 focus-within:ring-2 focus-within:ring-violet-500/10 dark:border-zinc-700 dark:bg-zinc-800"
    :class="{ 'cursor-not-allowed opacity-40': disabled }"
    :title="title"
  >
    <span class="pointer-events-none absolute inset-1 rounded-md shadow-inner" :style="{ background: modelValue }" />
    <input
      type="color"
      :value="modelValue"
      :disabled="disabled"
      class="absolute inset-0 h-full w-full cursor-pointer opacity-0 disabled:cursor-not-allowed"
      @input="onInput"
    />
  </label>
</template>
