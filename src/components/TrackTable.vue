<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { ArrowDownIcon as ArrowDown } from '@solar-icons/vue/linear/arrow-down'
import { ArrowUpIcon as ArrowUp } from '@solar-icons/vue/linear/arrow-up'
import { VerifiedCheckIcon as Check } from '@solar-icons/vue/linear/verified-check'
import { SortVerticalIcon as ChevronsUpDown } from '@solar-icons/vue/linear/sort-vertical'
import { VinylRecordIcon as Disc3 } from '@solar-icons/vue/linear/vinyl-record'
import { FolderOpenIcon as FolderOpen } from '@solar-icons/vue/linear/folder-open'
import { HeartIcon as Heart } from '@solar-icons/vue/linear/heart'
import { ListDownIcon as ListEnd } from '@solar-icons/vue/linear/list-down'
import { Playlist2Icon as ListPlus } from '@solar-icons/vue/linear/playlist-2'
import { MapPointIcon as LocateFixed } from '@solar-icons/vue/linear/map-point'
import { PlayIcon as Play } from '@solar-icons/vue/linear/play'
import type { Track } from '@/types'
import VirtualList from '@/components/VirtualList.vue'
import ContextMenu from '@/components/ContextMenu.vue'
import type { MenuItem } from '@/components/ContextMenu.vue'
import { usePlayerStore } from '@/stores/player'
import { useLibraryStore } from '@/stores/library'
import { useNav } from '@/composables/useNav'
import { api } from '@/api/commands'
import { toast } from '@/composables/useToast'

const props = defineProps<{ tracks: Track[]; playlistId?: number; favoritesView?: boolean; sort?: string; reorderable?: boolean; batchMode?: boolean }>()
const emit = defineEmits<{
  nearEnd: []
  refresh: []
  reorder: [from: number, to: number]
  sortChange: [value: string]
  selection: [ids: number[]]
}>()

const player = usePlayerStore()
const library = useLibraryStore()
const nav = useNav()

// ---- 表头点击排序（传入 sort 属性时启用；歌单视图保持拖拽顺序不启用）----
const vlist = ref<{ scrollToTop: () => void; scrollToIndex: (i: number) => void } | null>(null)
const sortCols = [
  { field: 'title', label: '标题' },
  { field: 'artist', label: '艺人' },
  { field: 'album', label: '专辑' },
  { field: 'duration', label: '时长' },
] as const

const isAsc = (field: string) => props.sort === field
const isDesc = (field: string) => props.sort === `-${field}`
/** 三态循环：升序 → 降序 → 不排序（'none' = 入库顺序） */
function toggleSort(field: string) {
  if (isAsc(field)) emit('sortChange', `-${field}`)
  else if (isDesc(field)) emit('sortChange', 'none')
  else emit('sortChange', field)
}
watch(
  () => props.sort,
  () => vlist.value?.scrollToTop(),
)

// ---- 定位正在播放：不可见时浮出按钮，点击滚动过去 ----
const range = ref({ start: 0, end: 30 })
function onRange(s: number, e: number) {
  range.value = { start: s, end: e }
}
const playingIndex = computed(() =>
  player.current ? props.tracks.findIndex((t) => t.id === player.current!.id) : -1,
)
const showLocate = computed(
  () =>
    playingIndex.value >= 0 &&
    (playingIndex.value < range.value.start || playingIndex.value >= range.value.end),
)
function locatePlaying() {
  if (playingIndex.value >= 0) vlist.value?.scrollToIndex(playingIndex.value)
}

function fmtDuration(s: number | null | undefined) {
  if (s == null || !Number.isFinite(s)) return '--:--'
  const m = Math.floor(s / 60)
  const sec = Math.floor(s % 60)
  return `${m}:${String(sec).padStart(2, '0')}`
}

function rowClick(_t: Track, i: number) {
  selected.value = i
}

