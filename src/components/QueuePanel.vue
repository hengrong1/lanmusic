<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { LocateFixed, Trash2, X } from '@lucide/vue'
import { usePlayerStore } from '@/stores/player'

const player = usePlayerStore()

const props = defineProps<{ open?: boolean }>()
const emit = defineEmits<{ close: [] }>()

// 点击面板外部 / 按 Esc 关闭
const panel = ref<HTMLElement | null>(null)
function onDocClick(e: MouseEvent) {
  const t = e.target as HTMLElement
  // 播放条上的队列开合按钮自己处理切换，不在此关闭
  if (t.closest('[data-queue-toggle]')) return
  if (panel.value && !panel.value.contains(t)) emit('close')
}
function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') emit('close')
}
onMounted(() => {
  document.addEventListener('click', onDocClick, true)
  window.addEventListener('keydown', onKey)
})
onBeforeUnmount(() => {
  document.removeEventListener('click', onDocClick, true)
  window.removeEventListener('keydown', onKey)
})

function fmt(s: number | null | undefined) {
  if (s == null || !Number.isFinite(s)) return '--:--'
  const m = Math.floor(s / 60)
  const sec = Math.floor(s % 60)
  return `${m}:${String(sec).padStart(2, '0')}`
}

// ---- 定位正在播放：不可见时浮出按钮 ----
const listEl = ref<HTMLElement | null>(null)
const activeVisible = ref(true)

function activeRow(): HTMLElement | null {
  return (
    (listEl.value?.querySelector(`[data-queue-idx="${player.index}"]`) as HTMLElement | null) ?? null
  )
}
function checkVisible() {
  const el = activeRow()
  if (!el || !listEl.value) {
    activeVisible.value = true
    return
  }
  const top = el.offsetTop
  const c = listEl.value
  activeVisible.value = top >= c.scrollTop - 1 && top + el.offsetHeight <= c.scrollTop + c.clientHeight + 1
}
function locateActive() {
  const el = activeRow()
  if (!el || !listEl.value) return
  listEl.value.scrollTo({ top: el.offsetTop - listEl.value.clientHeight / 2, behavior: 'smooth' })
}
watch(
  () => player.index,
  () => void nextTick(checkVisible),
)

// 每次打开面板：当前播放不在可视区就直接滚过去
function locateOnOpen() {
  void nextTick(() => {
    checkVisible()
    if (!activeVisible.value) {
      const el = activeRow()
      if (el && listEl.value) {
        listEl.value.scrollTo({ top: Math.max(0, el.offsetTop - listEl.value.clientHeight / 2) })
      }
      checkVisible()
    }
  })
}
watch(
  () => props.open,
  (v) => {
    if (v) locateOnOpen()
  },
)
</script>

