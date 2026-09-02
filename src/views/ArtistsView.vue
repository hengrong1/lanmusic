<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { RefreshIcon as LoaderCircle } from '@solar-icons/vue/linear/refresh'
import { MicrophoneIcon as Mic } from '@solar-icons/vue/linear/microphone'
import EmptyState from '@/components/EmptyState.vue'
import { useLibraryStore } from '@/stores/library'
import { useNav } from '@/composables/useNav'
import { useStagger } from '@/composables/useStagger'
import { api } from '@/api/commands'
import type { ArtistItem } from '@/types'

const library = useLibraryStore()
const nav = useNav()

const artists = ref<ArtistItem[]>([])
const total = ref(0)
const loading = ref(false)

const root = ref<HTMLElement | null>(null)
useStagger(root, computed(() => artists.value.length > 0))

async function load() {
  loading.value = true
  try {
    const page = await api.queryArtists(undefined, 0, 1000)
    artists.value = page.items
    total.value = page.total
  } finally {
    loading.value = false
  }
}

onMounted(load)
watch(
  () => library.stats.artists,
  () => void load(),
)

function initial(name: string) {
  return name.trim().charAt(0).toUpperCase() || '?'
}
</script>

<template>
  <div ref="root" class="h-full overflow-y-auto px-6 pt-5 pb-8">
    <div class="mb-4 flex items-end justify-between">
      <div>
        <p data-stagger class="text-xs font-semibold tracking-wider text-violet-500 uppercase">我的音乐</p>
        <h1 data-stagger class="mt-0.5 text-2xl font-bold text-zinc-900 dark:text-zinc-50">艺人</h1>
      </div>
      <span v-if="total" data-stagger class="text-sm text-zinc-500">{{ total.toLocaleString() }} 位</span>
    </div>

    <div v-if="loading && !artists.length" class="flex h-64 items-center justify-center">
      <LoaderCircle class="h-6 w-6 animate-spin text-violet-500" />
    </div>

    <EmptyState
      v-else-if="!artists.length"
      :icon="Mic"
      title="还没有艺人"
      description="添加音乐文件夹并完成扫描后会显示在这里。"
    />

    <div v-else class="grid gap-4" style="grid-template-columns: repeat(auto-fill, minmax(130px, 1fr))">
      <div
        v-for="a in artists"
        :key="a.id"
        class="group flex cursor-pointer flex-col items-center gap-2 rounded-xl p-3 transition hover:bg-zinc-100 dark:hover:bg-zinc-800/60"
        @click="nav.go({ view: 'tracks', artistId: a.id, artistName: a.name })"
      >
        <div class="flex h-24 w-24 items-center justify-center overflow-hidden rounded-full bg-gradient-to-br from-violet-400 to-indigo-500 text-white shadow-sm transition group-hover:shadow-md">
          <span class="text-2xl font-bold">{{ initial(a.name) }}</span>
        </div>
        <p class="max-w-full truncate text-sm font-medium text-zinc-800 dark:text-zinc-100" :title="a.name">
          {{ a.name }}
        </p>
        <p class="text-xs text-zinc-500">{{ a.trackCount }} 首</p>
      </div>
    </div>
  </div>
</template>
