<script setup lang="ts">
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
  sm: { box: 'h-4 w-4', text: 'text-sm', check: 'h-2.5 w-2.5' },
  md: { box: 'h-5 w-5', text: 'text-sm', check: 'h-3 w-3' },
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
    class="inline-flex cursor-pointer items-center gap-2 rounded-md select-none focus:outline-none focus-visible:ring-2 focus-visible:ring-violet-500/40"
    :class="{ 'cursor-not-allowed opacity-50': disabled }"
    :aria-checked="modelValue"
    :disabled="disabled"
    @click="toggle"
  >
    <span
      class="relative flex shrink-0 items-center justify-center rounded border-2 transition-all duration-150"
      :class="[
        sizeMap[size].box,
        modelValue
          ? 'border-violet-500 bg-violet-500'
          : 'border-zinc-300 bg-transparent dark:border-zinc-600',
      ]"
    >
      <svg
        v-if="modelValue"
        :class="['text-white transition-transform duration-150', sizeMap[size].check]"
        viewBox="0 0 14 14"
        fill="none"
      >
        <path
          d="M3 7l3.5 3.5L11 4"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
    </span>
    <span v-if="label || $slots.default" :class="[sizeMap[size].text, 'text-zinc-700 dark:text-zinc-200']">
      <slot>{{ label }}</slot>
    </span>
  </button>
</template>
