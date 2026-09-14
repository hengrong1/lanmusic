<script setup lang="ts">
import { computed } from 'vue'

const props = withDefaults(
  defineProps<{
    modelValue: string
    value: string
    label?: string
    disabled?: boolean
    size?: 'sm' | 'md'
  }>(),
  {
    disabled: false,
    size: 'md',
  },
)

const emit = defineEmits<{ 'update:modelValue': [value: string] }>()

const sizeMap = {
  sm: { box: 'h-4 w-4', text: 'text-sm', dot: 'h-1.5 w-1.5' },
  md: { box: 'h-5 w-5', text: 'text-sm', dot: 'h-2 w-2' },
}

const isChecked = computed(() => props.modelValue === props.value)

function select() {
  if (props.disabled) return
  emit('update:modelValue', props.value)
}
</script>

<template>
  <button
    type="button"
    role="radio"
    class="group inline-flex cursor-pointer items-center gap-2 rounded-md select-none focus:outline-none focus-visible:ring-2 focus-visible:ring-violet-500/40"
    :class="{ 'cursor-not-allowed opacity-50': disabled }"
    :aria-checked="isChecked"
    :disabled="disabled"
    @click="select"
  >
    <span
      class="relative flex shrink-0 items-center justify-center rounded-full border-2 transition-all duration-150"
      :class="[
        sizeMap[size].box,
        isChecked
          ? 'border-violet-500 group-hover:border-violet-600'
          : 'border-zinc-300 group-hover:border-zinc-400 dark:border-zinc-600 dark:group-hover:border-zinc-500',
      ]"
    >
      <span
        v-if="isChecked"
        :class="[
          'rounded-full bg-violet-500 transition-transform duration-150 group-hover:bg-violet-600',
          sizeMap[size].dot,
        ]"
      />
    </span>
    <span v-if="label || $slots.default" :class="[sizeMap[size].text, 'text-zinc-700 dark:text-zinc-200']">
      <slot>{{ label }}</slot>
    </span>
  </button>
</template>
