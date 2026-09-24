<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { MusicNoteIcon as Music } from '@solar-icons/vue/linear/music-note'
import { coverUrl } from '@/api/scheme'

const props = defineProps<{ albumId: number | null | undefined; rounded?: string }>()

const failed = ref(false)
const loaded = ref(false)
watch(
  () => props.albumId,
  () => {
    failed.value = false
    loaded.value = false
  },
)

const src = computed(() => coverUrl(props.albumId))
const showFallback = computed(() => !src.value || failed.value)
</script>

<template>
  <div
    class="cover-fallback relative overflow-hidden"
    :class="rounded ?? 'rounded-md'"
  >
    <!-- 默认占位图常驻底层：封面未加载完成（网络慢 / 虚拟滚动刚挂载）或加载失败时显示，
         图片就绪后淡入盖上，避免加载期间只剩空白灰底 -->
    <div class="absolute inset-0 flex items-center justify-center text-zinc-400 dark:text-zinc-600">
      <Music class="h-[42%] w-[42%]" :stroke-width="1.5" />
    </div>
    <img
      v-if="!showFallback"
      :src="src!"
      class="absolute inset-0 h-full w-full object-cover transition-opacity duration-300"
      :class="loaded ? 'opacity-100' : 'opacity-0'"
      draggable="false"
      loading="lazy"
      @load="loaded = true"
      @error="failed = true"
    />
  </div>
</template>
