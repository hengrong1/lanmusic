<script setup lang="ts">
import { computed } from 'vue'
import type { Component } from 'vue'

export interface TabItem {
  value: string
  label: string
  icon?: Component
}

const props = withDefaults(
  defineProps<{
    modelValue: string
    items: TabItem[]
  }>(),
  {},
)

const emit = defineEmits<{ 'update:modelValue': [value: string] }>()

const activeIndex = computed(() => props.items.findIndex((t) => t.value === props.modelValue))
const indicatorStyle = computed(() => ({
  width: `${100 / Math.max(props.items.length, 1)}%`,
  transform: `translateX(${Math.max(activeIndex.value, 0) * 100}%)`,
}))
</script>

<template>
  <div class="relative flex border-b border-zinc-200 dark:border-zinc-700">
    <button
      v-for="tab in items"
      :key="tab.value"
      type="button"
      class="relative z-10 flex flex-1 cursor-pointer items-center justify-center gap-1.5 px-4 py-2.5 text-sm font-medium transition-colors"
      :class="[
        tab.value === modelValue
          ? 'text-violet-600 dark:text-violet-400'
          : 'text-zinc-500 hover:text-zinc-700 dark:text-zinc-400 dark:hover:text-zinc-200',
      ]"
      @click="emit('update:modelValue', tab.value)"
    >
      <component v-if="tab.icon" :is="tab.icon" class="h-4 w-4" />
      {{ tab.label }}
    </button>
    <!-- 活动指示器：modelValue 未命中任何项时隐藏，避免指示条飞出容器 -->
    <div
      v-if="activeIndex >= 0"
      class="absolute bottom-0 left-0 h-0.5 rounded-full bg-violet-500 transition-all duration-200 ease-out"
      :style="indicatorStyle"
    />
  </div>
</template>
