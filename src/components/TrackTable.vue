<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { ArrowDownIcon as ArrowDown } from '@solar-icons/vue/linear/arrow-down'
import { ArrowUpIcon as ArrowUp } from '@solar-icons/vue/linear/arrow-up'
import { SortVerticalIcon as ChevronsUpDown } from '@solar-icons/vue/linear/sort-vertical'
import { VinylRecordIcon as Disc3 } from '@solar-icons/vue/linear/vinyl-record'
import { FolderOpenIcon as FolderOpen } from '@solar-icons/vue/linear/folder-open'
import { HeartIcon as Heart } from '@solar-icons/vue/linear/heart'
import { ListDownIcon as ListEnd } from '@solar-icons/vue/linear/list-down'
import { Playlist2Icon as ListPlus } from '@solar-icons/vue/linear/playlist-2'
import { MapPointIcon as LocateFixed } from '@solar-icons/vue/linear/map-point'
import { PlayIcon as Play } from '@solar-icons/vue/linear/play'
import { VideoFramePlayHorizontalIcon as VideoFramePlay } from '@solar-icons/vue/linear/video-frame-play-horizontal'
import { ListCrossMinimalisticIcon as ListCross } from '@solar-icons/vue/linear/list-cross-minimalistic'
import { AddSquareIcon as AddSquare } from '@solar-icons/vue/linear/add-square'
import { TrashBinTrashIcon as TrashBinTrash } from '@solar-icons/vue/linear/trash-bin-trash'
import type { Track } from '@/types'
import VirtualList from '@/components/VirtualList.vue'
import ContextMenu from '@/components/ContextMenu.vue'
import HighlightText from '@/components/HighlightText.vue'
import CheckboxIndicator from '@/components/ui/CheckboxIndicator.vue'
import type { MenuItem } from '@/components/ContextMenu.vue'
import { usePlayerStore } from '@/stores/player'
import { useLibraryStore } from '@/stores/library'
import { useNav } from '@/composables/useNav'
import { useMvPlayer } from '@/composables/useMvPlayer'
import { api } from '@/api/commands'
import { toast } from '@/composables/useToast'
import { confirmDialog } from '@/composables/useConfirm'
import { useI18n } from 'vue-i18n'
import { errorText } from '@/i18n/error'

const props = defineProps<{ tracks: Track[]; playlistId?: number; favoritesView?: boolean; sort?: string; reorderable?: boolean; batchMode?: boolean }>()
const emit = defineEmits<{
  nearEnd: []
  refresh: []
  reorder: [from: number, to: number]
  sortChange: [value: string]
  selection: [ids: number[]]
}>()

const { t: tr } = useI18n()
const player = usePlayerStore()
const library = useLibraryStore()
const nav = useNav()
const mv = useMvPlayer()

/** 带参提示文案（模板 \`$t\` 无带参重载，统一在 setup 内生成） */
const artistTip = (name: string) => tr('artist.viewArtist', { name })
const albumTip = (name: string | null | undefined) =>
  tr('album.viewAlbum', { name: name ?? tr('album.unknownAlbum') })
const filenameTip = (p: string) => tr('library.filenameHit', { path: p })

/** 当前搜索关键词（高亮命中字用） */
const searchTerm = computed(() => nav.current.value.search ?? '')

// ---- 表头点击排序（传入 sort 属性时启用；歌单视图保持拖拽顺序不启用）----
const vlist = ref<{ scrollToTop: () => void; scrollToIndex: (i: number) => void } | null>(null)
const sortCols = computed(
  () =>
    [
      { field: 'title', label: tr('library.sortTitle') },
      { field: 'artist', label: tr('library.sortArtist') },
      { field: 'album', label: tr('library.sortAlbum') },
      { field: 'duration', label: tr('library.sortDuration') },
    ] as const,
)

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
  () => {
    vlist.value?.scrollToTop()
    // 排序变化时清空选中状态
    selSet.value = new Set()
    selected.value = -1
  },
)

