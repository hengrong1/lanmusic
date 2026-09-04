<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { IS_WIN } from '@/utils/platform'

const title = ref('MV 播放')
const trackId = ref<number | null>(null)

const videoUrl = computed(() => {
  if (!trackId.value) return ''
  return IS_WIN ? `http://video.localhost/mv/${trackId.value}` : `video://mv/${trackId.value}`
})

onMounted(async () => {
  // 从 window.location 获取 trackId
  const urlObj = new URL(window.location.href)
  const id = urlObj.searchParams.get('trackId')
  if (id) {
    trackId.value = Number(id)
  }
  
  // 尝试从后端获取歌曲标题
  if (trackId.value) {
    try {
      const { api } = await import('@/api/commands')
      const track = await api.getTrack(trackId.value)
      if (track) {
        title.value = track.title
        document.title = `${track.title} - MV`
      }
    } catch {
      // 忽略错误
    }
  }
})
</script>

<template>
  <div class="flex h-screen w-screen items-center justify-center bg-black">
    <video
      v-if="videoUrl"
      :src="videoUrl"
      controls
      autoplay
      class="max-h-full max-w-full"
    ></video>
    <div v-else class="text-white">无法加载视频</div>
  </div>
</template>
