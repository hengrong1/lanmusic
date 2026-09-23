<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { FolderOpenIcon as FolderOpen } from '@solar-icons/vue/linear/folder-open'
import { HeartIcon as Heart } from '@solar-icons/vue/linear/heart'
import { RefreshIcon as LoaderCircle } from '@solar-icons/vue/linear/refresh'
import { MusicNoteIcon as Music } from '@solar-icons/vue/linear/music-note'
import { EyeClosedIcon as SearchX } from '@solar-icons/vue/linear/eye-closed'
import { ListCheckIcon as ListChecks } from '@solar-icons/vue/linear/list-check'
import { PlayIcon as Play } from '@solar-icons/vue/bold/play'
import { Playlist2Icon as ListPlus } from '@solar-icons/vue/linear/playlist-2'
import { AddSquareIcon as AddSquare } from '@solar-icons/vue/linear/add-square'
import { TrashBinTrashIcon as TrashBinTrash } from '@solar-icons/vue/linear/trash-bin-trash'
import { open as openFileDialog } from '@tauri-apps/plugin-dialog'
import TrackTable from '@/components/TrackTable.vue'
import EmptyState from '@/components/EmptyState.vue'
import ContextMenu from '@/components/ContextMenu.vue'
import type { MenuItem } from '@/components/ContextMenu.vue'
import { useLibraryStore } from '@/stores/library'
import { usePlayerStore } from '@/stores/player'
import { useNav } from '@/composables/useNav'
import { useStagger } from '@/composables/useStagger'
import { toast } from '@/composables/useToast'
import { confirmDialog } from '@/composables/useConfirm'
import { api } from '@/api/commands'
import { BaseButton, BaseSelect } from '@/components/ui'
import type { SelectOption } from '@/components/ui'
import { useI18n } from 'vue-i18n'
import { errorText } from '@/i18n/error'

const { t } = useI18n()
const library = useLibraryStore()
const player = usePlayerStore()
const nav = useNav()
const root = ref<HTMLElement | null>(null)
// ready = 数据落定（加载完成，空也算就绪）：加载中先隐藏 stagger 元素，就绪后播一次
useStagger(root, computed(() => library.trackPage.items.length > 0 || !library.loading))

const sortOptions = computed<SelectOption[]>(() => [
  { value: 'title', label: t('library.byTitle') },
  { value: 'album', label: t('library.byAlbum') },
  { value: 'artist', label: t('library.byArtist') },
  { value: 'added', label: t('library.byDateAdded') },
  { value: 'duration', label: t('library.byDuration') },
])

const header = computed(() => {
  const r = nav.current.value
  if (r.search) return { title: t('library.searchTitle', { query: r.search }), subtitle: '' }
  if (r.albumId) return { title: r.albumTitle ?? t('album.title'), subtitle: t('album.title') }
  if (r.artistId) return { title: r.artistName ?? t('artist.title'), subtitle: t('artist.title') }
  if (r.favorites) return { title: t('library.myFavorites'), subtitle: t('library.myMusic') }
  if (r.recent) return { title: t('nav.recent'), subtitle: t('library.myMusic') }
  return { title: t('library.allTracks'), subtitle: t('library.myMusic') }
})

/** 曲目总数标签（带参翻译在 setup 内生成） */
const totalLabel = computed(() =>
  library.trackPage.total ? t('common.songsCount', { count: library.trackPage.total.toLocaleString() }) : '',
)

/** 排序下拉直接双向绑定到库查询状态（最近播放页内部使用 'recent'，不影响存档）。
 * 表头点击会产生带 "-" 前缀的降序值，下拉框展示时剥离前缀归到基础选项。 */
const sort = computed({
  get: () => (library.query.sort || '').replace(/^-/, '') || 'title',
  set: (v: string) => library.setQuery({ sort: v }),
})

function syncQuery() {
  const r = nav.current.value
  if (r.albumId) {
    library.setQuery({ view: 'album', refId: r.albumId, search: undefined })
  } else if (r.artistId) {
    library.setQuery({ view: 'artist', refId: r.artistId, search: undefined })
  } else if (r.search) {
    library.setQuery({ view: 'all', search: r.search })
  } else if (r.favorites) {
    library.setQuery({ view: 'favorites', search: undefined })
  } else if (r.recent) {
    library.setQuery({ view: 'all', sort: 'recent', search: undefined })
  } else {
    // 从最近播放返回时恢复用户选择的排序；其余情况保持当前排序
    const restore =
      library.query.sort === 'recent' ? localStorage.getItem('lm.sort') || 'title' : library.query.sort
    library.setQuery({ view: 'all', search: undefined, sort: restore })
  }
}

onMounted(syncQuery)
watch(
  () => [
    nav.current.value.albumId,
    nav.current.value.artistId,
    nav.current.value.search,
    nav.current.value.recent,
    nav.current.value.favorites,
  ],
  syncQuery,
)

