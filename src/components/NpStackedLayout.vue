<script setup lang="ts">
import NpCoverStage from '@/components/NpCoverStage.vue'
import NpLyricsColumn from '@/components/NpLyricsColumn.vue'

const emit = defineEmits<{ navigate: [] }>()
defineProps<{ focusHidden?: boolean }>()
</script>

<template>
  <!-- 上下：封面居上（约 38% 高，居中）+ 曲目信息与歌词居下（整宽滚动）；窄窗口下更从容 -->
  <div class="flex min-h-0 flex-1 flex-col gap-4 px-10 pb-6">
    <div class="h-[38%] min-h-0 shrink-0">
      <NpCoverStage :max-size="280" />
    </div>
    <NpLyricsColumn class="min-h-0" align="center" :focus-hidden="focusHidden" @navigate="emit('navigate')" />
  </div>
</template>
