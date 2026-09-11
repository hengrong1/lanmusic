<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { MapPointIcon as LocateFixed } from '@solar-icons/vue/linear/map-point'
import { AltArrowUpIcon as ArrowUp } from '@solar-icons/vue/linear/alt-arrow-up'
import { PlaylistIcon as ListPlus } from '@solar-icons/vue/linear/playlist'
import { TrashBin2Icon as Trash2 } from '@solar-icons/vue/linear/trash-bin-2'
import { CloseIcon as X } from '@solar-icons/vue/linear/close'
import { usePlayerStore } from '@/stores/player'
import { useLibraryStore } from '@/stores/library'
import { useNav } from '@/composables/useNav'
import { toast } from '@/composables/useToast'
import { useI18n } from 'vue-i18n'
import { errorText } from '@/i18n/error'

const { t } = useI18n()
const player = usePlayerStore()
const library = useLibraryStore()
const nav = useNav()

/** 队列曲目数（带参翻译在 setup 内生成） */
const queueCountLabel = computed(() => t('common.songsCount', { count: player.queue.length }))

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

// ---- 另存为歌单：把当前队列整体保存为新歌单并跳转（以保存时间命名，便于多次保存区分）----
const saving = ref(false)
async function saveAsPlaylist() {
  if (saving.value || !player.queue.length) return
  saving.value = true
  try {
    const pad = (n: number) => String(n).padStart(2, '0')
    const d = new Date()
    const name = t('playlist.saveAsPlaylistDefault', {
      date: `${d.getMonth() + 1}/${d.getDate()}`,
      time: `${pad(d.getHours())}:${pad(d.getMinutes())}`,
    })
    const p = await library.createPlaylist(name)
    await library.addToPlaylist(p.id, player.queue.map((t) => t.id))
    nav.go({ view: 'playlist', playlistId: p.id, playlistName: p.name })
  } catch (e) {
    toast(errorText(e), 'error')
  } finally {
    saving.value = false
  }
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
    showBackToTop.value = listEl.value ? listEl.value.scrollTop > 100 : false
    return
  }
  const top = el.offsetTop
  const c = listEl.value
  activeVisible.value = top >= c.scrollTop - 1 && top + el.offsetHeight <= c.scrollTop + c.clientHeight + 1
  showBackToTop.value = c.scrollTop > 100
}
function locateActive() {
  const el = activeRow()
  if (!el || !listEl.value) return
  listEl.value.scrollTo({ top: el.offsetTop - listEl.value.clientHeight / 2, behavior: 'smooth' })
}
function scrollToTop() {
  if (!listEl.value) return
  listEl.value.scrollTo({ top: 0, behavior: 'smooth' })
}
const showBackToTop = ref(false)
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
            <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ $t('queue.title') }}</h2>
            <p class="text-xs text-zinc-500">{{ queueCountLabel }}</p>
          </div>
          <div class="flex items-center gap-1">
            <button
              class="transition-colors duration-150 flex h-8 w-8 cursor-pointer items-center justify-center rounded-lg text-zinc-500 hover:bg-zinc-100 hover:text-violet-500 disabled:cursor-default disabled:opacity-40 dark:hover:bg-zinc-800"
              :title="$t('queue.saveAsPlaylistHint')"
              :disabled="saving"
              @click="saveAsPlaylist"
            >
              <ListPlus class="h-4 w-4" />
            </button>
            <button
              class="transition-colors duration-150 flex h-8 w-8 cursor-pointer items-center justify-center rounded-lg text-zinc-500 hover:bg-zinc-100 hover:text-red-500 dark:hover:bg-zinc-800"
              :title="$t('queue.clearQueue')"
              @click="player.clearQueue()"
            >
              <Trash2 class="h-4 w-4" />
            </button>
            <button
              class="transition-colors duration-150 flex h-8 w-8 cursor-pointer items-center justify-center rounded-lg text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800"
              :title="$t('common.close')"
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
            class="group flex cursor-default items-center gap-3 px-4 py-2 text-sm transition-all duration-300"
            :class="
              i === player.index
                ? 'queue-active-row mx-1 rounded-xl bg-gradient-to-r from-violet-100 via-violet-50/50 to-transparent shadow-[inset_0_0_0_1px_rgba(139,92,246,0.15),0_2px_8px_-2px_rgba(139,92,246,0.2)] dark:from-violet-500/20 dark:via-violet-500/10 dark:to-transparent dark:shadow-[inset_0_0_0_1px_rgba(139,92,246,0.25),0_2px_8px_-2px_rgba(139,92,246,0.3)]'
                : 'mx-1 rounded-xl hover:bg-zinc-100 dark:hover:bg-zinc-800/60'
            "
            @dblclick="i === player.index ? player.toggle() : player.playAt(i)"
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
              <p class="truncate text-xs text-zinc-500">{{ t.artist ?? $t('artist.unknownArtist') }}</p>
            </div>
            <span
              class="shrink-0 font-mono text-xs tabular-nums"
              :class="i === player.index ? 'text-violet-500' : 'text-zinc-400'"
            >{{ fmt(t.duration) }}</span>
            <button
              class="transition-colors duration-150 flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-lg text-zinc-400 opacity-0 hover:bg-zinc-200 hover:text-zinc-600 group-hover:opacity-100 dark:hover:bg-zinc-700"
              :title="$t('player.removeFromQueue')"
              @click="player.removeFromQueue(i)"
            >
              <X class="h-3.5 w-3.5" />
            </button>
          </div>
        </div>
        <!-- 底部操作按钮：定位正在播放 + 返回顶部 -->
        <div class="absolute right-4 bottom-4 z-10 flex flex-col items-end gap-2">
          <Transition
            enter-active-class="transition duration-200 ease-out"
            enter-from-class="opacity-0 translate-y-2"
            leave-active-class="transition duration-150 ease-in"
            leave-to-class="opacity-0"
          >
            <button
              v-if="!activeVisible && player.index >= 0"
              class="flex h-9 w-9 cursor-pointer items-center justify-center rounded-full bg-white shadow-lg shadow-zinc-300/50 transition hover:bg-zinc-100 dark:bg-zinc-800 dark:shadow-zinc-900/50 dark:hover:bg-zinc-700"
              :title="$t('queue.scrollToCurrent')"
              @click="locateActive"
            >
              <LocateFixed class="h-4 w-4 text-zinc-600 dark:text-zinc-300" />
            </button>
          </Transition>
          <Transition
            enter-active-class="transition duration-200 ease-out"
            enter-from-class="opacity-0 translate-y-2"
            leave-active-class="transition duration-150 ease-in"
            leave-to-class="opacity-0"
          >
            <button
              v-if="showBackToTop"
              class="flex h-9 w-9 cursor-pointer items-center justify-center rounded-full bg-white shadow-lg shadow-zinc-300/50 transition hover:bg-zinc-100 dark:bg-zinc-800 dark:shadow-zinc-900/50 dark:hover:bg-zinc-700"
              :title="$t('common.backToTop')"
              @click="scrollToTop"
            >
              <ArrowUp class="h-4 w-4 text-zinc-600 dark:text-zinc-300" />
            </button>
          </Transition>
        </div>
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
/* 正在播放行：渐变流动 + 光晕脉冲 */
.queue-active-row {
  background-size: 200% 100%;
  animation: queue-shimmer 3s ease-in-out infinite, queue-pulse 2s ease-in-out infinite;
}
@keyframes queue-shimmer {
  0%,
  100% {
    background-position: 0% 50%;
  }
  50% {
    background-position: 100% 50%;
  }
}
@keyframes queue-pulse {
  0%,
  100% {
    box-shadow:
      inset 0 0 0 1px rgba(139, 92, 246, 0.15),
      0 2px 8px -2px rgba(139, 92, 246, 0.2);
  }
  50% {
    box-shadow:
      inset 0 0 0 1px rgba(139, 92, 246, 0.3),
      0 4px 16px -2px rgba(139, 92, 246, 0.35);
  }
}
</style>
