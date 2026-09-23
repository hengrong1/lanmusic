<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { MapPointIcon as LocateFixed } from '@solar-icons/vue/linear/map-point'
import { AltArrowUpIcon as ArrowUp } from '@solar-icons/vue/linear/alt-arrow-up'
import { PlaylistIcon as ListPlus } from '@solar-icons/vue/linear/playlist'
import { TrashBin2Icon as Trash2 } from '@solar-icons/vue/linear/trash-bin-2'
import { CloseIcon as X } from '@solar-icons/vue/linear/close'
import VirtualList from '@/components/VirtualList.vue'
import type { Track } from '@/types'
import { usePlayerStore } from '@/stores/player'
import { useLibraryStore } from '@/stores/library'
import { useNav } from '@/composables/useNav'
import { useAmbient } from '@/composables/useAmbient'
import { toast } from '@/composables/useToast'
import { useI18n } from 'vue-i18n'
import { errorText } from '@/i18n/error'

const { t } = useI18n()
const player = usePlayerStore()
const library = useLibraryStore()
const nav = useNav()

/** 队列曲目数（带参翻译在 setup 内生成） */
const queueCountLabel = computed(() => t('common.songsCount', { count: player.queue.length }))

const props = defineProps<{ open?: boolean; /** 播放页打开时面板跟随环境主题 */ nowPlaying?: boolean }>()
const emit = defineEmits<{ close: [] }>()

// ---- 主题：播放页打开时跟随环境配色（渐变背景 + 专辑主色强调），普通视图沿用应用卡片风 ----
const { palette } = useAmbient()
const themed = computed(() => props.nowPlaying === true)
const panelBg = computed(() => {
  const p = palette.value
  return p
    ? `linear-gradient(to bottom, ${p.glow} 0%, ${p.deep} 55%, #09090b 100%)`
    : 'linear-gradient(to bottom, #2e1065 0%, #09090b 55%, #09090b 100%)'
})
const accent = computed(() => palette.value?.accent ?? '#a78bfa')
const accentSoft = computed(() => palette.value?.accentSoft ?? 'rgba(139, 92, 246, 0.2)')
const accent2 = computed(() => palette.value?.accent2 ?? '#c084fc')

/** 活动行文字强调色（主题模式；普通模式走原 violet class） */
function accentStyle(active: boolean) {
  return themed.value && active ? { color: accent.value } : undefined
}
/** 活动行背景：主题模式用环境色横向渐变，普通模式走原 violet 渐变 class */
function rowStyle(i: number) {
  if (i !== player.index || !themed.value) return undefined
  return { background: `linear-gradient(to right, ${accentSoft.value}, transparent 72%)` }
}
/** 均衡器条：主题模式取环境色双色调渐变，普通模式保持原 violet class */
function eqStyle(delay: string) {
  return themed.value
    ? { animationDelay: delay, background: `linear-gradient(to top, ${accent.value}, ${accent2.value})` }
    : { animationDelay: delay }
}
const activeRowClass = computed(() =>
  themed.value
    ? 'mx-1 rounded-xl'
    : 'queue-active-row mx-1 rounded-xl bg-gradient-to-r from-violet-100 via-violet-50/50 to-transparent shadow-[inset_0_0_0_1px_color-mix(in_srgb,var(--color-violet-500)_15%,transparent),0_2px_8px_-2px_color-mix(in_srgb,var(--color-violet-500)_20%,transparent)] dark:from-violet-500/20 dark:via-violet-500/10 dark:to-transparent dark:shadow-[inset_0_0_0_1px_color-mix(in_srgb,var(--color-violet-500)_25%,transparent),0_2px_8px_-2px_color-mix(in_srgb,var(--color-violet-500)_30%,transparent)]',
)
const inactiveRowClass = computed(() =>
  // 非主题色模式才补主色描边：themed（播放页随封面配色）下强调色是 --accent，不是软件主色
  themed.value ? 'mx-1 rounded-xl hover:bg-white/10' : 'hover-accent-line mx-1 rounded-xl hover:bg-zinc-100 dark:hover:bg-zinc-800/60',
)

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
  if (scrollRaf) cancelAnimationFrame(scrollRaf)
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
// 列表用 VirtualList 虚拟滚动（与 TrackTable 同款），行高固定 52px：
// 标题 20px + 歌手 16px + py-2 上下共 16px
const ITEM_H = 52
const LIST_PAD_TOP = 16
const LIST_PAD_BOTTOM = 20
/** 结构化类型：泛型组件不支持 InstanceType 提取 */
const vlist = ref<{
  scrollToTop: () => void
  scrollToIndex: (index: number, align?: 'top' | 'center', behavior?: ScrollBehavior) => void
} | null>(null)
const activeVisible = ref(true)
const showBackToTop = ref(false)
/** 最近一次滚动指标：滚动事件里记录，定位判断不再逐次裸读布局 */
let metrics = { top: 0, height: 0 }

