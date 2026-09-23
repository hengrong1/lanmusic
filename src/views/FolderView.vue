<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Folder2Icon as Folder2 } from '@solar-icons/vue/linear/folder-2'
import { MusicNoteIcon as MusicNote } from '@solar-icons/vue/linear/music-note'
import { PlayIcon as Play } from '@solar-icons/vue/bold/play'
import { RefreshIcon as LoaderCircle } from '@solar-icons/vue/linear/refresh'
import TrackTable from '@/components/TrackTable.vue'
import EmptyState from '@/components/EmptyState.vue'
import { BaseButton } from '@/components/ui'
import { useNav } from '@/composables/useNav'
import { usePlayerStore } from '@/stores/player'
import { useStagger } from '@/composables/useStagger'
import { toast } from '@/composables/useToast'
import { api } from '@/api/commands'
import { errorText } from '@/i18n/error'
import type { FolderItem, Track } from '@/types'

// 文件夹视图：按来源内的目录层级逐级浏览（相对路径，'/' 分隔）。
// 面包屑点击回到任意上级；子目录列表进入更深一级；当前目录下直接存放的歌曲用表格展示。

const { t } = useI18n()
const nav = useNav()
const player = usePlayerStore()

const folders = ref<FolderItem[]>([])
const tracks = ref<Track[]>([])
const loading = ref(false)

/** 当前目录（空串 = 来源根） */
const folderPath = computed(() => nav.current.value.folderPath ?? '')
/** 面包屑分段 */
const segments = computed(() => (folderPath.value ? folderPath.value.split('/') : []))

const root = ref<HTMLElement | null>(null)
useStagger(
  root,
  computed(() => !loading.value),
)

/** 带参翻译（模板 $t 无带参重载，须在 setup 内生成） */
const songCount = (n: number) => t('common.songsCount', { count: n })

const metaLine = computed(() => {
  const parts: string[] = []
  if (folders.value.length) parts.push(t('folder.folderCount', { count: folders.value.length }))
  parts.push(t('common.songsCount', { count: tracks.value.length }))
  return parts.join(' · ')
})

/** 请求序号：快速切换目录时旧回包丢弃，避免「面包屑是 B、内容是 A」 */
let loadSeq = 0
async function load() {
  const my = ++loadSeq
  loading.value = true
  try {
    const p = folderPath.value || null
    const [dirs, files] = await Promise.all([api.queryFolders(p), api.queryTracksByFolder(p)])
    if (my !== loadSeq) return
    folders.value = dirs
    tracks.value = files
  } catch (e) {
    if (my === loadSeq) toast(errorText(e), 'error')
  } finally {
    if (my === loadSeq) loading.value = false
  }
}

onMounted(load)
watch(folderPath, load)

function enterFolder(path: string) {
  nav.go({ view: 'folder', folderPath: path })
}
function goRoot() {
  nav.go({ view: 'folder' })
}
function goSegment(i: number) {
  nav.go({ view: 'folder', folderPath: segments.value.slice(0, i + 1).join('/') })
}
function playAll() {
  if (tracks.value.length) player.playList(tracks.value, 0)
}
</script>

<template>
  <div ref="root" class="flex h-full min-h-0 flex-col">
    <div class="flex shrink-0 items-center gap-4 px-6 pt-5 pb-3">
      <div class="min-w-0 flex-1">
        <!-- 面包屑：根 → 各级目录，末级高亮 -->
        <nav data-stagger class="flex flex-wrap items-center gap-0.5 text-xs">
          <button
            type="button"
            class="rounded px-1.5 py-0.5 transition-colors hover:text-violet-500"
            :class="segments.length ? 'text-zinc-400' : 'font-medium text-zinc-700 dark:text-zinc-200'"
            @click="goRoot"
          >
            {{ $t('folder.root') }}
          </button>
          <template v-for="(seg, i) in segments" :key="i">
            <span class="text-zinc-300 dark:text-zinc-600">/</span>
            <button
              type="button"
              class="rounded px-1.5 py-0.5 transition-colors hover:text-violet-500"
              :class="i === segments.length - 1 ? 'font-medium text-zinc-700 dark:text-zinc-200' : 'text-zinc-400'"
              @click="goSegment(i)"
            >
              {{ seg }}
            </button>
          </template>
        </nav>
        <h1 data-stagger class="mt-1 truncate text-2xl font-bold text-zinc-900 dark:text-zinc-50">
          {{ segments.length ? segments[segments.length - 1] : $t('nav.folder') }}
        </h1>
        <p data-stagger class="mt-1 text-xs text-zinc-400">{{ metaLine }}</p>
      </div>
      <BaseButton
        v-if="tracks.length"
        data-stagger
        variant="primary"
        size="sm"
        :icon="Play"
        @click="playAll"
      >
        {{ $t('playlist.playAll') }}
      </BaseButton>
    </div>

    <div v-if="loading" class="flex min-h-0 flex-1 items-center justify-center">
      <LoaderCircle class="h-6 w-6 animate-spin text-violet-500" />
    </div>

    <template v-else>
      <!-- 子目录 -->
      <div v-if="folders.length" class="shrink-0 overflow-y-auto px-6 pb-3" style="max-height: 40%">
        <div class="grid grid-cols-2 gap-2 sm:grid-cols-3 xl:grid-cols-4">
          <button
            v-for="f in folders"
            :key="f.path"
            type="button"
            class="flex min-w-0 items-center gap-2.5 rounded-xl border border-zinc-200 bg-white px-3 py-2.5 text-left transition-colors hover-accent-border dark:border-zinc-800 dark:bg-zinc-900"
            @click="enterFolder(f.path)"
          >
            <Folder2 class="h-5 w-5 shrink-0 text-violet-500" />
            <span class="min-w-0 flex-1">
              <span v-tooltip="f.name" class="block truncate text-sm text-zinc-700 dark:text-zinc-200">{{ f.name }}</span>
              <span class="block text-xs text-zinc-400">{{ songCount(f.trackCount) }}</span>
            </span>
          </button>
        </div>
      </div>

      <!-- 当前目录下的歌曲 -->
      <div v-if="tracks.length" class="min-h-0 flex-1">
        <TrackTable :tracks="tracks" follow-playing />
      </div>
      <div v-else-if="!folders.length" class="min-h-0 flex-1">
        <EmptyState :icon="MusicNote" :title="$t('empty.folderTitle')" :description="$t('empty.folderHint')" />
      </div>
    </template>
  </div>
</template>
