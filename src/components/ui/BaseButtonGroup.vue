<script setup lang="ts">
import type { Component } from 'vue'

export interface ButtonGroupItem {
  value: string
  label: string
  icon?: Component
}

const props = withDefaults(
  defineProps<{
    modelValue: string
    items: ButtonGroupItem[]
    size?: 'xs' | 'sm' | 'md'
    disabled?: boolean
  }>(),
  {
    size: 'sm',
    disabled: false,
  },
)

const emit = defineEmits<{ 'update:modelValue': [value: string] }>()

const sizeClasses = {
  xs: 'px-2.5 py-1 text-xs',
  sm: 'px-3 py-1.5 text-sm',
  md: 'px-4 py-2 text-sm',
}

const iconSizeClasses = {
  xs: 'h-3.5 w-3.5',
  sm: 'h-4 w-4',
  md: 'h-4 w-4',
}

function isActive(item: ButtonGroupItem) {
  return props.modelValue === item.value
}

function select(item: ButtonGroupItem) {
  if (props.disabled) return
  emit('update:modelValue', item.value)
}
</script>

<template>
  <div
    class="inline-flex items-center gap-0.5 rounded-lg bg-zinc-100 p-0.5 dark:bg-zinc-800"
    :class="{ 'opacity-50': disabled }"
  >
    <button
      v-for="item in items"
      :key="item.value"
      type="button"
      class="inline-flex cursor-pointer items-center gap-1.5 rounded-md font-medium transition-all duration-150"
      :class="[
        sizeClasses[size],
        isActive(item)
          ? 'bg-white text-violet-600 shadow-sm dark:bg-zinc-700 dark:text-violet-400'
          : 'text-zinc-500 hover:bg-zinc-200/60 hover:text-zinc-700 dark:text-zinc-400 dark:hover:bg-zinc-700/60 dark:hover:text-zinc-200',
      ]"
      :disabled="disabled"
      @click="select(item)"
    >
      <component v-if="item.icon" :is="item.icon" :class="iconSizeClasses[size]" />
      {{ item.label }}
    </button>
  </div>
</template>