// ---- 批量多选模式 ----
const selSet = ref<Set<number>>(new Set())
watch(
  () => props.batchMode,
  (m) => {
    if (!m) selSet.value = new Set()
  },
)
watch(
  selSet,
  (s) => emit('selection', [...s]),
  { deep: true },
)
const allSelected = computed(() => props.tracks.length > 0 && props.tracks.every((t) => selSet.value.has(t.id)))
function toggleAll() {
  const next = new Set(selSet.value)
  if (allSelected.value) props.tracks.forEach((t) => next.delete(t.id))
  else props.tracks.forEach((t) => next.add(t.id))
  selSet.value = next
}
function onRowClick(t: Track, i: number) {
  if (props.batchMode) {
    const next = new Set(selSet.value)
    if (next.has(t.id)) next.delete(t.id)
    else next.add(t.id)
    selSet.value = next
  } else {
    rowClick(t, i)
  }
}

function rowDblClick(_t: Track, i: number) {
  player.playList(props.tracks, i)
}

const selected = ref(-1)

/** 正在播放的行：渐变底色 + 左侧紫条 + 标题紫色 + 跳动音条 */
function rowClass(t: Track, index: number) {
  if (props.batchMode && selSet.value.has(t.id)) {
    return 'bg-violet-50 dark:bg-violet-500/10'
  }
  if (player.current?.id === t.id) {
    return 'bg-gradient-to-r from-violet-100 to-transparent shadow-[inset_2px_0_0_0_#8b5cf6] dark:from-violet-500/15 dark:to-transparent'
  }
  if (selected.value === index) return 'bg-violet-100/70 dark:bg-violet-500/10'
  return 'hover:bg-zinc-100/80 dark:hover:bg-zinc-800/50'
}

// ---- 右键菜单 ----
const menu = ref<{ x: number; y: number; track: Track } | null>(null)
const menuItems = computed<MenuItem[]>(() => {
  const t = menu.value?.track
  if (!t) return []
  const items: MenuItem[] = [
    {
      label: '播放',
      icon: Play,
      action: () => player.playList(props.tracks, props.tracks.findIndex((x) => x.id === t.id)),
    },
    { label: '下一首播放', icon: ListEnd, action: () => player.playNextInQueue(t) },
    { label: '加入队列', icon: ListPlus, action: () => player.enqueue(t) },
    {
      label: t.fav ? '取消喜欢' : '喜欢',
      icon: Heart,
      action: () =>
        api
          .favoriteToggle(t.id, !t.fav)
          .then(() => {
            t.fav = !t.fav
            // 刷新侧边栏「我的喜欢」计数
            void library.loadStats()
            if (props.favoritesView) emit('refresh')
          })
          .catch((e) => toast(String(e), 'error')),
    },
  ]

  if (props.playlistId != null) {
    items.push({
      label: '从歌单移除',
      danger: true,
      action: () => {
        library
          .removeFromPlaylist(props.playlistId!, t.id)
          .then(() => emit('refresh'))
          .catch((e) => toast(String(e), 'error'))
      },
    })
  } else {
    items.push({
      label: '加入歌单',
      children: library.playlists.length
        ? library.playlists.map((p) => ({
            label: p.name,
            action: () => {
              library.addToPlaylist(p.id, [t.id]).catch((e) => toast(String(e), 'error'))
            },
          }))
        : [{ label: '（先在侧边栏新建歌单）', disabled: true }],
    })
  }

  items.push(
    {
      label: '查看专辑',
      icon: Disc3,
      disabled: t.albumId == null,
      action: () => nav.go({ view: 'tracks', albumId: t.albumId!, albumTitle: t.album ?? '未知专辑' }),
    },
    {
      label: '在文件夹中显示',
      icon: FolderOpen,
      action: () => api.revealTrack(t.id).catch((e) => toast(String(e), 'error')),
    },
  )
  return items
})

function openMenu(e: MouseEvent, t: Track) {
  e.preventDefault()
  selected.value = props.tracks.findIndex((x) => x.id === t.id)
  menu.value = { x: e.clientX, y: e.clientY, track: t }
}

