<script setup lang="ts">
import CheckboxIndicator from './CheckboxIndicator.vue'

const props = withDefaults(
  defineProps<{
    modelValue: boolean
    label?: string
    disabled?: boolean
    size?: 'sm' | 'md'
  }>(),
  {
    disabled: false,
    size: 'md',
  },
)

const emit = defineEmits<{ 'update:modelValue': [value: boolean] }>()

const sizeMap = {
  sm: { text: 'text-sm' },
  md: { text: 'text-sm' },
}

function toggle() {
  if (props.disabled) return
  emit('update:modelValue', !props.modelValue)
}
</script>

<template>
  <button
    type="button"
    role="checkbox"
    class="group inline-flex cursor-pointer items-center gap-2 rounded-md select-none focus:outline-none focus-visible:ring-2 focus-visible:ring-violet-500/40"
    :class="{ 'cursor-not-allowed opacity-50': disabled }"
    :aria-checked="modelValue"
    :disabled="disabled"
    @click="toggle"
  >
    <CheckboxIndicator :model-value="modelValue" :size="size" />
    <span v-if="label || $slots.default" :class="[sizeMap[size].text, 'text-zinc-700 dark:text-zinc-200']">
      <slot>{{ label }}</slot>
    </span>
  </button>
</template>
