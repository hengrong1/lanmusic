<script setup lang="ts">
import type { Component } from 'vue'

defineProps<{
  icon: Component
  title: string
  description?: string
}>()
</script>

<template>
  <div class="flex h-full flex-col items-center justify-center gap-3 p-10 text-center">
    <div class="flex h-16 w-16 items-center justify-center rounded-2xl bg-zinc-100 dark:bg-zinc-800/60">
      <component :is="icon" class="h-8 w-8 text-zinc-400 dark:text-zinc-500" :stroke-width="1.5" />
    </div>
    <div>
      <p class="text-base font-medium text-zinc-700 dark:text-zinc-200">{{ title }}</p>
      <p v-if="description" class="mt-1 max-w-sm text-sm text-zinc-500 dark:text-zinc-400">{{ description }}</p>
    </div>
    <slot />
  </div>
</template>
