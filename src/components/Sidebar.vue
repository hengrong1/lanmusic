<script setup lang="ts">
import { nextTick, onMounted, ref, watch } from 'vue'
import { VinylRecordIcon as Disc3 } from '@solar-icons/vue/linear/vinyl-record'
import { HeartIcon as Heart } from '@solar-icons/vue/linear/heart'
import { HistoryIcon as History } from '@solar-icons/vue/linear/history'
import { MicrophoneIcon as Mic } from '@solar-icons/vue/linear/microphone'
import { SoundwaveIcon as Soundwave } from '@solar-icons/vue/linear/soundwave'
import { MusicNoteIcon as Music } from '@solar-icons/vue/linear/music-note'
import { AddIcon as Plus } from '@solar-icons/vue/linear/add'
import { CloseIcon as X } from '@solar-icons/vue/linear/close'
import gsap from 'gsap'
import logo from '@/assets/logo.png'
import { useLibraryStore } from '@/stores/library'
import { useNav } from '@/composables/useNav'
import { useSidebar } from '@/composables/useSidebar'
import { toast } from '@/composables/useToast'
import { confirmDialog } from '@/composables/useConfirm'
import { IS_WIN } from '@/utils/platform'
import ContextMenu from '@/components/ContextMenu.vue'
import Tooltip from '@/components/Tooltip.vue'
import CoverImg from '@/components/CoverImg.vue'
import type { MenuItem } from '@/components/ContextMenu.vue'
import type { NavRoute } from '@/types'

// 侧栏宽度（模板与 GSAP 动画共用，避免两处数值不一致互相覆盖）
const W_EXPANDED = 240
const W_COLLAPSED = 60
// 收起态图标视觉尺寸 = 基础 16px 的 1.25 倍（即原来的 h-5 = 20px）；
// 基础占位固定为 h-4 w-4，视觉缩放由 GSAP transform 控制，与宽度动画统一调度更顺滑
const ICON_SCALE_COLLAPSED = 20 / 16

const library = useLibraryStore()
const { current, go } = useNav()
const { collapsed } = useSidebar()
const navEl = ref<HTMLElement | null>(null)

// 文字元素是否渲染：收起时延迟到淡出动画结束再移除，展开时立即渲染再淡入，
// 避免 v-if 随 collapsed 瞬时增删导致淡入淡出失效
const showText = ref(!collapsed.value)

watch(collapsed, () => animateSidebar())

function animateSidebar() {
  if (!navEl.value) return
  const collapsing = collapsed.value

  if (collapsing) {
    // 先淡出文字，宽度收拢完成后（文字已被裁切不可见）再从 DOM 移除
    gsap.to(navEl.value.querySelectorAll<HTMLElement>('.sidebar-fade'), {
      opacity: 0,
      duration: 0.15,
      ease: 'power2.out',
      overwrite: 'auto',
    })
    // 图标在宽度收拢后段放大（16px → 20px），避开文字/布局位移的前段，观感更从容
    gsap.to(navEl.value.querySelectorAll<HTMLElement>('.nav-icon'), {
      scale: ICON_SCALE_COLLAPSED,
      duration: 0.2,
      delay: 0.12,
      ease: 'power3.out',
      overwrite: 'auto',
    })
    gsap.to(navEl.value, {
      width: W_COLLAPSED,
      duration: 0.3,
      ease: 'power3.inOut',
      overwrite: 'auto',
      onComplete: () => (showText.value = false),
    })
  } else {
    showText.value = true
    void nextTick(() => {
      const els = navEl.value?.querySelectorAll<HTMLElement>('.sidebar-fade')
      if (els?.length) {
        gsap.fromTo(
          els,
          { opacity: 0 },
          { opacity: 1, duration: 0.2, delay: 0.12, ease: 'power2.out', overwrite: 'auto' },
        )
      }
    })
    // 图标在宽度展开前段缩小回 16px，与文字淡入同向衔接
    gsap.to(navEl.value.querySelectorAll<HTMLElement>('.nav-icon'), {
      scale: 1,
      duration: 0.25,
      delay: 0.05,
      ease: 'power3.out',
      overwrite: 'auto',
    })
    gsap.to(navEl.value, { width: W_EXPANDED, duration: 0.3, ease: 'power3.inOut', overwrite: 'auto' })
  }
}

