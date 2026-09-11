<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { FolderOpenIcon as FolderOpen } from '@solar-icons/vue/linear/folder-open'
import { HeartIcon as Heart } from '@solar-icons/vue/linear/heart'
import { RefreshIcon as LoaderCircle } from '@solar-icons/vue/linear/refresh'
import { MusicNoteIcon as Music } from '@solar-icons/vue/linear/music-note'
import { EyeClosedIcon as SearchX } from '@solar-icons/vue/linear/eye-closed'
import { open as openFileDialog } from '@tauri-apps/plugin-dialog'
import TrackTable from '@/components/TrackTable.vue'
import EmptyState from '@/components/EmptyState.vue'
import { useLibraryStore } from '@/stores/library'
import { useNav } from '@/composables/useNav'
import { useStagger } from '@/composables/useStagger'
import { toast } from '@/composables/useToast'
import { BaseButton, BaseSelect } from '@/components/ui'
import type { SelectOption } from '@/components/ui'
import { useI18n } from 'vue-i18n'
import { errorText } from '@/i18n/error'

const { t } = useI18n()
const library = useLibraryStore()
const nav = useNav()
const root = ref<HTMLElement | null>(null)
useStagger(root, computed(() => library.trackPage.items.length > 0))

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
</script>

<template>
  <div ref="root" class="flex h-full min-h-0 flex-col">
    <!-- 头部 -->
    <div class="flex shrink-0 items-end justify-between px-6 pt-5 pb-4">
      <div>
        <p data-stagger class="text-xs font-semibold tracking-wider text-violet-500 uppercase">{{ header.subtitle || $t('common.result') }}</p>
        <h1 data-stagger class="mt-0.5 text-2xl font-bold text-zinc-900 dark:text-zinc-50">{{ header.title }}</h1>
      </div>
      <div class="flex items-center gap-3">
        <span v-if="library.trackPage.total" data-stagger class="text-sm text-zinc-500 whitespace-nowrap">
          {{ totalLabel }}
        </span>
        <BaseSelect
          v-if="!nav.current.value.search && !nav.current.value.recent"
          v-model="sort"
          data-stagger
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

    <div v-else-if="!library.trackPage.total" class="min-h-0 flex-1">
      <EmptyState :icon="SearchX" :title="$t('empty.noSongsMatch')" :description="$t('empty.noSongsMatchHint')" />
    </div>

    <!-- 曲目表 -->
    <div v-else class="min-h-0 flex-1">
      <TrackTable
        :tracks="library.trackPage.items"
        :sort="library.query.sort"
        @sort-change="(v: string) => library.setQuery({ sort: v })"
        @near-end="library.loadMore()"
      />
    </div>
  </div>
</template>
