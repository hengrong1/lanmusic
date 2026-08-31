<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { Disc3, LoaderCircle } from 'lucide-vue-next'
import AlbumGrid from '@/components/AlbumGrid.vue'
import EmptyState from '@/components/EmptyState.vue'
import { useLibraryStore } from '@/stores/library'
import { usePlayerStore } from '@/stores/player'
import { useNav } from '@/composables/useNav'
import { useStagger } from '@/composables/useStagger'
import { toast } from '@/composables/useToast'
import { api } from '@/api/commands'
import type { AlbumItem } from '@/types'

const library = useLibraryStore()
const player = usePlayerStore()
const nav = useNav()

const albums = ref<AlbumItem[]>([])
const total = ref(0)
const loading = ref(false)
const root = ref<HTMLElement | null>(null)
useStagger(root, computed(() => albums.value.length > 0))

async function load() {
  loading.value = true
  try {
    const page = await api.queryAlbums(undefined, 0, 500)
    albums.value = page.items
    total.value = page.total
  } catch (e) {
    toast(String(e), 'error')
  } finally {
    loading.value = false
  }
}

onMounted(load)
watch(
  () => library.stats.albums,
  () => void load(),
)

async function playAlbum(album: AlbumItem) {
  try {
    const page = await api.queryTracks({ view: 'album', refId: album.id, sort: 'album', page: 0, pageSize: 2000 })
    if (page.items.length) player.playList(page.items, 0)
  } catch (e) {
    toast(String(e), 'error')
  }
}
</script>

<template>
  <div ref="root" class="h-full overflow-y-auto px-6 pt-5 pb-8">
    <div class="mb-4 flex items-end justify-between">
      <div>
        <p data-stagger class="text-xs font-semibold tracking-wider text-violet-500 uppercase">我的音乐</p>
        <h1 data-stagger class="mt-0.5 text-2xl font-bold text-zinc-900 dark:text-zinc-50">专辑</h1>
      </div>
      <span v-if="total" data-stagger class="text-sm text-zinc-500">{{ total.toLocaleString() }} 张</span>
    </div>

    <div v-if="loading && !albums.length" class="flex h-64 items-center justify-center">
      <LoaderCircle class="h-6 w-6 animate-spin text-violet-500" />
    </div>

    <EmptyState
      v-else-if="!albums.length"
      :icon="Disc3"
      title="还没有专辑"
      description="添加音乐文件夹并完成扫描后会显示在这里。"
    />

    <div v-else ref="gridEl">
      <AlbumGrid
        :albums="albums"
        @open="(a) => nav.go({ view: 'tracks', albumId: a.id, albumTitle: a.title })"
        @play="playAlbum"
      />
    </div>
  </div>
</template>