onMounted(() => {
  if (!navEl.value) return
  // 宽度由 GSAP 独占控制（不绑定响应式 style，避免 Vue 补丁瞬间置终值吞掉动画），
  // 这里只负责按初始状态设置一次起点宽度
  navEl.value.style.width = `${collapsed.value ? W_COLLAPSED : W_EXPANDED}px`
  // 初始即为收起态：常驻的文字元素（歌单名等无 v-if 的）直接置为透明
  if (collapsed.value) {
    navEl.value.querySelectorAll<HTMLElement>('.sidebar-fade').forEach((el) => (el.style.opacity = '0'))
    // 图标直接定格放大尺寸（无动画），避免首帧闪现 16px
    gsap.set(navEl.value.querySelectorAll<HTMLElement>('.nav-icon'), { scale: ICON_SCALE_COLLAPSED })
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
  { route: { view: 'genres' }, label: '风格', icon: Soundwave, count: () => library.stats.genres },
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
  if (e.route.view === 'genres') {
    return r.view === 'tracks' && !!r.genre
  }
  if (e.route.view === 'tracks') {
    if (e.route.favorites) return r.view === 'tracks' && !!r.favorites && !r.search
    if (e.route.recent) return r.view === 'tracks' && !!r.recent && !r.search
    return r.view === 'tracks' && !r.albumId && !r.artistId && !r.recent && !r.favorites && !r.genre && !r.search
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
    class="flex shrink-0 flex-col overflow-hidden bg-zinc-100 dark:bg-zinc-900"
  >
    <!-- Logo：固定左内边距 14px，收起态（60px）恰好居中，避免随 collapsed 切换 justify 而左右闪动 -->
    <div class="flex h-14 shrink-0 items-center gap-2 pl-3.5 pr-3" :data-tauri-drag-region="IS_WIN ? '' : undefined">
      <img
        :src="logo"
        alt="LanMusic"
        class="h-8 w-8 shrink-0 rounded-lg shadow-sm"
        :data-tauri-drag-region="IS_WIN ? '' : undefined"
      />
      <span
        v-if="showText"
        class="sidebar-fade flex-1 text-[15px] font-bold tracking-wide text-zinc-800 dark:text-zinc-100"
        :data-tauri-drag-region="IS_WIN ? '' : undefined"
      >LanMusic</span>
    </div>

    <div class="px-3">
      <!-- 分组标题常驻渲染（仅参与淡入淡出）：避免 showText 切换时高度增减推挤下方图标 -->
      <p class="sidebar-fade px-2 pb-1 text-[11px] font-semibold tracking-wider text-zinc-400 uppercase dark:text-zinc-600">
        我的音乐
      </p>
      <Tooltip
        v-for="e in entries"
        :key="e.label"
        :text="e.label"
        :disabled="!collapsed"
      >
        <button
          class="mb-0.5 flex h-9 w-full shrink-0 cursor-pointer items-center rounded-lg text-sm transition"
          :class="[
            showText ? 'gap-2.5 px-2.5' : 'justify-center px-0',
            isActive(e)
              ? 'bg-violet-100 font-medium text-violet-700 dark:bg-violet-500/15 dark:text-violet-300'
              : 'text-zinc-600 hover:bg-zinc-200/60 dark:text-zinc-300 dark:hover:bg-zinc-800/60',
          ]"
          @click="go(e.route)"
        >
          <component
            :is="e.icon"
            class="nav-icon h-4 w-4 shrink-0"
            :class="isActive(e) ? 'text-violet-500' : 'text-zinc-400'"
          />
          <span v-if="showText" class="sidebar-fade flex-1 text-left">{{ e.label }}</span>
          <span v-if="showText && e.count" class="sidebar-fade text-xs tabular-nums text-zinc-400">{{ e.count() }}</span>
        </button>
      </Tooltip>
    </div>

    <!-- 歌单：收起态仍可点击图标进入歌单（悬停有高亮 + title 提示） -->
    <div class="mt-3 min-h-0 flex-1 overflow-y-auto px-3">
      <div class="flex items-center justify-between px-2 pb-1">
        <p class="sidebar-fade text-[11px] font-semibold tracking-wider text-zinc-400 uppercase dark:text-zinc-600">歌单</p>
        <button
          class="sidebar-fade flex h-5 w-5 cursor-pointer items-center justify-center rounded text-zinc-400 hover:bg-zinc-200 hover:text-zinc-600 disabled:cursor-default dark:hover:bg-zinc-700 dark:hover:text-zinc-300"
          title="新建歌单"
          :disabled="collapsed"
          @click="startCreate"
        >
          <Plus class="h-3.5 w-3.5" />
        </button>
      </div>

      <!-- 新建/重命名输入行（随 editing 常驻，避免收起结束时高度增减推挤下方歌单项） -->
      <div
        v-if="editing"
        class="sidebar-fade mb-1 flex items-center gap-1 rounded-lg bg-white px-2 py-1 ring-1 ring-violet-400 dark:bg-zinc-800"
        :class="{ 'pointer-events-none': collapsed }"
      >
        <input
          ref="inputEl"
          v-model="editing.value"
          class="min-w-0 flex-1 bg-transparent text-sm text-zinc-800 outline-none dark:text-zinc-100"
          placeholder="歌单名称"
          @keydown.enter="confirmEdit"
          @keydown.esc="editing = null"
        />
        <button class="cursor-pointer text-zinc-400 hover:text-zinc-600" @click="editing = null">
          <X class="h-3.5 w-3.5" />
        </button>
      </div>

      <div v-for="p in library.playlists" :key="p.id">
        <Tooltip :text="p.name" :disabled="!collapsed">
          <button
            v-if="editing?.id !== p.id"
            class="mb-0.5 flex h-9 w-full cursor-pointer items-center rounded-lg text-sm transition"
            :class="[
              showText ? 'gap-2.5 px-2.5' : 'justify-center px-0',
              current.view === 'playlist' && current.playlistId === p.id
                ? 'bg-violet-100 font-medium text-violet-700 dark:bg-violet-500/15 dark:text-violet-300'
                : 'text-zinc-600 hover:bg-zinc-200/60 dark:text-zinc-300 dark:hover:bg-zinc-800/60',
            ]"
            @click="go({ view: 'playlist', playlistId: p.id, playlistName: p.name })"
            @contextmenu="openPlaylistMenu($event, p)"
          >
            <CoverImg
              class="nav-icon h-4 w-4 shrink-0 overflow-hidden"
              :album-id="p.coverAlbumId"
              rounded="rounded"
            />
            <span v-if="showText" class="sidebar-fade flex-1 truncate text-left">{{ p.name }}</span>
            <span v-if="showText" class="sidebar-fade text-xs tabular-nums text-zinc-400">{{ p.trackCount }}</span>
          </button>
        </Tooltip>
      </div>
      <p v-if="showText && !library.playlists.length && !editing" class="sidebar-fade px-2.5 py-2 text-sm text-zinc-400 dark:text-zinc-600">
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

<style scoped>
/* 展开动画期间容器仍较窄：文字禁止换行（避免竖排），超出部分被 nav 的 overflow-hidden 裁切 */
.sidebar-fade {
  white-space: nowrap;
}
/* 图标缩放动画：SVG 以自身视觉中心为原点缩放（HTML 元素同样生效），避免从左上角偏移 */
.nav-icon {
  transform-box: fill-box;
  transform-origin: center center;
}
</style>