function scrollerEl(): HTMLElement | null {
  return panel.value?.querySelector<HTMLElement>('[data-queue-scroller]') ?? null
}

let scrollRaf = 0
function onQueueScroll() {
  // rAF 节流：滚动事件高频触发，布局读取与状态更新合并到每帧一次
  if (scrollRaf) return
  scrollRaf = requestAnimationFrame(() => {
    scrollRaf = 0
    syncMetrics()
    updateActiveVisible()
  })
}

function updateActiveVisible() {
  if (player.index < 0) {
    activeVisible.value = true
  } else {
    const top = LIST_PAD_TOP + player.index * ITEM_H
    activeVisible.value = top >= metrics.top - 1 && top + ITEM_H <= metrics.top + metrics.height + 1
  }
  showBackToTop.value = metrics.top > 100
}

function locateActive() {
  if (player.index >= 0) vlist.value?.scrollToIndex(player.index, 'center')
}

function scrollToTop() {
  vlist.value?.scrollToTop()
}

function syncMetrics() {
  const el = scrollerEl()
  if (el) metrics = { top: el.scrollTop, height: el.clientHeight }
}

// 每次打开面板：当前播放不在可视区就直接滚过去
function locateOnOpen() {
  void nextTick(() => {
    syncMetrics()
    updateActiveVisible()
    if (!activeVisible.value && player.index >= 0) {
      vlist.value?.scrollToIndex(player.index, 'center', 'auto')
      void nextTick(() => {
        syncMetrics()
        updateActiveVisible()
      })
    }
  })
}
watch(
  () => props.open,
  (v) => {
    if (v) locateOnOpen()
  },
)
watch(
  () => player.index,
  () => {
    // 切歌自动跟随：面板开着且新活动行不在可视区就滚过去居中（与打开面板时的定位同款），
    // 省得每次切歌都要手动点定位按钮；行已在可视区则只更新按钮显隐，不打断浏览
    if (!props.open) return
    void nextTick(() => {
      syncMetrics()
      updateActiveVisible()
      if (!activeVisible.value && player.index >= 0) {
        vlist.value?.scrollToIndex(player.index, 'center', 'auto')
        void nextTick(() => {
          syncMetrics()
          updateActiveVisible()
        })
      }
    })
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
        class="queue-panel fixed right-2 bottom-[88px] z-50 flex h-[calc(100vh-120px)] w-80 origin-bottom-right flex-col overflow-hidden rounded-xl border shadow-2xl"
        :class="themed ? 'border-white/10' : 'app-surface-blur border-white/15 bg-(--app-surface)'"
        :style="themed ? { background: panelBg } : undefined"
      >
        <header
          class="flex shrink-0 items-center justify-between border-b px-4 py-3"
          :class="themed ? 'border-white/10' : 'border-zinc-200 dark:border-zinc-800'"
        >
          <div>
            <h2 class="text-sm font-semibold" :class="themed ? 'text-white/90' : 'text-zinc-800 dark:text-zinc-100'">{{ $t('queue.title') }}</h2>
            <p class="text-xs" :class="themed ? 'text-white/40' : 'text-zinc-500'">{{ queueCountLabel }}</p>
          </div>
          <div class="flex items-center gap-1">
            <button
              class="transition-colors duration-150 flex h-8 w-8 cursor-pointer items-center justify-center rounded-lg disabled:cursor-default disabled:opacity-40"
              :class="themed ? 'text-white/50 hover:bg-white/10 hover:text-white' : 'text-zinc-500 hover:bg-zinc-100 hover:text-violet-500 dark:hover:bg-zinc-800'"
              v-tooltip="$t('queue.saveAsPlaylistHint')"
              :disabled="saving"
              @click="saveAsPlaylist"
            >
              <ListPlus class="h-4 w-4" />
            </button>
            <button
              class="transition-colors duration-150 flex h-8 w-8 cursor-pointer items-center justify-center rounded-lg"
              :class="themed ? 'text-white/50 hover:bg-white/10 hover:text-red-400' : 'text-zinc-500 hover:bg-zinc-100 hover:text-red-500 dark:hover:bg-zinc-800'"
              v-tooltip="$t('queue.clearQueue')"
              @click="player.clearQueue()"
            >
              <Trash2 class="h-4 w-4" />
            </button>
            <button
              class="transition-colors duration-150 flex h-8 w-8 cursor-pointer items-center justify-center rounded-lg"
              :class="themed ? 'text-white/50 hover:bg-white/10 hover:text-white' : 'text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800'"
              v-tooltip="$t('common.close')"
              @click="$emit('close')"
            >
              <X class="h-4 w-4" />
            </button>
          </div>
        </header>

        <!-- 队列虚拟滚动：pt-4/pb-5 由 pad-top/pad-bottom 提供（在滚动容器内部，光晕不被裁切） -->
        <VirtualList
          ref="vlist"
          data-queue-scroller
          class="min-h-0 flex-1 select-none"
          :items="player.queue"
          :item-height="ITEM_H"
          :item-key="(t: Track, i: number) => `${t.id}-${i}`"
          :pad-top="LIST_PAD_TOP"
          :pad-bottom="LIST_PAD_BOTTOM"
          @scroll="onQueueScroll"
        >
          <template #default="{ item: t, index: i }">
            <div
              class="group flex h-full cursor-default items-center gap-3 px-4 py-2 text-sm transition-all duration-300"
              :class="i === player.index ? activeRowClass : inactiveRowClass"
              :style="rowStyle(i)"
              @dblclick="i === player.index ? player.toggle() : player.playAt(i)"
            >
              <span
                v-if="i === player.index"
                class="flex h-4 w-5 shrink-0 items-end justify-center gap-[2.5px]"
                :class="player.playing ? '' : 'eq-paused'"
              >
                <span class="eq-bar w-[3px] rounded-full bg-gradient-to-t from-violet-600 to-violet-400" :style="eqStyle('0s')"></span>
                <span class="eq-bar w-[3px] rounded-full bg-gradient-to-t from-violet-600 to-violet-400" :style="eqStyle('0.25s')"></span>
                <span class="eq-bar w-[3px] rounded-full bg-gradient-to-t from-violet-600 to-violet-400" :style="eqStyle('0.5s')"></span>
              </span>
              <span
                v-else
                class="w-5 shrink-0 text-center text-xs tabular-nums"
                :class="themed ? 'text-white/35' : 'text-zinc-400'"
              >
                {{ i + 1 }}
              </span>
              <div class="min-w-0 flex-1">
                <p
                  class="truncate"
                  :class="
                    i === player.index
                      ? themed
                        ? 'font-medium'
                        : 'font-medium text-violet-600 dark:text-violet-400'
                      : themed
                        ? 'text-white/85'
                        : 'text-zinc-800 dark:text-zinc-100'
                  "
                  :style="accentStyle(i === player.index)"
                >
                  {{ t.title }}
                </p>
                <p class="truncate text-xs" :class="themed ? 'text-white/45' : 'text-zinc-500'">
                  {{ t.artist ?? $t('artist.unknownArtist') }}
                </p>
              </div>
              <span
                class="shrink-0 font-mono text-xs tabular-nums"
                :class="i === player.index ? (themed ? '' : 'text-violet-500') : themed ? 'text-white/40' : 'text-zinc-400'"
                :style="accentStyle(i === player.index)"
              >{{ fmt(t.duration) }}</span>
              <button
                class="transition-colors duration-150 flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-lg text-zinc-400 opacity-0 hover:bg-zinc-200 hover:text-zinc-600 group-hover:opacity-100 dark:hover:bg-zinc-700"
                v-tooltip="$t('player.removeFromQueue')"
                @click="player.removeFromQueue(i)"
              >
                <X class="h-3.5 w-3.5" />
              </button>
            </div>
          </template>
        </VirtualList>
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
              class="flex h-9 w-9 cursor-pointer items-center justify-center rounded-full shadow-lg transition"
              :class="themed ? 'bg-white/10 shadow-black/30 hover:bg-white/20' : 'bg-white bg-solid shadow-zinc-300/50 hover:bg-zinc-100 dark:bg-zinc-800 dark:shadow-zinc-900/50 dark:hover:bg-zinc-700'"
              v-tooltip="$t('queue.scrollToCurrent')"
              @click="locateActive"
            >
              <LocateFixed class="h-4 w-4" :class="themed ? 'text-white/80' : 'text-zinc-600 dark:text-zinc-300'" />
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
              class="flex h-9 w-9 cursor-pointer items-center justify-center rounded-full shadow-lg transition"
              :class="themed ? 'bg-white/10 shadow-black/30 hover:bg-white/20' : 'bg-white bg-solid shadow-zinc-300/50 hover:bg-zinc-100 dark:bg-zinc-800 dark:shadow-zinc-900/50 dark:hover:bg-zinc-700'"
              v-tooltip="$t('common.backToTop')"
              @click="scrollToTop"
            >
              <ArrowUp class="h-4 w-4" :class="themed ? 'text-white/80' : 'text-zinc-600 dark:text-zinc-300'" />
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
      inset 0 0 0 1px color-mix(in srgb, var(--color-violet-500) 15%, transparent),
      0 2px 8px -2px color-mix(in srgb, var(--color-violet-500) 20%, transparent);
  }
  50% {
    box-shadow:
      inset 0 0 0 1px color-mix(in srgb, var(--color-violet-500) 30%, transparent),
      0 4px 16px -2px color-mix(in srgb, var(--color-violet-500) 35%, transparent);
  }
}
</style>
