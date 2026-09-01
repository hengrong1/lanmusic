<script setup lang="ts">
import { nextTick, onMounted, ref, watch } from 'vue'
import { Disc3, Heart, History, Mic, Music, Plus, X } from '@lucide/vue'
import gsap from 'gsap'
import { useLibraryStore } from '@/stores/library'
import { useNav } from '@/composables/useNav'
import { useSidebar } from '@/composables/useSidebar'
import { toast } from '@/composables/useToast'
import { confirmDialog } from '@/composables/useConfirm'
import { IS_WIN } from '@/utils/platform'
import ContextMenu from '@/components/ContextMenu.vue'
import type { MenuItem } from '@/components/ContextMenu.vue'
import type { NavRoute } from '@/types'

const library = useLibraryStore()
const { current, go } = useNav()
const { collapsed } = useSidebar()
const navEl = ref<HTMLElement | null>(null)

watch(collapsed, () => animateSidebar())

function animateSidebar() {
  if (!navEl.value) return
  const targets = navEl.value.querySelectorAll<HTMLElement>('.sidebar-fade')
  const collapsing = collapsed.value

  if (collapsing) {
    gsap.to(targets, { opacity: 0, duration: 0.15, ease: 'power2.out' })
    gsap.to(navEl.value, { width: 60, duration: 0.3, ease: 'power3.inOut' })
  } else {
    gsap.to(navEl.value, { width: 220, duration: 0.3, ease: 'power3.inOut' })
    gsap.fromTo(targets, { opacity: 0 }, { opacity: 1, duration: 0.2, delay: 0.15, ease: 'power2.out' })
  }
}

onMounted(() => {
  if (collapsed.value && navEl.value) {
    navEl.value.querySelectorAll<HTMLElement>('.sidebar-fade').forEach((el) => (el.style.opacity = '0'))
  }
})

interface NavEntry {
  route: NavRoute
  label: string
  icon: typeof Music
  count?: () => number
}

const entries: NavEntry[] = [
  { route: { view: 'tracks' }, label: '全部歌曲', icon: Music, count: () => library.stats.tracks },
  { route: { view: 'tracks', favorites: true }, label: '我的喜欢', icon: Heart, count: () => library.stats.favorites },
  { route: { view: 'albums' }, label: '专辑', icon: Disc3, count: () => library.stats.albums },
  { route: { view: 'artists' }, label: '艺人', icon: Mic, count: () => library.stats.artists },
  { route: { view: 'tracks', recent: true }, label: '最近播放', icon: History },
]

function isActive(e: NavEntry) {
  const r = current.value
  // 专辑/艺人详情页高亮所属的父级标签
  if (e.route.view === 'albums') {
    return r.view === 'tracks' && r.albumId != null
  }
  if (e.route.view === 'artists') {
    return r.view === 'tracks' && r.artistId != null
  }
  if (e.route.view === 'tracks') {
    if (e.route.favorites) return r.view === 'tracks' && !!r.favorites && !r.search
    if (e.route.recent) return r.view === 'tracks' && !!r.recent && !r.search
    return r.view === 'tracks' && !r.albumId && !r.artistId && !r.recent && !r.favorites && !r.search
  }
  if (e.route.view === 'settings') {
    return r.view === e.route.view
  }
  return false
}

// ---- 歌单：新建 / 重命名 / 删除 ----
const editing = ref<{ id?: number; value: string } | null>(null)
const inputEl = ref<HTMLInputElement | null>(null)

function startCreate() {
  editing.value = { value: '' }
  void nextTick(() => inputEl.value?.focus())
}
function startRename(id: number, name: string) {
  editing.value = { id, value: name }
  void nextTick(() => inputEl.value?.focus())
}
async function confirmEdit() {
  const e = editing.value
  if (!e) return
  const name = e.value.trim()
  if (!name) {
    editing.value = null
    return
  }
  try {
    if (e.id != null) {
      await library.renamePlaylist(e.id, name)
      if (current.value.playlistId === e.id) current.value = { ...current.value, playlistName: name }
    } else {
      const p = await library.createPlaylist(name)
      go({ view: 'playlist', playlistId: p.id, playlistName: p.name })
    }
  } catch (err) {
    toast(String(err), 'error')
  } finally {
    editing.value = null
  }
}

const playlistMenu = ref<{ x: number; y: number; id: number; name: string } | null>(null)
const playlistMenuItems = ref<MenuItem[]>([])
function openPlaylistMenu(e: MouseEvent, p: { id: number; name: string }) {
  e.preventDefault()
  e.stopPropagation()
  playlistMenu.value = { x: e.clientX, y: e.clientY, id: p.id, name: p.name }
  playlistMenuItems.value = [
    { label: '重命名', action: () => startRename(p.id, p.name) },
    {
      label: '删除歌单',
      danger: true,
      action: () => {
        confirmDialog({
          title: '删除歌单',
          message: `确定删除歌单「${p.name}」吗？歌曲本身不会被删除。`,
          danger: true,
          confirmText: '删除',
        })
          .then(async (ok) => {
            if (!ok) return
            await library.deletePlaylist(p.id)
            if (current.value.playlistId === p.id) go({ view: 'tracks' })
          })
          .catch((err) => toast(String(err), 'error'))
      },
    },
  ]
}
</script>

