<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { MusicNoteIcon as Music } from '@solar-icons/vue/linear/music-note'
import { coverUrl } from '@/api/scheme'

const props = defineProps<{ albumId: number | null | undefined; rounded?: string }>()

const failed = ref(false)
const loaded = ref(false)
/**
 * 交叉淡入的旧图：切歌时 img 换 src 会立刻清空像素、回落到灰色占位，
 * 新图加载完才淡入（闪一下占位图）。这里把上一张封面固化在底层 img，
 * 等新图 onload 后同步淡出（300ms 交叉），过渡完再卸载。
 */
const prevSrc = ref<string | null>(null)
let prevCleanup: ReturnType<typeof setTimeout> | undefined

const src = computed(() => coverUrl(props.albumId))
const showFallback = computed(() => !src.value || failed.value)

watch(src, (nv, ov) => {
  failed.value = false
  loaded.value = false
  clearTimeout(prevCleanup)
  if (ov && ov !== nv) {
    prevSrc.value = ov
    prevCleanup = setTimeout(() => (prevSrc.value = null), 700)
  } else {
    prevSrc.value = null
  }
})
onBeforeUnmount(() => clearTimeout(prevCleanup))

/** 新图加载失败：撤掉旧图兜底，让占位图露出（旧图再留着就是错误画面） */
function onImgError() {
  failed.value = true
  clearTimeout(prevCleanup)
  prevSrc.value = null
}
</script>

<template>
  <div
    class="cover-fallback relative overflow-hidden"
    :class="rounded ?? 'rounded-md'"
  >
    <!-- 默认占位图常驻最底层：封面未加载完成（网络慢 / 虚拟滚动刚挂载）或加载失败时显示，
         图片就绪后淡入盖上，避免加载期间只剩空白灰底 -->
    <div class="absolute inset-0 flex items-center justify-center text-zinc-400 dark:text-zinc-600">
      <Music class="h-[42%] w-[42%]" :stroke-width="1.5" />
    </div>
    <!-- 旧图兜底层（交叉淡入用）：新图淡入期间保持画面连续，就绪后同步淡出 -->
    <img
      v-if="prevSrc"
      :src="prevSrc"
      class="absolute inset-0 h-full w-full object-cover transition-opacity duration-300"
      :class="loaded ? 'opacity-0' : 'opacity-100'"
      draggable="false"
    />
    <img
      v-if="!showFallback"
      :src="src!"
      class="absolute inset-0 h-full w-full object-cover transition-opacity duration-300"
      :class="loaded ? 'opacity-100' : 'opacity-0'"
      draggable="false"
      loading="lazy"
      @load="loaded = true"
      @error="onImgError"
    />
  </div>
</template>
