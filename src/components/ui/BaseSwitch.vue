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
  // 滑块尺寸 = 轨道内高（轨道高 - 上下各 2px 内边距），配合 translate 位移左右对称
  sm: {
    track: 'h-5 w-9',
    thumb: 'h-4 w-4',
    translate: 'translate-x-4',
    label: 'text-sm',
  },
  md: {
    track: 'h-6 w-11',
    thumb: 'h-5 w-5',
    translate: 'translate-x-5',
    label: 'text-sm',
  },
}

function toggle() {
  if (props.disabled) return
  emit('update:modelValue', !props.modelValue)
}
</script>

<template>
  <button
    type="button"
    role="switch"
    class="inline-flex cursor-pointer items-center gap-2.5 rounded-md select-none focus:outline-none focus-visible:ring-2 focus-visible:ring-violet-500/40"
    :class="{ 'cursor-not-allowed opacity-50': disabled }"
    :aria-checked="modelValue"
    :disabled="disabled"
    @click="toggle"
  >
    <span
      class="relative inline-flex shrink-0 rounded-full transition-colors duration-200"
      :class="[
        sizeMap[size].track,
        modelValue ? 'bg-violet-500' : 'bg-zinc-300 dark:bg-zinc-600',
      ]"
    >
      <span
        class="absolute top-0.5 left-0.5 rounded-full bg-violet-300 shadow-sm transition-transform duration-200"
        :class="[sizeMap[size].thumb, modelValue ? sizeMap[size].translate : 'translate-x-0']"
      />
    </span>
    <span v-if="label || $slots.default" :class="[sizeMap[size].label, 'text-zinc-700 dark:text-zinc-200']">
      <slot>{{ label }}</slot>
    </span>
  </button>
</template>