// ---- 定位正在播放：常驻按钮，点击滚动过去并居中 ----
// 不用「可视窗口」判断显隐：排序/刷新会让行序大变，正在播放的行经常恰好落进可视范围，
// 图标随之消失，用户会当成坏了（尤其刚排完序想跳回去的时候）。
// 另外列表是分页加载的（每页 200）：排序后正在播放的歌可能排在未加载的页里，
// 所以点击时若当前列表没有，就让 store 继续翻页直到找到。
const playingIndex = computed(() =>
  player.current ? props.tracks.findIndex((t) => t.id === player.current!.id) : -1,
)
const showLocate = computed(() => !!player.current)
const locating = ref(false)
async function locatePlaying() {
  if (!player.current || locating.value) return
  locating.value = true
  try {
    let idx = playingIndex.value
    // 不在已加载的列表里（排序后可能被翻到后面的页）：翻页加载直到找到
    if (idx < 0) idx = await library.indexOfTrack(player.current.id)
    if (idx >= 0) {
      vlist.value?.scrollToIndex(idx)
    } else {
      toast(tr('player.locateMissing'))
    }
  } finally {
    locating.value = false
  }
}
function scrollToTop() {
  vlist.value?.scrollToTop()
}
// 返回顶部按钮：滚过一屏后再浮出
function onScroll(e: Event) {
  const target = e.target as HTMLElement
  showBackToTop.value = target.scrollTop > 100
}
const showBackToTop = ref(false)

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
// 所有更新路径都整只替换 Set（toggleAll/onRowClick/退出批量模式），无需 deep 追踪
watch(selSet, (s) => emit('selection', [...s]))
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

function playMv(t: Track) {
  // 当前页面内弹出 Plyr 播放层（见 MvPlayer.vue）
  void mv.open(t)
}

const selected = ref(-1)

/** 正在播放的行：圆角卡片 + 渐变底色 + 左侧紫条 + 标题紫色 + 跳动音条 + 脉冲边框动画 */
function rowClass(t: Track, index: number) {
  if (props.batchMode && selSet.value.has(t.id)) {
    return 'bg-violet-50 dark:bg-violet-500/10'
  }
  if (player.current?.id === t.id) {
    return 'row-playing from-violet-100 via-violet-50 to-transparent dark:from-violet-500/20 dark:via-violet-500/10 dark:to-transparent ring-1 ring-inset ring-violet-200/50 dark:ring-violet-400/20'
  }
  if (selected.value === index) return 'bg-violet-100/70 dark:bg-violet-500/10'
  return 'hover:bg-violet-50/50 dark:hover:bg-violet-500/10'
}

