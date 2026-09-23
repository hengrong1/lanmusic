<script setup lang="ts">
import NpCoverStage from '@/components/NpCoverStage.vue'
import NpLyricsColumn from '@/components/NpLyricsColumn.vue'

const emit = defineEmits<{ navigate: [] }>()
defineProps<{ focusHidden?: boolean }>()
</script>

<template>
  <!-- 经典：左侧封面（约 40% 宽，垂直居中）+ 右侧曲目信息与歌词（约 60%，居中） -->
  <div class="flex min-h-0 flex-1 gap-12 px-10 pb-8">
    <div class="h-full min-w-0 flex-1 basis-2/5">
      <NpCoverStage :max-size="340" />
    </div>
    <NpLyricsColumn class="basis-3/5" align="center" :focus-hidden="focusHidden" @navigate="emit('navigate')" />
  </div>
</template>
