<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { Check, LoaderCircle, Music, Search, X } from '@lucide/vue'
import type { Track } from '@/types'
import { api } from '@/api/commands'
import { useLibraryStore } from '@/stores/library'
import { toast } from '@/composables/useToast'

/**
 * 歌单选歌弹层：搜索曲库 + 多选添加。
 * - 「全部 / 已选」视图切换：歌太多时可随时切到「已选」回看勾选清单并逐个取消
 * - 「全选当前」（Ctrl/Cmd+A）/「清空已选」
 * - 已在歌单的曲目禁选并标注；仅「全部」视图滚动到底部自动加载下一页
 */
const props = defineProps<{ playlistId: number; existingIds: number[] }>()
const emit = defineEmits<{ close: []; added: [count: number] }>()

const library = useLibraryStore()

const PAGE_SIZE = 50
const keyword = ref('')
const items = ref<Track[]>([])
const total = ref(0)
const loading = ref(false)
const adding = ref(false)
const selectedTracks = ref<Track[]>([])
const filterMode = ref<'all' | 'selected'>('all')
const scroller = ref<HTMLElement | null>(null)
const inputEl = ref<HTMLInputElement | null>(null)

const existing = computed(() => new Set(props.existingIds))
const selectedIds = computed(() => new Set(selectedTracks.value.map((t) => t.id)))

/** 当前展示的列表：全部视图显示搜索结果；已选视图显示勾选清单 */
const visibleItems = computed(() =>
  filterMode.value === 'all' ? items.value : selectedTracks.value,
)
const selectableCount = computed(() => items.value.filter((t) => !existing.value.has(t.id)).length)

let page = 0
let timer: ReturnType<typeof setTimeout> | undefined

async function load(reset = false) {
  if (loading.value) return
  loading.value = true
  try {
    if (reset) {
      page = 0
      items.value = []
    }
    const p = await api.queryTracks({
      view: 'all',
      search: keyword.value.trim() || undefined,
      sort: 'title',
      page,
      pageSize: PAGE_SIZE,
    })
    total.value = p.total
    items.value = reset ? p.items : [...items.value, ...p.items]
    page += 1
  } catch (e) {
    toast(String(e), 'error')
  } finally {
    loading.value = false
  }
}

watch(keyword, () => {
  clearTimeout(timer)
  timer = setTimeout(() => void load(true), 300)
})

function onScroll() {
  if (filterMode.value !== 'all') return
  const el = scroller.value
  if (!el || loading.value || items.value.length >= total.value) return
  if (el.scrollTop + el.clientHeight >= el.scrollHeight - 60) void load()
}

function isSelected(t: Track) {
  return selectedIds.value.has(t.id)
}

function toggleTrack(t: Track) {
  if (existing.value.has(t.id)) return
  const i = selectedTracks.value.findIndex((x) => x.id === t.id)
  if (i >= 0) selectedTracks.value.splice(i, 1)
  else selectedTracks.value.push(t)
}

/** 全部视图：勾选当前已加载结果中的全部可选曲目 */
function selectAllVisible() {
  if (filterMode.value !== 'all') return
  const have = selectedIds.value
  for (const t of items.value) {
    if (!existing.value.has(t.id) && !have.has(t.id)) selectedTracks.value.push(t)
  }
}

function clearSelected() {
  selectedTracks.value = []
}

async function confirm() {
  if (!selectedTracks.value.length || adding.value) return
  adding.value = true
  try {
    const n = await library.addToPlaylist(
      props.playlistId,
      selectedTracks.value.map((t) => t.id),
    )
    if (n > 0) {
      selectedTracks.value = []
      emit('added', n)
    }
  } catch (e) {
    toast(String(e), 'error')
  } finally {
    adding.value = false
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') emit('close')
  const tgt = e.target as HTMLElement
  if (
    (e.metaKey || e.ctrlKey) &&
    e.key.toLowerCase() === 'a' &&
    tgt.tagName !== 'INPUT' &&
    tgt.tagName !== 'TEXTAREA' &&
    filterMode.value === 'all'
  ) {
    e.preventDefault()
    selectAllVisible()
  }
}

onMounted(() => {
  void load(true)
  window.addEventListener('keydown', onKeydown)
  void Promise.resolve().then(() => inputEl.value?.focus())
})
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown)
  clearTimeout(timer)
})
</script>

