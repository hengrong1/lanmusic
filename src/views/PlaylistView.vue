<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { ListCheckIcon as ListChecks } from '@solar-icons/vue/linear/list-check'
import { PlaylistIcon as ListMusic } from '@solar-icons/vue/linear/playlist'
import { Playlist2Icon as ListPlus } from '@solar-icons/vue/linear/playlist-2'
import { RefreshIcon as LoaderCircle } from '@solar-icons/vue/linear/refresh'
import { PenIcon as Pencil } from '@solar-icons/vue/linear/pen'
import { PlayIcon as Play } from '@solar-icons/vue/bold/play'
import { TrashBin2Icon as Trash2 } from '@solar-icons/vue/linear/trash-bin-2'
import { AddIcon as Plus } from '@solar-icons/vue/linear/add'
import TrackTable from '@/components/TrackTable.vue'
import TrackPicker from '@/components/TrackPicker.vue'
import PlaylistEditDialog from '@/components/PlaylistEditDialog.vue'
import CoverImg from '@/components/CoverImg.vue'
import EmptyState from '@/components/EmptyState.vue'
import { useLibraryStore } from '@/stores/library'
import { usePlayerStore } from '@/stores/player'
import { useNav } from '@/composables/useNav'
import { useStagger } from '@/composables/useStagger'
import { toast } from '@/composables/useToast'
import { api } from '@/api/commands'
import type { Track } from '@/types'
import { BaseButton } from '@/components/ui'

const library = useLibraryStore()
const player = usePlayerStore()
const nav = useNav()
const current = nav.current

const tracks = ref<Track[]>([])
const loading = ref(false)
const coverAlbumId = ref<number | null>(null)
const pickerOpen = ref(false)
const editOpen = ref(false)

const root = ref<HTMLElement | null>(null)
useStagger(root, computed(() => tracks.value.length > 0))

const playlistId = computed(() => nav.current.value.playlistId ?? null)
const playlistName = computed(() => nav.current.value.playlistName ?? '歌单')
const playlistMeta = computed(() => library.playlists.find((p) => p.id === playlistId.value) ?? null)
const metaDesc = computed(() => playlistMeta.value?.description ?? null)
const metaCreated = computed(() => playlistMeta.value?.createdAt ?? null)
const createdText = computed(() =>
  metaCreated.value
    ? new Date(metaCreated.value * 1000).toLocaleDateString('zh-CN', { year: 'numeric', month: 'long', day: 'numeric' })
    : null,
)

// ---- 集中编辑弹层 ---- */
function openEdit() {
  editOpen.value = true
}
/** 保存成功后：同步顶栏标题、刷新歌单列表与当前内容 */
async function onEdited(name?: string) {
  if (name != null && playlistId.value != null && current.value.playlistId === playlistId.value) {
    current.value = { ...current.value, playlistName: name }
  }
  await Promise.all([load(), library.loadPlaylists()])
}
/** 删除成功后：返回全部歌曲页 */
function onDeleted() {
  nav.go({ view: 'tracks' })
}

// ---- 歌单多选批量操作 ----
const batchMode = ref(false)
const selIds = ref<number[]>([])
const selTracks = computed(() => tracks.value.filter((t) => selIds.value.includes(t.id)))

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
function batchPlay() {
  if (selTracks.value.length) {
    player.playList(selTracks.value, 0)
    exitBatch()
  }
}
function batchEnqueue() {
  if (selTracks.value.length) {
    selTracks.value.forEach((t) => player.enqueue(t))
    toast(`已加入队列（${selTracks.value.length} 首）`)
    exitBatch()
  }
}
async function batchRemove() {
  const id = playlistId.value
  if (!selIds.value.length || id == null) return
  try {
    await library.removeTracksFromPlaylist(id, selIds.value)
    toast(`已移除 ${selIds.value.length} 首`)
    exitBatch()
    await load()
  } catch (e) {
    toast(String(e), 'error')
  }
}

async function load() {
  const id = playlistId.value
  if (id == null) return
  loading.value = true
  try {
    tracks.value = await api.playlistGetItems(id)
    // 歌单封面 = 最新加入歌曲的专辑封面
    coverAlbumId.value = await api.playlistCover(id)
  } catch (e) {
    toast(String(e), 'error')
  } finally {
    loading.value = false
  }
}

onMounted(load)
watch(playlistId, load)

function playAll() {
  if (tracks.value.length) player.playList(tracks.value, 0)
}

/** 选歌弹层添加成功后：刷新列表、封面与侧栏计数 */
async function onPickerAdded() {
  await Promise.all([load(), library.loadPlaylists()])
}
</script>