const adding = ref(false)
async function addFolder() {
  const path = await openFileDialog({ directory: true, multiple: false })
  if (!path || adding.value) return
  adding.value = true
  try {
    await library.addFolder(path as string)
  } catch (e) {
    toast(errorText(e), 'error')
  } finally {
    adding.value = false
  }
}

// ---- 多选批量操作（全部歌曲 / 喜欢 / 最近播放 / 专辑 / 艺人 / 搜索结果通用）----
const batchMode = ref(false)
const selIds = ref<number[]>([])
const selLabel = computed(() => t('common.selectedWithCount', { count: selIds.value.length }))
const selTracks = computed(() => library.trackPage.items.filter((t) => selIds.value.includes(t.id)))

function enterBatch() {
  batchMode.value = true
  selIds.value = []
}
function exitBatch() {
  batchMode.value = false
  selIds.value = []
}
function onSelection(ids: number[]) {
  selIds.value = ids
}
// 切换视图/路由时退出多选（列表已整体更换，旧选择失去意义）
watch(
  () => nav.current.value,
  () => exitBatch(),
)

function batchPlay() {
  if (selTracks.value.length) {
    player.playList(selTracks.value, 0)
    exitBatch()
  }
}
function batchEnqueue() {
  if (selTracks.value.length) {
    selTracks.value.forEach((t) => player.enqueue(t))
    toast(t('toast.addedToQueueCount', { count: selTracks.value.length }))
    exitBatch()
  }
}

// 添加到歌单：按钮位置弹出歌单菜单（复用 ContextMenu）
const playlistMenu = ref<{ x: number; y: number } | null>(null)
const playlistMenuItems = ref<MenuItem[]>([])

function batchAddToPlaylist(e: MouseEvent) {
  if (!selIds.value.length) return
  if (!library.playlists.length) {
    toast(t('playlist.noneToPick'))
    return
  }
  // 按钮上方弹出（placement='top'）：取按钮几何而非点击点——点击点在按钮中部，
  // 直接用会让菜单底边压在操作栏上；左缘对齐按钮左缘
  const btn = (e.currentTarget as HTMLElement | null) ?? (e.target as HTMLElement)
  const rect = btn.getBoundingClientRect()
  playlistMenu.value = { x: rect.left, y: rect.top }
  playlistMenuItems.value = library.playlists.slice(0, 50).map((p) => ({
    label: `${p.name} (${p.trackCount})`,
    action: () => void addSelectedToPlaylist(p.id),
  }))
}
async function addSelectedToPlaylist(pid: number) {
  try {
    // library.addToPlaylist 自带「新增 N 首 / 全部已在歌单」toast
    await library.addToPlaylist(pid, selIds.value)
    exitBatch()
  } catch (e) {
    toast(errorText(e), 'error')
  }
}

/** 从曲库移除（不删磁盘文件；记录可在设置 → 已移除歌曲 查看） */
async function batchRemoveFromLibrary() {
  if (!selIds.value.length) return
  const ok = await confirmDialog({
    title: t('library.removeTracksTitle', { count: selIds.value.length }),
    message: t('library.removeTracksMessage'),
    danger: true,
    confirmText: t('library.removeTracksConfirm'),
  })
  if (!ok) return
  try {
    const removed = await api.removeTracks(selIds.value)
    toast(t('library.removedFromLibrary', { count: removed }))
    exitBatch()
    await Promise.all([library.loadTracks(), library.loadStats(), library.loadPlaylists()])
    const still = player.current ? await api.getTrack(player.current.id).catch(() => null) : null
    if (player.current && !still) player.clearQueue()
  } catch (e) {
    toast(errorText(e), 'error')
  }
}
</script>

