<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { RefreshIcon as LoaderCircle } from '@solar-icons/vue/linear/refresh'
import { SoundwaveIcon as Soundwave } from '@solar-icons/vue/linear/soundwave'
import EmptyState from '@/components/EmptyState.vue'
import { useLibraryStore } from '@/stores/library'
import { useNav } from '@/composables/useNav'
import { useStagger } from '@/composables/useStagger'
import { api } from '@/api/commands'
import type { GenreItem } from '@/types'

const library = useLibraryStore()
const nav = useNav()

const genres = ref<GenreItem[]>([])
const total = ref(0)
const loading = ref(false)

const root = ref<HTMLElement | null>(null)
useStagger(root, computed(() => genres.value.length > 0))

async function load() {
  loading.value = true
  try {
    const page = await api.queryGenres(undefined, 0, 1000)
    genres.value = page.items
    total.value = page.total
  } finally {
    loading.value = false
  }
}

onMounted(load)
watch(
  () => library.stats.genres,
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
        <h1 data-stagger class="mt-0.5 text-2xl font-bold text-zinc-900 dark:text-zinc-50">风格</h1>
      </div>
      <span v-if="total" data-stagger class="text-sm text-zinc-500">{{ total.toLocaleString() }} 类</span>
    </div>

    <div v-if="loading && !genres.length" class="flex h-64 items-center justify-center">
      <LoaderCircle class="h-6 w-6 animate-spin text-violet-500" />
    </div>

    <EmptyState
      v-else-if="!genres.length"
      :icon="Soundwave"
      title="还没有风格标签"
      description="歌曲标签中包含风格（Genre）信息时会自动归类显示。"
    />

    <div v-else class="grid gap-4" style="grid-template-columns: repeat(auto-fill, minmax(160px, 1fr))">
      <div
        v-for="g in genres"
        :key="g.name"
        class="group flex cursor-pointer items-center gap-3 rounded-xl p-3 transition hover:bg-zinc-100 dark:hover:bg-zinc-800/60"
        @click="nav.go({ view: 'tracks', genre: g.name })"
      >
        <div class="flex h-12 w-12 shrink-0 items-center justify-center rounded-xl bg-gradient-to-br from-violet-400 to-indigo-500 text-white shadow-sm transition group-hover:shadow-md">
          <span class="text-lg font-bold">{{ initial(g.name) }}</span>
        </div>
        <div class="min-w-0">
          <p class="max-w-full truncate text-sm font-medium text-zinc-800 dark:text-zinc-100" :title="g.name">
            {{ g.name }}
          </p>
          <p class="text-xs text-zinc-500">{{ g.trackCount }} 首</p>
        </div>
      </div>
    </div>
  </div>
</template>