<template>
  <nav
    ref="navEl"
    class="flex shrink-0 flex-col overflow-hidden border-r border-zinc-200 bg-zinc-50/80 transition-[width] duration-300 ease-[cubic-bezier(0.4,0,0.2,1)] dark:border-zinc-800 dark:bg-zinc-900/60"
    :style="{ width: collapsed ? '60px' : '240px' }"
  >
    <!-- Logo -->
    <div class="flex h-14 shrink-0 items-center gap-2 px-3" :data-tauri-drag-region="IS_WIN ? '' : undefined">
      <div
        class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-violet-500 text-white shadow-sm"
        :data-tauri-drag-region="IS_WIN ? '' : undefined"
      >
        <Music class="h-4 w-4" fill="currentColor" />
      </div>
      <span
        v-if="!collapsed"
        class="sidebar-fade flex-1 text-[15px] font-bold tracking-wide text-zinc-800 dark:text-zinc-100"
        :data-tauri-drag-region="IS_WIN ? '' : undefined"
      >LanMusic</span>
    </div>

    <div class="px-3">
      <p v-if="!collapsed" class="sidebar-fade px-2 pb-1 text-[11px] font-semibold tracking-wider text-zinc-400 uppercase dark:text-zinc-600">
        我的音乐
      </p>
      <button
        v-for="e in entries"
        :key="e.label"
        class="mb-0.5 flex w-full items-center rounded-lg py-2 text-sm transition"
        :class="[
          collapsed ? 'justify-center px-0' : 'gap-2.5 px-2.5',
          isActive(e)
            ? 'bg-violet-100 font-medium text-violet-700 dark:bg-violet-500/15 dark:text-violet-300'
            : 'text-zinc-600 hover:bg-zinc-200/60 dark:text-zinc-300 dark:hover:bg-zinc-800/60',
        ]"
        :title="e.label"
        @click="go(e.route)"
      >
        <component :is="e.icon" class="h-4 w-4 shrink-0" :class="isActive(e) ? 'text-violet-500' : 'text-zinc-400'" />
        <span v-if="!collapsed" class="sidebar-fade flex-1 text-left">{{ e.label }}</span>
        <span v-if="!collapsed && e.count" class="sidebar-fade text-xs tabular-nums text-zinc-400">{{ e.count() }}</span>
      </button>
    </div>

    <!-- 歌单 -->
    <div
      class="mt-3 min-h-0 flex-1 overflow-y-auto px-3"
      :class="{ 'pointer-events-none': collapsed }"
    >
      <div class="flex items-center justify-between px-2 pb-1">
        <p class="sidebar-fade text-[11px] font-semibold tracking-wider text-zinc-400 uppercase dark:text-zinc-600">歌单</p>
        <button
          class="sidebar-fade flex h-5 w-5 items-center justify-center rounded text-zinc-400 hover:bg-zinc-200 hover:text-zinc-600 dark:hover:bg-zinc-700"
          title="新建歌单"
          @click="startCreate"
        >
          <Plus class="h-3.5 w-3.5" />
        </button>
      </div>

      <!-- 新建/重命名输入行 -->
      <div v-if="editing" class="sidebar-fade mb-1 flex items-center gap-1 rounded-lg bg-white px-2 py-1 ring-1 ring-violet-400 dark:bg-zinc-800">
        <input
          ref="inputEl"
          v-model="editing.value"
          class="min-w-0 flex-1 bg-transparent text-sm text-zinc-800 outline-none dark:text-zinc-100"
          placeholder="歌单名称"
          @keydown.enter="confirmEdit"
          @keydown.esc="editing = null"
        />
        <button class="text-zinc-400 hover:text-zinc-600" @click="editing = null">
          <X class="h-3.5 w-3.5" />
        </button>
      </div>

      <div v-for="p in library.playlists" :key="p.id">
        <button
          v-if="editing?.id !== p.id"
          class="mb-0.5 flex w-full items-center gap-2.5 rounded-lg px-2.5 py-2 text-sm transition"
          :class="
            current.view === 'playlist' && current.playlistId === p.id
              ? 'bg-violet-100 font-medium text-violet-700 dark:bg-violet-500/15 dark:text-violet-300'
              : 'text-zinc-600 hover:bg-zinc-200/60 dark:text-zinc-300 dark:hover:bg-zinc-800/60'
          "
          @click="go({ view: 'playlist', playlistId: p.id, playlistName: p.name })"
          @contextmenu="openPlaylistMenu($event, p)"
        >
          <Music class="h-4 w-4 shrink-0 text-zinc-400" />
          <span class="sidebar-fade flex-1 truncate text-left">{{ p.name }}</span>
          <span class="sidebar-fade text-xs tabular-nums text-zinc-400">{{ p.trackCount }}</span>
        </button>
      </div>
      <p v-if="!library.playlists.length && !editing" class="sidebar-fade px-2.5 py-2 text-sm text-zinc-400 dark:text-zinc-600">
        暂无歌单，点 + 新建
      </p>
    </div>

    <ContextMenu
      v-if="playlistMenu"
      :x="playlistMenu.x"
      :y="playlistMenu.y"
      :items="playlistMenuItems"
      @close="playlistMenu = null"
    />
  </nav>
</template>