<template>
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6" @click.self="emit('close')">
    <div
      class="flex h-[600px] max-h-full w-full max-w-2xl flex-col overflow-hidden rounded-xl border border-zinc-200 bg-white shadow-2xl dark:border-zinc-700 dark:bg-zinc-900"
    >
      <!-- 搜索栏 -->
      <div class="flex shrink-0 items-center gap-2 border-b border-zinc-200 px-4 py-3 dark:border-zinc-800">
        <Search class="h-4 w-4 shrink-0 text-zinc-400" />
        <input
          ref="inputEl"
          v-model="keyword"
          class="h-8 min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-zinc-400"
          placeholder="搜索歌曲、艺人、专辑"
        />
        <button
          class="flex h-7 w-7 shrink-0 items-center justify-center rounded-full text-zinc-400 transition hover:bg-zinc-100 dark:hover:bg-zinc-800"
          title="关闭"
          @click="emit('close')"
        >
          <X class="h-4 w-4" />
        </button>
      </div>

      <!-- 工具条：全部/已选切换 + 全选 + 清空 -->
      <div class="flex shrink-0 items-center gap-2 border-b border-zinc-200 px-4 py-2 dark:border-zinc-800">
        <div class="flex rounded-full bg-zinc-100 p-0.5 text-sm dark:bg-zinc-800">
          <button
            class="rounded-full px-3 py-1 transition"
            :class="filterMode === 'all' ? 'bg-white font-medium text-violet-600 shadow-sm dark:bg-zinc-700 dark:text-violet-300' : 'text-zinc-500 dark:text-zinc-400'"
            @click="filterMode = 'all'"
          >
            全部
          </button>
          <button
            class="rounded-full px-3 py-1 transition"
            :class="filterMode === 'selected' ? 'bg-white font-medium text-violet-600 shadow-sm dark:bg-zinc-700 dark:text-violet-300' : 'text-zinc-500 dark:text-zinc-400'"
            @click="filterMode = 'selected'"
          >
            已选（{{ selectedTracks.length }}）
          </button>
        </div>
        <div class="flex-1" />
        <button
          class="rounded-full px-2.5 py-1 text-xs text-zinc-500 transition hover:bg-zinc-100 hover:text-violet-600 disabled:opacity-40 dark:text-zinc-400 dark:hover:bg-zinc-800"
          :disabled="filterMode !== 'all' || !selectableCount"
          title="勾选当前结果中的全部可选歌曲（Ctrl/Cmd+A）"
          @click="selectAllVisible"
        >
          全选当前
        </button>
        <button
          class="rounded-full px-2.5 py-1 text-xs text-zinc-500 transition hover:bg-zinc-100 hover:text-red-500 disabled:opacity-40 dark:text-zinc-400 dark:hover:bg-zinc-800"
          :disabled="!selectedTracks.length"
          @click="clearSelected"
        >
          清空已选
        </button>
      </div>

      <!-- 曲目列表 -->
      <div ref="scroller" class="min-h-0 flex-1 overflow-y-auto" @scroll="onScroll">
        <button
          v-for="t in visibleItems"
          :key="t.id"
          class="flex w-full items-center gap-3 px-4 py-2 text-left text-sm transition"
          :class="[
            isSelected(t) ? 'bg-violet-50 dark:bg-violet-500/10' : 'hover:bg-zinc-100/80 dark:hover:bg-zinc-800/50',
            existing.has(t.id) ? 'cursor-default opacity-45' : '',
          ]"
          @click="toggleTrack(t)"
        >
          <span
            class="flex h-4 w-4 shrink-0 items-center justify-center rounded border transition"
            :class="isSelected(t) ? 'border-violet-500 bg-violet-500 text-white' : 'border-zinc-300 dark:border-zinc-600'"
          >
            <Check v-if="isSelected(t)" class="h-3 w-3" />
          </span>
          <span class="min-w-0 flex-1 truncate text-zinc-800 dark:text-zinc-100">{{ t.title }}</span>
          <span class="w-24 shrink-0 truncate text-xs text-zinc-500 dark:text-zinc-400">{{ t.artist ?? '未知艺人' }}</span>
          <span v-if="existing.has(t.id)" class="w-20 shrink-0 text-right text-xs text-zinc-400">已在歌单</span>
          <span
            v-else-if="isSelected(t)"
            class="flex w-20 shrink-0 items-center justify-end gap-1 text-right text-xs font-medium text-violet-500"
          >
            <Check class="h-3 w-3" /> 已选
          </span>
          <span v-else class="w-20 shrink-0 text-right text-xs text-zinc-300 dark:text-zinc-600">
            {{ filterMode === 'all' ? '点击选择' : '' }}
          </span>
        </button>

        <!-- 已选视图空态 -->
        <div v-if="filterMode === 'selected' && !selectedTracks.length" class="flex flex-col items-center gap-2 py-12 text-zinc-400">
          <Check class="h-8 w-8" :stroke-width="1.5" />
          <p class="text-sm">还没有选中任何歌曲</p>
        </div>
        <!-- 全部视图 loading / 空态 -->
        <template v-else-if="filterMode === 'all'">
          <div v-if="loading" class="flex justify-center py-4">
            <LoaderCircle class="h-5 w-5 animate-spin text-violet-500" />
          </div>
          <div v-else-if="!items.length" class="flex flex-col items-center gap-2 py-12 text-zinc-400">
            <Music class="h-8 w-8" :stroke-width="1.5" />
            <p class="text-sm">没有找到匹配的歌曲</p>
          </div>
        </template>
      </div>

      <!-- 底栏 -->
      <div class="flex shrink-0 items-center justify-between border-t border-zinc-200 px-4 py-3 dark:border-zinc-800">
        <span class="text-xs text-zinc-500">
          <template v-if="selectedTracks.length">
            已选 <b class="text-violet-500">{{ selectedTracks.length }}</b> 首
          </template>
          <template v-else>请选择要添加的歌曲</template>
        </span>
        <div class="flex items-center gap-2">
          <button
            class="rounded-full px-3 py-1.5 text-sm text-zinc-500 transition hover:bg-zinc-100 dark:hover:bg-zinc-800"
            @click="emit('close')"
          >
            取消
          </button>
          <button
            class="flex items-center gap-1.5 rounded-full bg-violet-500 px-4 py-1.5 text-sm font-medium text-white transition hover:bg-violet-400 disabled:opacity-40"
            :disabled="!selectedTracks.length || adding"
            @click="confirm"
          >
            <LoaderCircle v-if="adding" class="h-4 w-4 animate-spin" />
            添加到歌单
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