// ---- 右键菜单 ----
const menu = ref<{ x: number; y: number; track: Track } | null>(null)
const menuItems = computed<MenuItem[]>(() => {
  const t = menu.value?.track
  if (!t) return []
  const items: MenuItem[] = [
    {
      label: tr('player.play'),
      icon: Play,
      action: () => player.playList(props.tracks, props.tracks.findIndex((x) => x.id === t.id)),
    },
    { label: tr('player.playNext'), icon: ListEnd, action: () => player.playNextInQueue(t) },
    { label: tr('player.addToQueue'), icon: ListPlus, action: () => player.enqueue(t) },
    // 仅存在同名视频文件（MV）的歌曲才显示
    ...(t.hasMv ? [{ label: tr('mv.play'), icon: VideoFramePlay, action: () => playMv(t) }] : []),
    {
      label: t.fav ? tr('library.unlike') : tr('library.like'),
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
          .catch((e) => toast(errorText(e), 'error')),
    },
  ]

  if (props.playlistId != null) {
    items.push({
      label: tr('playlist.removeFromPlaylist'),
      icon: ListCross,
      danger: true,
      action: () => {
        library
          .removeFromPlaylist(props.playlistId!, t.id)
          .then(() => emit('refresh'))
          .catch((e) => toast(errorText(e), 'error'))
      },
    })
  } else {
    items.push({
      label: tr('library.addToPlaylist'),
      icon: AddSquare,
      children: library.playlists.length
        ? library.playlists.map((p) => ({
            label: p.name,
            action: () => {
              library.addToPlaylist(p.id, [t.id]).catch((e) => toast(errorText(e), 'error'))
            },
          }))
        : [{ label: tr('playlist.createFirstHint'), disabled: true }],
    })
  }

  // 从曲库移除（不删磁盘文件；记录可在设置 → 已移除歌曲 查看）
  items.push({
    label: tr('library.removeFromLibrary'),
    icon: TrashBinTrash,
    danger: true,
    action: () => {
      confirmDialog({
        title: tr('library.removeTracksTitle', { count: 1 }),
        message: tr('library.removeTracksMessage'),
        danger: true,
        confirmText: tr('library.removeTracksConfirm'),
      })
        .then(async (ok) => {
          if (!ok) return
          const removed = await api.removeTracks([t.id])
          if (removed > 0) {
            toast(tr('library.removedFromLibrary', { count: removed }))
            emit('refresh')
            void library.loadStats()
            // 移除的是正在播放的歌曲：清空播放队列
            const still = player.current ? await api.getTrack(player.current.id).catch(() => null) : null
            if (player.current && !still) player.clearQueue()
          }
        })
        .catch((e) => toast(errorText(e), 'error'))
    },
  })

  items.push(
    {
      label: tr('album.goToAlbum'),
      icon: Disc3,
      disabled: t.albumId == null,
      action: () => nav.go({ view: 'tracks', albumId: t.albumId!, albumTitle: t.album ?? tr('album.unknownAlbum') }),
    },
    {
      label: tr('common.revealInFolder'),
      icon: FolderOpen,
      action: () => api.revealTrack(t.id).catch((e) => toast(errorText(e), 'error')),
    },
  )
  return items
})

function openMenu(e: MouseEvent, t: Track) {
  e.preventDefault()
  selected.value = props.tracks.findIndex((x) => x.id === t.id)
  menu.value = { x: e.clientX, y: e.clientY, track: t }
}

/** 行内展示的艺人列表：优先用后端拆分的多艺人（各自可点击），回退到合并字符串 */
function artistLinks(track: Track): { id: number | null; name: string }[] {
  if (track.artists?.length) return track.artists.map((a) => ({ id: a.id, name: a.name }))
  if (track.artist) return [{ id: track.artistId, name: track.artist }]
  return [{ id: null, name: tr('artist.unknownArtist') }]
}

function openArtist(artist: { id: number | null; name: string }) {
  if (artist.id == null) return
  nav.go({ view: 'tracks', artistId: artist.id, artistName: artist.name })
}