<template>
  <Teleport to="body">
    <Transition
      enter-active-class="transition duration-200 ease-out"
      enter-from-class="translate-y-3 scale-[0.97] opacity-0"
      leave-active-class="transition duration-150 ease-in"
      leave-to-class="translate-y-3 scale-[0.97] opacity-0"
    >
      <aside
        v-if="open && player.queue.length"
        ref="panel"
        class="fixed right-2 bottom-[88px] z-50 flex max-h-[calc(100vh-120px)] w-80 origin-bottom-right flex-col overflow-hidden rounded-xl border border-zinc-200 bg-white/98 shadow-2xl dark:border-zinc-800 dark:bg-zinc-900/98"
      >
        <header class="flex shrink-0 items-center justify-between border-b border-zinc-200 px-4 py-3 dark:border-zinc-800">
          <div>
            <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">播放队列</h2>
            <p class="text-xs text-zinc-500">{{ player.queue.length }} 首</p>
          </div>
          <div class="flex items-center gap-1">
            <button
              class="flex h-8 w-8 items-center justify-center rounded-full text-zinc-500 hover:bg-zinc-100 hover:text-red-500 dark:hover:bg-zinc-800"
              title="清空队列"
              @click="player.clearQueue()"
            >
              <Trash2 class="h-4 w-4" />
            </button>
            <button
              class="flex h-8 w-8 items-center justify-center rounded-full text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800"
              title="关闭"
              @click="$emit('close')"
            >
              <X class="h-4 w-4" />
            </button>
          </div>
        </header>

        <div ref="listEl" class="relative min-h-0 flex-1 overflow-y-auto py-1" @scroll.passive="checkVisible">
          <div
            v-for="(t, i) in player.queue"
            :key="`${t.id}-${i}`"
            :data-queue-idx="i"
            class="group flex cursor-default items-center gap-3 px-4 py-2 text-sm"
            :class="
              i === player.index
                ? 'bg-gradient-to-r from-violet-100 to-transparent shadow-[inset_2px_0_0_0_#8b5cf6] dark:from-violet-500/15 dark:to-transparent'
                : 'hover:bg-zinc-100 dark:hover:bg-zinc-800/60'
            "
            @dblclick="player.playAt(i)"
          >
            <span
              v-if="i === player.index"
              class="flex h-4 w-5 shrink-0 items-end justify-center gap-[2.5px]"
              :class="player.playing ? '' : 'eq-paused'"
            >
              <span class="eq-bar w-[3px] rounded-full bg-gradient-to-t from-violet-600 to-fuchsia-400" style="animation-delay: 0s"></span>
              <span class="eq-bar w-[3px] rounded-full bg-gradient-to-t from-violet-600 to-fuchsia-400" style="animation-delay: 0.25s"></span>
              <span class="eq-bar w-[3px] rounded-full bg-gradient-to-t from-violet-600 to-fuchsia-400" style="animation-delay: 0.5s"></span>
            </span>
            <span v-else class="w-5 shrink-0 text-center text-xs tabular-nums" :class="i === player.index ? 'text-violet-500' : 'text-zinc-400'">
              {{ i + 1 }}
            </span>
            <div class="min-w-0 flex-1">
              <p
                class="truncate"
                :class="i === player.index ? 'font-medium text-violet-600 dark:text-violet-400' : 'text-zinc-800 dark:text-zinc-100'"
              >
                {{ t.title }}
              </p>
              <p class="truncate text-xs text-zinc-500">{{ t.artist ?? '未知艺人' }}</p>
            </div>
            <span
              class="shrink-0 font-mono text-xs tabular-nums"
              :class="i === player.index ? 'text-violet-500' : 'text-zinc-400'"
            >{{ fmt(t.duration) }}</span>
            <button
              class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full text-zinc-400 opacity-0 hover:bg-zinc-200 hover:text-zinc-600 group-hover:opacity-100 dark:hover:bg-zinc-700"
              title="移出队列"
              @click="player.removeFromQueue(i)"
            >
              <X class="h-3.5 w-3.5" />
            </button>
          </div>
        </div>
        <!-- 定位正在播放的歌曲 -->
        <Transition
          enter-active-class="transition duration-200 ease-out"
          enter-from-class="opacity-0 translate-y-2"
          leave-active-class="transition duration-150 ease-in"
          leave-to-class="opacity-0"
        >
          <button
            v-if="!activeVisible && player.index >= 0"
            class="absolute right-4 bottom-4 z-10 flex max-w-[240px] items-center gap-2 rounded-full bg-violet-500 px-4 py-2 text-xs font-medium text-white shadow-lg shadow-violet-500/30 transition hover:bg-violet-400"
            title="滚动到正在播放的歌曲"
            @click="locateActive"
          >
            <LocateFixed class="h-3.5 w-3.5 shrink-0" />
            <span class="truncate">正在播放：{{ player.current?.title }}</span>
          </button>
        </Transition>
      </aside>
    </Transition>
  </Teleport>
</template>

<style scoped>
.eq-paused .eq-bar {
  animation-play-state: paused;
}
.eq-bar {
  height: 100%;
  transform: scaleY(0.4);
  transform-origin: bottom;
  animation: eq 0.9s ease-in-out infinite alternate;
}
/* 用 scaleY 代替高度动画：合成器处理，不触发布局重排 */
@keyframes eq {
  from {
    transform: scaleY(0.2);
  }
  to {
    transform: scaleY(1);
  }
}
</style>
