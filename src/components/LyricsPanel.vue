<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { usePlayerStore } from '@/stores/player'

const player = usePlayerStore()
const container = ref<HTMLElement | null>(null)

const hasSynced = computed(() => !!player.lyricsLines?.length)

/** 上下留白 = 容器半高：首行歌词正好从垂直中心开始，滚动连续无跳变 */
const pad = ref(0)
let resizeObserver: ResizeObserver | null = null
function updatePad() {
  if (container.value) {
    pad.value = Math.max(0, container.value.clientHeight / 2 - 20)
  }
}
onMounted(() => {
  updatePad()
  resizeObserver = new ResizeObserver(updatePad)
  if (container.value) resizeObserver.observe(container.value)
})
onBeforeUnmount(() => resizeObserver?.disconnect())

function scrollToActive() {
  const idx = player.activeLyricIndex
  if (idx < 0 || !container.value || !player.lyricsLines) return
  const el = container.value.querySelector(`[data-idx="${idx}"]`) as HTMLElement | null
  if (!el) return
  const target = el.offsetTop - container.value.clientHeight / 2 + el.clientHeight / 2
  container.value.scrollTo({ top: Math.max(0, target), behavior: 'smooth' })
}

watch(() => player.activeLyricIndex, () => void nextTick(scrollToActive))
// 歌词行加载完成 / 切歌后立即定位到当前行
watch(
  () => player.lyricsLines,
  () => void nextTick(scrollToActive),
)
onMounted(() => void nextTick(scrollToActive))
</script>

<template>
  <div
    ref="container"
    class="no-scrollbar relative h-full scroll-smooth px-6"
    :class="hasSynced ? 'overflow-y-auto' : 'flex flex-col items-center justify-center overflow-hidden'"
    :style="hasSynced ? { paddingTop: pad + 'px', paddingBottom: pad + 'px' } : undefined"
  >
    <!-- 加载中 -->
    <p v-if="player.lyricsLoading" class="text-center text-sm text-zinc-500">歌词加载中…</p>

    <!-- 时间轴歌词 -->
    <template v-else-if="player.lyricsLines?.length">
      <div class="relative">
        <!-- 左侧时间轴虚线：贯穿整个歌词列表，圆点按钮浮于其上 -->
        <div class="absolute top-0 bottom-0 left-[11px] border-l-2 border-dashed border-zinc-300/60 dark:border-zinc-600/40"></div>
        <div
          v-for="(line, i) in player.lyricsLines"
          :key="i"
          :data-idx="i"
          class="relative flex"
          :class="line.text ? 'py-2.5' : 'py-0.5'"
        >
          <!-- 虚线左侧的跳转按钮（圆点居中于虚线上，hover 放大便于点按） -->
          <button
            class="absolute top-1/2 left-0 flex h-6 w-6 -translate-y-1/2 items-center justify-center rounded-full transition-transform hover:scale-125"
            :title="line.text ? `跳到：${line.text}` : '跳转到间奏'"
            @click="player.seek(line.time)"
          >
            <span
              class="block rounded-full transition-[background-color,box-shadow]"
              :class="[
                i === player.activeLyricIndex
                  ? 'h-2 w-2 bg-[var(--np-accent,#fff)] shadow-[0_0_10px_var(--np-accent,#fff)]'
                  : 'bg-zinc-400/80 hover:bg-zinc-600 dark:bg-zinc-500 dark:hover:bg-zinc-300',
                !line.text ? '!bg-zinc-300/40 dark:!bg-zinc-700/40' : '',
              ]"
            ></span>
          </button>
          <p
            class="flex-1 text-center transition-[color,transform,text-shadow] duration-300 ease-out"
            :class="[
              line.text ? 'text-base' : 'text-xs leading-none',
              i === player.activeLyricIndex
                ? 'scale-[1.07] font-semibold'
                : 'text-zinc-400/80 dark:text-zinc-500',
            ]"
            :style="
              i === player.activeLyricIndex
                ? { color: 'var(--np-accent, #ffffff)', textShadow: '0 0 22px var(--np-accent, #ffffff)' }
                : undefined
            "
          >
            <template v-if="line.text">{{ line.text }}</template>
            <!-- 间奏占位：折叠后的一行，极简 -->
            <span v-else class="tracking-[0.5em] opacity-30">···</span>
          </p>
        </div>
      </div>
    </template>

    <!-- 纯文本歌词 -->
    <template v-else-if="player.lyricsPlain?.length">
      <p
        v-for="(line, i) in player.lyricsPlain"
        :key="i"
        class="py-1.5 text-center text-sm leading-relaxed text-zinc-500 dark:text-zinc-400"
      >
        {{ line }}
      </p>
    </template>

    <!-- 无歌词 -->
    <p v-else class="text-center text-sm text-zinc-500">
      暂无歌词<br /><span class="text-xs opacity-70">支持 .lrc 同名文件或内嵌歌词</span>
    </p>
  </div>
</template>

<style scoped>
/* 歌词区域自身滚动，不显示滚动条 */
.no-scrollbar {
  scrollbar-width: none;
}
.no-scrollbar::-webkit-scrollbar {
  display: none;
}
</style>
