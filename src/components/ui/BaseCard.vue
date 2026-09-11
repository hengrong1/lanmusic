<script setup lang="ts">
withDefaults(
  defineProps<{
    padding?: 'none' | 'sm' | 'md' | 'lg'
    hoverable?: boolean
  }>(),
  {
    padding: 'md',
    hoverable: false,
  },
)

const paddingClasses = {
  none: '',
  sm: 'p-3',
  md: 'p-4',
  lg: 'p-6',
}
</script>

<template>
  <div
    class="rounded-xl border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900"
    :class="[paddingClasses[padding], hoverable ? 'transition-shadow hover:shadow-md' : '']"
  >
    <slot />
  </div>
</template>
