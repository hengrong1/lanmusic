<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { ListMusic, LoaderCircle, Play } from 'lucide-vue-next'
import TrackTable from '@/components/TrackTable.vue'
import EmptyState from '@/components/EmptyState.vue'
import { useLibraryStore } from '@/stores/library'
import { usePlayerStore } from '@/stores/player'
import { useNav } from '@/composables/useNav'
import { useStagger } from '@/composables/useStagger'
import { toast } from '@/composables/useToast'
import { api } from '@/api/commands'
import type { Track } from '@/types'

const library = useLibraryStore()
const player = usePlayerStore()
const nav = useNav()

const tracks = ref<Track[]>([])
const loading = ref(false)

const root = ref<HTMLElement | null>(null)
useStagger(root, computed(() => tracks.value.length > 0))

const playlistId = computed(() => nav.current.value.playlistId ?? null)
const playlistName = computed(() => nav.current.value.playlistName ?? '歌单')

async function load() {
  const id = playlistId.value
  if (id == null) return
  loading.value = true
  try {
    tracks.value = await api.playlistGetItems(id)
  } catch (e) {
    toast(String(e), 'error')
  } finally {
    loading.value = false
  }
}

onMounted(load)
watch(playlistId, load)

async function onReorder(from: number, to: number) {
  const id = playlistId.value
  if (id == null) return
  const ids = tracks.value.map((t) => t.id)
  const [moved] = ids.splice(from, 1)
  ids.splice(to, 0, moved)
  const [movedTrack] = tracks.value.splice(from, 1)
  tracks.value.splice(to, 0, movedTrack)
  try {
    await library.reorderPlaylist(id, ids)
  } catch (e) {
    toast(String(e), 'error')
    void load()
  }
}

function playAll() {
  if (tracks.value.length) player.playList(tracks.value, 0)
}
</script>

<template>
  <div ref="root" class="flex h-full min-h-0 flex-col">
    <div class="flex shrink-0 items-end justify-between px-6 pt-5 pb-4">
      <div>
        <p data-stagger class="text-xs font-semibold tracking-wider text-violet-500 uppercase">歌单</p>
        <h1 data-stagger class="mt-0.5 text-2xl font-bold text-zinc-900 dark:text-zinc-50">{{ playlistName }}</h1>
      </div>
      <button
        v-if="tracks.length"
        data-stagger
        class="flex items-center gap-2 rounded-full bg-violet-500 px-5 py-2 text-sm font-medium text-white shadow transition hover:bg-violet-400"
        @click="playAll"
      >
        <Play class="h-4 w-4" fill="currentColor" />
        播放全部
      </button>
    </div>

    <div v-if="loading" class="flex min-h-0 flex-1 items-center justify-center">
      <LoaderCircle class="h-6 w-6 animate-spin text-violet-500" />
    </div>

    <div v-else-if="!tracks.length" class="min-h-0 flex-1">
      <EmptyState
        :icon="ListMusic"
        title="歌单还是空的"
        description="在任意歌曲上右键 →「加入歌单」即可添加；拖拽行可调整顺序。"
      />
    </div>

    <div v-else class="min-h-0 flex-1">
      <TrackTable
        :tracks="tracks"
        :playlist-id="playlistId ?? undefined"
        @reorder="onReorder"
        @refresh="load"
      />
    </div>
  </div>
</template>