function openAlbum(track: Track) {
  if (track.albumId == null) return
  nav.go({ view: 'tracks', albumId: track.albumId, albumTitle: track.album ?? tr('album.unknownAlbum') })
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
          class="cursor-pointer rounded transition focus:outline-none focus-visible:ring-2 focus-visible:ring-violet-500/40"
          role="checkbox"
          :aria-checked="allSelected"
          v-tooltip="$t('common.selectAll')"
          @click.stop="toggleAll"
        >
          <CheckboxIndicator :model-value="allSelected" size="sm" />
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

    <!-- 底部光晕呼吸空间由 VirtualList 的 pad-bottom 提供（在滚动容器内部，虚拟滚动数学已含偏移） -->
    <div class="relative min-h-0 flex-1">
      <VirtualList
        ref="vlist"
        :items="props.tracks"
        :item-height="44"
        :item-key="(t: Track) => t.id"
        :pad-top="8"
        :pad-bottom="14"
        @near-end="emit('nearEnd')"
        @scroll="onScroll"
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
            v-tooltip="t.path"
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
              <CheckboxIndicator :model-value="selSet.has(t.id)" size="sm" />
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
                <span class="eq-bar w-[3px] rounded-full bg-gradient-to-t from-violet-600 to-violet-400" style="animation-delay: 0s"></span>
                <span class="eq-bar w-[3px] rounded-full bg-gradient-to-t from-violet-600 to-violet-400" style="animation-delay: 0.25s"></span>
                <span class="eq-bar w-[3px] rounded-full bg-gradient-to-t from-violet-600 to-violet-400" style="animation-delay: 0.5s"></span>
              </span>
            </div>
            <div class="flex min-w-0 items-center gap-1.5" :class="player.current?.id === t.id ? 'font-medium text-violet-600 dark:text-violet-400' : 'text-zinc-800 dark:text-zinc-100'">
              <span class="truncate"><HighlightText :text="t.title" :keyword="searchTerm" /></span>
              <!-- 命中字段徽标：歌词 / 文件名 -->
              <span
                v-if="t.matchedFields?.includes('lyrics')"
                class="shrink-0 rounded-full bg-violet-100 px-1.5 py-px text-[10px] font-medium text-violet-600 dark:bg-violet-500/20 dark:text-violet-300"
                v-tooltip="$t('library.lyricsHit')"
              >{{ $t('player.lyrics') }}</span>
              <span
                v-if="t.matchedFields?.includes('filename')"
                class="shrink-0 rounded-full bg-emerald-100 px-1.5 py-px text-[10px] font-medium text-emerald-600 dark:bg-emerald-500/20 dark:text-emerald-300"
                v-tooltip="filenameTip(t.path)"
              >{{ $t('settings.fieldFilename') }}</span>
              <button
                v-if="t.hasMv"
                class="shrink-0 rounded-lg p-0.5 text-violet-500 transition hover:bg-violet-500/10"
                v-tooltip="$t('mv.play')"
                :aria-label="$t('mv.play')"
                @click.stop="playMv(t)"
              >
                <VideoFramePlay class="h-4 w-4" />
              </button>
            </div>
            <div class="min-w-0 truncate text-zinc-500 dark:text-zinc-400">
              <!-- 多艺人：每个名字独立可点击（区分每一个艺人）；
                   分隔符放在 i>0 的项前面，避免为取 length 每行再调一次 artistLinks -->
              <template v-for="(a, i) in artistLinks(t)" :key="a.id ?? `na-${i}`">
                <span v-if="i > 0" class="opacity-50"> / </span>
                <button
                  v-if="a.id != null"
                  class="max-w-full cursor-pointer truncate transition hover:text-violet-600 hover:underline dark:hover:text-violet-400"
                  v-tooltip="artistTip(a.name)"
                  @click.stop="openArtist(a)"
                ><HighlightText :text="a.name" :keyword="searchTerm" /></button>
                <span v-else><HighlightText :text="a.name" :keyword="searchTerm" /></span>
              </template>
            </div>
            <div class="min-w-0 truncate text-zinc-500 dark:text-zinc-400">
              <button
                class="max-w-full cursor-pointer truncate transition hover:text-violet-600 hover:underline dark:hover:text-violet-400"
                v-tooltip="albumTip(t.album)"
                @click.stop="openAlbum(t)"
              ><HighlightText :text="t.album ?? $t('album.unknownAlbum')" :keyword="searchTerm" /></button>
            </div>
            <div class="text-right font-mono text-xs tabular-nums transition-colors" :class="player.current?.id === t.id ? 'text-violet-500' : 'text-zinc-500 dark:text-zinc-400'">
              {{ fmtDuration(t.duration) }}
            </div>
          </div>
        </template>
      </VirtualList>

      <!-- 底部操作按钮：定位正在播放 + 返回顶部 -->
      <div class="absolute right-6 bottom-5 z-10 flex flex-col items-end gap-2">
        <Transition
          enter-active-class="transition duration-200 ease-out"
          enter-from-class="opacity-0 translate-y-2"
          leave-active-class="transition duration-150 ease-in"
          leave-to-class="opacity-0"
        >
          <button
            v-if="showLocate"
            class="flex h-9 w-9 cursor-pointer items-center justify-center rounded-full bg-white shadow-lg shadow-zinc-300/50 transition hover:bg-zinc-100 dark:bg-zinc-800 dark:shadow-zinc-900/50 dark:hover:bg-zinc-700"
            v-tooltip="$t('queue.scrollToCurrent')"
            @click="locatePlaying"
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
            v-tooltip="$t('common.backToTop')"
            @click="scrollToTop"
          >
            <ArrowUp class="h-4 w-4 text-zinc-600 dark:text-zinc-300" />
          </button>
        </Transition>
      </div>
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

/* 正在播放的行：圆角卡片 + 脉冲边框 + 左侧高亮条 + 渐变动画
   （颜色全部走 violet 变量：跟随设置 → 自定义主题色，未选时回落 Tailwind 原生紫） */
.row-playing {
  position: relative;
  border-radius: 12px;
  margin: 0 4px;
  background-size: 200% 100%;
  background-image: linear-gradient(
    to right,
    color-mix(in srgb, var(--color-violet-200) 90%, transparent) 0%,
    color-mix(in srgb, var(--color-violet-100) 60%, transparent) 50%,
    transparent 100%
  );
  box-shadow:
    inset 3px 0 0 0 var(--color-violet-500),
    0 0 0 1px color-mix(in srgb, var(--color-violet-500) 15%, transparent),
    0 2px 8px color-mix(in srgb, var(--color-violet-500) 8%, transparent);
  animation: row-shimmer 3s ease-in-out infinite, row-pulse 2s ease-in-out infinite;
  transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
}
.dark .row-playing {
  background-image: linear-gradient(
    to right,
    color-mix(in srgb, var(--color-violet-500) 25%, transparent) 0%,
    color-mix(in srgb, var(--color-violet-500) 12%, transparent) 50%,
    transparent 100%
  );
  box-shadow:
    inset 3px 0 0 0 var(--color-violet-400),
    0 0 0 1px color-mix(in srgb, var(--color-violet-400) 20%, transparent),
    0 2px 12px color-mix(in srgb, var(--color-violet-500) 15%, transparent);
  animation-name: row-shimmer, row-pulse-dark;
}

/* 渐变背景流动动画 */
@keyframes row-shimmer {
  0%, 100% {
    background-position: 0% 50%;
  }
  50% {
    background-position: 100% 50%;
  }
}

/* 边框光晕脉冲（亮色） */
@keyframes row-pulse {
  0%, 100% {
    box-shadow:
      inset 3px 0 0 0 var(--color-violet-500),
      0 0 0 1px color-mix(in srgb, var(--color-violet-500) 15%, transparent),
      0 2px 8px color-mix(in srgb, var(--color-violet-500) 8%, transparent);
  }
  50% {
    box-shadow:
      inset 3px 0 0 0 var(--color-violet-500),
      0 0 0 2px color-mix(in srgb, var(--color-violet-500) 30%, transparent),
      0 4px 16px color-mix(in srgb, var(--color-violet-500) 15%, transparent);
  }
}

/* 边框光晕脉冲（暗色） */
@keyframes row-pulse-dark {
  0%, 100% {
    box-shadow:
      inset 3px 0 0 0 var(--color-violet-400),
      0 0 0 1px color-mix(in srgb, var(--color-violet-400) 20%, transparent),
      0 2px 12px color-mix(in srgb, var(--color-violet-500) 15%, transparent);
  }
  50% {
    box-shadow:
      inset 3px 0 0 0 var(--color-violet-400),
      0 0 0 2px color-mix(in srgb, var(--color-violet-400) 40%, transparent),
      0 4px 20px color-mix(in srgb, var(--color-violet-500) 25%, transparent);
  }
}
</style>