<template>
  <div ref="root" class="flex h-full min-h-0 flex-col">
    <!-- 头部 -->
    <div class="flex shrink-0 items-end justify-between px-6 pt-5 pb-4">
      <div>
        <p data-stagger class="text-xs font-semibold tracking-wider text-violet-500 uppercase">{{ header.subtitle || $t('common.result') }}</p>
        <h1 data-stagger class="mt-0.5 text-2xl font-bold text-zinc-900 dark:text-zinc-50">{{ header.title }}</h1>
      </div>
      <div class="flex shrink-0 items-center gap-3">
        <span v-if="library.trackPage.total" data-stagger class="text-sm text-zinc-500 whitespace-nowrap">
          {{ totalLabel }}
        </span>
        <!-- 多选：批量播放 / 加入队列 / 添加到歌单 / 从曲库移除 -->
        <BaseButton
          v-if="library.trackPage.total"
          data-stagger
          class="shrink-0"
          size="sm"
          :variant="batchMode ? 'primary' : 'outline'"
          :icon="ListChecks"
          @click="batchMode ? exitBatch() : enterBatch()"
        >
          {{ batchMode ? $t('playlist.exitBatch') : $t('playlist.batchMode') }}
        </BaseButton>
        <BaseSelect
          v-if="!nav.current.value.search && !nav.current.value.recent"
          v-model="sort"
          data-stagger
          class="w-36"
          :options="[{ value: 'none', label: $t('library.sortLibraryOrder') }, ...sortOptions]"
          size="sm"
        />
      </div>
    </div>

    <!-- 空状态 -->
    <div v-if="!library.hasSource && !library.trackPage.total" class="min-h-0 flex-1">
      <EmptyState
        :icon="Music"
        :title="$t('empty.libraryTitle')"
        :description="$t('empty.libraryHint')"
      >
        <BaseButton
          class="mt-2"
          :icon="adding ? LoaderCircle : FolderOpen"
          :loading="adding"
          :disabled="adding"
          @click="addFolder"
        >
          {{ $t('empty.addMusicFolder') }}
        </BaseButton>
      </EmptyState>
    </div>

    <!-- 扫描进行中：来源已添加、数据还没入库（扫描在后台异步跑）。
         不能落到下面的「没有找到匹配的歌曲」——那是搜索语义，此时显示纯属误导 -->
    <div v-else-if="!library.trackPage.total && library.scanning" class="flex min-h-0 flex-1 flex-col items-center justify-center">
      <LoaderCircle class="h-6 w-6 animate-spin text-violet-500" />
      <p class="mt-3 text-sm text-zinc-500 dark:text-zinc-300">{{ $t('empty.scanningTitle') }}</p>
      <p class="mt-1 text-xs text-zinc-400">{{ $t('empty.scanningHint') }}</p>
    </div>

    <div v-else-if="!library.trackPage.total && library.loading" class="flex min-h-0 flex-1 items-center justify-center">
      <LoaderCircle class="h-6 w-6 animate-spin text-violet-500" />
    </div>

    <div v-else-if="nav.current.value.favorites && !library.trackPage.total" class="min-h-0 flex-1">
      <EmptyState
        :icon="Heart"
        :title="$t('empty.likedTitle')"
        :description="$t('empty.likedHint')"
      />
    </div>

    <!-- 搜索无结果（有搜索词才谈得上「换个关键词」） -->
    <div v-else-if="nav.current.value.search && !library.trackPage.total" class="min-h-0 flex-1">
      <EmptyState :icon="SearchX" :title="$t('empty.noSongsMatch')" :description="$t('empty.noSongsMatchHint')" />
    </div>

    <!-- 扫描完成但一首都没有：来源里没有音频文件（与搜索语义分开） -->
    <div v-else-if="!library.trackPage.total" class="min-h-0 flex-1">
      <EmptyState :icon="Music" :title="$t('empty.noTracksTitle')" :description="$t('empty.noTracksHint')" />
    </div>

    <!-- 曲目表 -->
    <div v-else class="min-h-0 flex-1">
      <TrackTable
        :tracks="library.trackPage.items"
        :sort="library.query.sort"
        :batch-mode="batchMode"
        follow-playing
        paginate-missing
        @selection="onSelection"
        @sort-change="(v: string) => library.setQuery({ sort: v })"
        @near-end="library.loadMore()"
        @refresh="library.loadTracks()"
      />
    </div>

    <!-- 批量操作条（Teleport 到 body：脱离 app-surface-blur 主卡片 DOM，否则 bg-white 被映射成近全透白雾） -->
    <Teleport to="body">
    <Transition
      enter-active-class="transition duration-200 ease-out"
      enter-from-class="opacity-0 translate-y-3"
      leave-active-class="transition duration-150 ease-in"
      leave-to-class="opacity-0 translate-y-3"
    >
      <div
        v-if="batchMode && selIds.length"
        class="app-surface-blur fixed bottom-24 left-1/2 z-30 flex max-w-[calc(100vw-16px)] -translate-x-1/2 flex-wrap items-center justify-center gap-1.5 rounded-2xl border border-white/15 bg-(--app-surface) px-3.5 py-2 shadow-xl [&>*]:shrink-0"
      >
        <span class="px-2 text-xs font-medium text-zinc-500 dark:text-zinc-400">{{ selLabel }}</span>
        <BaseButton variant="ghost" size="sm" :icon="Play" @click="batchPlay">{{ $t('player.play') }}</BaseButton>
        <BaseButton variant="ghost" size="sm" :icon="ListPlus" @click="batchEnqueue">{{ $t('player.addToQueue') }}</BaseButton>
        <BaseButton variant="ghost" size="sm" :icon="AddSquare" @click="batchAddToPlaylist">{{ $t('playlist.addToPlaylist') }}</BaseButton>
        <BaseButton variant="ghost" tone="danger" size="sm" :icon="TrashBinTrash" @click="batchRemoveFromLibrary">{{ $t('library.removeFromLibrary') }}</BaseButton>
        <BaseButton variant="ghost" size="sm" class="ml-1" @click="exitBatch">{{ $t('common.cancel') }}</BaseButton>
      </div>
    </Transition>
    </Teleport>

    <!-- 添加到歌单：歌单选择菜单（多选操作栏在底部 → placement=top 向上弹出） -->
    <ContextMenu
      v-if="playlistMenu"
      placement="top"
      :x="playlistMenu.x"
      :y="playlistMenu.y"
      :items="playlistMenuItems"
      @close="playlistMenu = null"
    />
  </div>
</template>
