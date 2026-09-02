<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { FolderOpen, Heart, LoaderCircle, Music, SearchX } from '@lucide/vue'
import { open as openFileDialog } from '@tauri-apps/plugin-dialog'
import TrackTable from '@/components/TrackTable.vue'
import EmptyState from '@/components/EmptyState.vue'
import { useLibraryStore } from '@/stores/library'
import { useNav } from '@/composables/useNav'
import { useStagger } from '@/composables/useStagger'
import { toast } from '@/composables/useToast'

const library = useLibraryStore()
const nav = useNav()
const root = ref<HTMLElement | null>(null)
useStagger(root, computed(() => library.trackPage.items.length > 0))

const sortOptions = [
  { value: 'title', label: '按标题' },
  { value: 'album', label: '按专辑' },
  { value: 'artist', label: '按艺人' },
  { value: 'added', label: '按添加时间' },
  { value: 'duration', label: '按时长' },
]

const header = computed(() => {
  const r = nav.current.value
  if (r.search) return { title: `搜索：${r.search}`, subtitle: '' }
  if (r.albumId) return { title: r.albumTitle ?? '专辑', subtitle: '专辑' }
  if (r.artistId) return { title: r.artistName ?? '艺人', subtitle: '艺人' }
  if (r.favorites) return { title: '我的喜欢', subtitle: '我的音乐' }
  if (r.recent) return { title: '最近播放', subtitle: '我的音乐' }
  return { title: '全部歌曲', subtitle: '我的音乐' }
})

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
    toast(String(e), 'error')
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
        <p data-stagger class="text-xs font-semibold tracking-wider text-violet-500 uppercase">{{ header.subtitle || '结果' }}</p>
        <h1 data-stagger class="mt-0.5 text-2xl font-bold text-zinc-900 dark:text-zinc-50">{{ header.title }}</h1>
      </div>
      <div class="flex items-center gap-3">
        <span v-if="library.trackPage.total" data-stagger class="text-sm text-zinc-500">
          {{ library.trackPage.total.toLocaleString() }} 首
        </span>
        <select
          v-if="!nav.current.value.search && !nav.current.value.recent"
          v-model="sort"
          data-stagger
          class="h-8 cursor-pointer rounded-lg border border-zinc-200 bg-white px-2 text-sm text-zinc-600 outline-none dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"
        >
          <option value="none">入库顺序</option>
          <option v-for="o in sortOptions" :key="o.value" :value="o.value">{{ o.label }}</option>
        </select>
      </div>
    </div>

    <!-- 空状态 -->
    <div v-if="!library.hasSource && !library.trackPage.total" class="min-h-0 flex-1">
      <EmptyState
        :icon="Music"
        title="音乐库还是空的"
        description="添加本地文件夹、连接局域网设备或 NAS 的 WebDAV 目录，歌曲会自动入库。"
      >
        <button
          class="mt-2 flex cursor-pointer items-center gap-2 rounded-full bg-violet-500 px-5 py-2.5 text-sm font-medium text-white shadow transition hover:bg-violet-400 disabled:cursor-not-allowed disabled:opacity-40"
          :disabled="adding"
          @click="addFolder"
        >
          <LoaderCircle v-if="adding" class="h-4 w-4 animate-spin" />
          <FolderOpen v-else class="h-4 w-4" />
          添加音乐文件夹
        </button>
      </EmptyState>
    </div>

    <div v-else-if="!library.trackPage.total && library.loading" class="flex min-h-0 flex-1 items-center justify-center">
      <LoaderCircle class="h-6 w-6 animate-spin text-violet-500" />
    </div>

    <div v-else-if="nav.current.value.favorites && !library.trackPage.total" class="min-h-0 flex-1">
      <EmptyState
        :icon="Heart"
        title="还没有喜欢的歌曲"
        description="在歌曲上右键选择「喜欢」，或点击播放条上的爱心。"
      />
    </div>

    <div v-else-if="!library.trackPage.total" class="min-h-0 flex-1">
      <EmptyState :icon="SearchX" title="没有找到匹配的歌曲" description="换个关键词试试。" />
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
