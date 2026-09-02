<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { MusicNoteIcon as Music } from '@solar-icons/vue/linear/music-note'
import { coverUrl } from '@/api/scheme'

const props = defineProps<{ albumId: number | null | undefined; rounded?: string }>()

const failed = ref(false)
watch(
  () => props.albumId,
  () => (failed.value = false),
)

const src = computed(() => coverUrl(props.albumId))
const showFallback = computed(() => !src.value || failed.value)
</script>

<template>
  <div
    class="relative overflow-hidden bg-gradient-to-br from-zinc-200 to-zinc-300 dark:from-zinc-800 dark:to-zinc-800/50"
    :class="rounded ?? 'rounded-md'"
  >
    <img
      v-if="!showFallback"
      :src="src!"
      class="h-full w-full object-cover"
      draggable="false"
      loading="lazy"
      @error="failed = true"
    />
    <div v-else class="flex h-full w-full items-center justify-center text-zinc-400 dark:text-zinc-600">
      <Music class="h-[42%] w-[42%]" :stroke-width="1.5" />
    </div>
  </div>
</template>