<template>
  <div ref="root" class="flex h-full min-h-0 flex-col">
    <div class="flex shrink-0 items-center gap-5 px-6 pt-5 pb-4">
      <CoverImg :album-id="coverAlbumId" rounded="h-20 w-20 shrink-0 rounded-xl shadow-md" />
      <div class="min-w-0 flex-1">
        <p data-stagger class="text-xs font-semibold tracking-wider text-violet-500 uppercase">歌单</p>
        <h1 data-stagger class="mt-0.5 truncate text-2xl font-bold text-zinc-900 dark:text-zinc-50">
          {{ playlistName }}
        </h1>
        <p data-stagger class="mt-1 text-xs text-zinc-400">
          {{ tracks.length }} 首{{ createdText ? ` · ${createdText}` : '' }} · 按加入时间倒序
        </p>
        <!-- 简介（只读展示，编辑统一在弹层） -->
        <p v-if="metaDesc" data-stagger class="mt-1.5 max-w-md truncate text-xs text-zinc-500 dark:text-zinc-400">
          {{ metaDesc }}
        </p>
      </div>
      <div class="flex shrink-0 items-center gap-2">
        <BaseButton
          data-stagger
          variant="outline"
          size="sm"
          :icon="Pencil"
          title="编辑歌单信息（名称、简介、删除）"
          @click="openEdit"
        >
          编辑
        </BaseButton>
        <BaseButton
          data-stagger
          v-if="tracks.length"
          :variant="batchMode ? 'primary' : 'outline'"
          size="sm"
          :icon="ListChecks"
          @click="batchMode ? exitBatch() : enterBatch()"
        >
          {{ batchMode ? '退出多选' : '多选' }}
        </BaseButton>
        <BaseButton
          data-stagger
          variant="outline"
          size="sm"
          :icon="Plus"
          @click="pickerOpen = true"
        >
          添加歌曲
        </BaseButton>
        <BaseButton
          v-if="tracks.length"
          data-stagger
          variant="primary"
          size="sm"
          :icon="Play"
          @click="playAll"
        >
          播放全部
        </BaseButton>
      </div>
    </div>

    <div v-if="loading" class="flex min-h-0 flex-1 items-center justify-center">
      <LoaderCircle class="h-6 w-6 animate-spin text-violet-500" />
    </div>

    <div v-else-if="!tracks.length" class="min-h-0 flex-1">
      <EmptyState
        :icon="ListMusic"
        title="歌单还是空的"
        description="点击右上角「添加歌曲」选择歌曲加入，或在任意歌曲上右键 →「加入歌单」。"
      />
    </div>

    <div v-else class="min-h-0 flex-1">
      <TrackTable
        :tracks="tracks"
        :playlist-id="playlistId ?? undefined"
        :batch-mode="batchMode"
        @selection="onSelection"
        @refresh="load"
      />
    </div>

    <TrackPicker
      v-if="pickerOpen && playlistId != null"
      :playlist-id="playlistId"
      :existing-ids="tracks.map((t) => t.id)"
      @close="pickerOpen = false"
      @added="onPickerAdded"
    />

    <PlaylistEditDialog
      v-if="editOpen && playlistId != null"
      :playlist-id="playlistId"
      @close="editOpen = false"
      @saved="onEdited"
      @deleted="onDeleted"
    />

    <!-- 批量操作条 -->
    <Transition
      enter-active-class="transition duration-200 ease-out"
      enter-from-class="opacity-0 translate-y-3"
      leave-active-class="transition duration-150 ease-in"
      leave-to-class="opacity-0 translate-y-3"
    >
      <div
        v-if="batchMode && selIds.length"
        class="fixed bottom-24 left-1/2 z-30 flex -translate-x-1/2 items-center gap-1 rounded-full border border-zinc-200 bg-white px-3 py-2 shadow-xl dark:border-zinc-700 dark:bg-zinc-800"
      >
        <span class="px-2 text-xs font-medium text-zinc-500 dark:text-zinc-400">已选 {{ selIds.length }} 首</span>
        <BaseButton variant="ghost" size="sm" :icon="Play" @click="batchPlay">播放</BaseButton>
        <BaseButton variant="ghost" size="sm" :icon="ListPlus" @click="batchEnqueue">加入队列</BaseButton>
        <BaseButton variant="ghost" tone="danger" size="sm" :icon="Trash2" @click="batchRemove">移出歌单</BaseButton>
        <BaseButton variant="ghost" size="sm" class="ml-1" @click="exitBatch">取消</BaseButton>
      </div>
    </Transition>
  </div>
</template>