function openArtist(t: Track) {
  if (t.artistId == null) return
  nav.go({ view: 'tracks', artistId: t.artistId, artistName: t.artist ?? '未知艺人' })
}

function openAlbum(t: Track) {
  if (t.albumId == null) return
  nav.go({ view: 'tracks', albumId: t.albumId, albumTitle: t.album ?? '未知专辑' })
}

// ---- 拖拽排序（仅传入 reorderable 时启用）----
const dragIndex = ref(-1)
const dragOverIndex = ref(-1)

function onDragStart(i: number) {
  if (!props.reorderable || props.batchMode) return
  dragIndex.value = i
}
function onDragOver(e: DragEvent, i: number) {
  if (dragIndex.value < 0) return
  e.preventDefault()
  dragOverIndex.value = i
}
function onDrop(e: DragEvent, i: number) {
  e.preventDefault()
  if (dragIndex.value >= 0 && dragIndex.value !== i) emit('reorder', dragIndex.value, i)
  dragIndex.value = -1
  dragOverIndex.value = -1
}
function onDragEnd() {
  dragIndex.value = -1
  dragOverIndex.value = -1
}
</script>

<template>
  <div class="flex h-full min-h-0 flex-col">
    <!-- 表头 -->
    <div
      class="group/th grid h-10 shrink-0 items-center gap-3 border-b border-zinc-200 px-4 pb-0.5 text-xs font-medium text-zinc-500 dark:border-zinc-800 dark:text-zinc-500"
      style="grid-template-columns: 40px minmax(0, 1fr) minmax(0, 220px) minmax(0, 220px) 56px"
    >
      <span v-if="props.batchMode" class="flex justify-center">
        <button
          class="flex h-4 w-4 cursor-pointer items-center justify-center rounded border transition"
          :class="allSelected ? 'border-violet-500 bg-violet-500 text-white' : 'border-zinc-300 dark:border-zinc-600'"
          title="全选"
          @click.stop="toggleAll"
        >
          <Check v-if="allSelected" class="h-3 w-3" />
        </button>
      </span>
      <span v-else class="text-center">#</span>
      <template v-for="col in sortCols" :key="col.field">
        <button
          v-if="props.sort !== undefined"
          class="flex cursor-pointer items-center gap-1 transition hover:text-zinc-700 dark:hover:text-zinc-200"
          :class="[col.field === 'duration' ? 'w-full justify-end' : '', (isAsc(col.field) || isDesc(col.field)) ? 'text-violet-500' : '']"
          @click="toggleSort(col.field)"
        >
          {{ col.label }}
          <ArrowUp v-if="isAsc(col.field)" class="h-3 w-3" />
          <ArrowDown v-else-if="isDesc(col.field)" class="h-3 w-3" />
          <ChevronsUpDown v-else class="h-3 w-3 opacity-40 transition group-hover/th:opacity-80" />
        </button>
        <span v-else :class="col.field === 'duration' ? 'text-right' : ''">{{ col.label }}</span>
      </template>
    </div>

    <div class="relative min-h-0 flex-1">
      <VirtualList
        ref="vlist"
        :items="props.tracks"
        :item-height="44"
        :item-key="(t: Track) => t.id"
        @near-end="emit('nearEnd')"
        @range="onRange"
      >
        <template #default="{ item: t, index }">
          <div
            class="group grid h-full items-center gap-3 px-4 text-sm select-none"
            :class="[
              rowClass(t, index),
              dragOverIndex === index && dragIndex !== index ? 'border-t-2 border-violet-500' : '',
              props.reorderable && !props.batchMode
                ? dragIndex === index
                  ? 'cursor-grabbing'
                  : 'cursor-grab'
                : 'cursor-default',
            ]"
            style="grid-template-columns: 40px minmax(0, 1fr) minmax(0, 220px) minmax(0, 220px) 56px"
            :title="t.path"
            :draggable="props.reorderable === true && !props.batchMode"
            @click="onRowClick(t, index)"
            @dblclick="rowDblClick(t, index)"
            @contextmenu="openMenu($event, t)"
            @dragstart="onDragStart(index)"
            @dragover="onDragOver($event, index)"
            @drop="onDrop($event, index)"
            @dragend="onDragEnd"
          >
            <div v-if="props.batchMode" class="relative flex h-5 items-center justify-center">
              <span
                class="flex h-4 w-4 items-center justify-center rounded border transition"
                :class="selSet.has(t.id) ? 'border-violet-500 bg-violet-500 text-white' : 'border-zinc-300 dark:border-zinc-600'"
              >
                <Check v-if="selSet.has(t.id)" class="h-3 w-3" />
              </span>
            </div>
            <div v-else class="relative flex h-5 items-center justify-center">
              <span
                v-if="player.current?.id !== t.id"
                class="text-xs text-zinc-400 group-hover:invisible dark:text-zinc-500"
                >{{ index + 1 }}</span
              >
              <Play
                v-if="player.current?.id !== t.id"
                class="absolute h-3.5 w-3.5 invisible text-zinc-500 group-hover:visible dark:text-zinc-300"
              />
              <span
                v-else
                class="flex h-4 items-end justify-center gap-[2.5px]"
                :class="player.playing ? '' : 'eq-paused'"
              >
                <span class="eq-bar w-[3px] rounded-full bg-gradient-to-t from-violet-600 to-fuchsia-400" style="animation-delay: 0s"></span>
                <span class="eq-bar w-[3px] rounded-full bg-gradient-to-t from-violet-600 to-fuchsia-400" style="animation-delay: 0.25s"></span>
                <span class="eq-bar w-[3px] rounded-full bg-gradient-to-t from-violet-600 to-fuchsia-400" style="animation-delay: 0.5s"></span>
              </span>
            </div>
            <div class="min-w-0 truncate" :class="player.current?.id === t.id ? 'font-medium text-violet-600 dark:text-violet-400' : 'text-zinc-800 dark:text-zinc-100'">
              {{ t.title }}
            </div>
            <div class="min-w-0 truncate text-zinc-500 dark:text-zinc-400">
              <button
                class="max-w-full cursor-pointer truncate transition hover:text-violet-600 hover:underline dark:hover:text-violet-400"
                :title="`查看艺人：${t.artist ?? '未知艺人'}`"
                @click.stop="openArtist(t)"
              >{{ t.artist ?? '未知艺人' }}</button>
            </div>
            <div class="min-w-0 truncate text-zinc-500 dark:text-zinc-400">
              <button
                class="max-w-full cursor-pointer truncate transition hover:text-violet-600 hover:underline dark:hover:text-violet-400"
                :title="`查看专辑：${t.album ?? '未知专辑'}`"
                @click.stop="openAlbum(t)"
              >{{ t.album ?? '未知专辑' }}</button>
            </div>
            <div class="text-right font-mono text-xs tabular-nums transition-colors" :class="player.current?.id === t.id ? 'text-violet-500' : 'text-zinc-500 dark:text-zinc-400'">
              {{ fmtDuration(t.duration) }}
            </div>
          </div>
        </template>
      </VirtualList>

      <!-- 定位正在播放的歌曲 -->
      <Transition
        enter-active-class="transition duration-200 ease-out"
        enter-from-class="opacity-0 translate-y-2"
        leave-active-class="transition duration-150 ease-in"
        leave-to-class="opacity-0"
      >
        <button
          v-if="showLocate"
          class="absolute right-6 bottom-5 z-10 flex max-w-[260px] cursor-pointer items-center gap-2 rounded-full bg-violet-500 px-4 py-2 text-xs font-medium text-white shadow-lg shadow-violet-500/30 transition hover:bg-violet-400"
          title="滚动到正在播放的歌曲"
          @click="locatePlaying"
        >
          <LocateFixed class="h-3.5 w-3.5 shrink-0" />
          <span class="truncate">正在播放：{{ player.current?.title }}</span>
        </button>
      </Transition>
    </div>

    <ContextMenu v-if="menu" :x="menu.x" :y="menu.y" :items="menuItems" @close="menu = null" />
  </div>
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
