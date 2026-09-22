<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import { HistoryIcon as HistoryBold } from '@solar-icons/vue/bold/history'
import { GraphUpIcon as GraphUpBold } from '@solar-icons/vue/bold/graph-up'
import { HeartIcon as HeartBold } from '@solar-icons/vue/bold/heart'
import { MicrophoneIcon as MicBold } from '@solar-icons/vue/bold/microphone'
import { MusicNoteIcon as MusicBold } from '@solar-icons/vue/bold/music-note'
import { VinylRecordIcon as Disc3Bold } from '@solar-icons/vue/bold/vinyl-record'
import { VinylRecordIcon as Disc3 } from '@solar-icons/vue/linear/vinyl-record'
import { HeartIcon as Heart } from '@solar-icons/vue/linear/heart'
import { HistoryIcon as History } from '@solar-icons/vue/linear/history'
import { GraphUpIcon as GraphUp } from '@solar-icons/vue/linear/graph-up'
import { MicrophoneIcon as Mic } from '@solar-icons/vue/linear/microphone'
import { MusicNoteIcon as Music } from '@solar-icons/vue/linear/music-note'
import { AddIcon as Plus } from '@solar-icons/vue/linear/add'
import { useStatsEntry } from '@/composables/useStatsEntry'
import { useI18n } from 'vue-i18n'
import gsap from 'gsap'
import logo from '@/assets/logo.png'
import { useLibraryStore } from '@/stores/library'
import { useNav } from '@/composables/useNav'
import { useSidebar } from '@/composables/useSidebar'
import { toast } from '@/composables/useToast'
import { confirmDialog } from '@/composables/useConfirm'
import { IS_WIN } from '@/utils/platform'
import ContextMenu from '@/components/ContextMenu.vue'
import CoverImg from '@/components/CoverImg.vue'
import PlaylistEditDialog from '@/components/PlaylistEditDialog.vue'
import type { MenuItem } from '@/components/ContextMenu.vue'
import type { NavRoute } from '@/types'
import { errorText } from '@/i18n/error'

// 侧栏宽度（模板与 GSAP 动画共用，避免两处数值不一致互相覆盖）
const W_EXPANDED = 240
const W_COLLAPSED = 60
// 收起态图标视觉尺寸 = 基础 16px 的 1.25 倍（即原来的 h-5 = 20px）；
// 基础占位固定为 h-4 w-4，视觉缩放由 GSAP transform 控制，与宽度动画统一调度更顺滑
const ICON_SCALE_COLLAPSED = 20 / 16
// 歌单封面的横向补偿量（px）：展开态封面缩进按钮 px-2（按钮内容宽 216px 时左缘 8、中心 24），
// 折叠态 justify-center 后居中（按钮内容宽 36px 时左缘 2、中心 18）——showText 瞬时切换布局
// 会让 32px 封面瞬跳 6px。用 GSAP 把这 6px 融进宽度动画（.playlist-cover 的 x 补偿，见下）
const COVER_SHIFT = 6

const { t } = useI18n()
/** 带参翻译在 setup 内生成（模板 `$t` 无带参重载） */
const songCount = (n: number) => t('common.songsCount', { count: n })
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
  const covers = () => navEl.value?.querySelectorAll<HTMLElement>('.playlist-cover')

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
    // 封面横向补偿：随宽度收拢平滑滑向折叠态居中位。结束后 showText 翻转会令布局瞬跳 +6px
    // （左对齐→居中），等 Vue 打完补丁的同一帧把 x 归零抵消，视觉无感
    gsap.to(covers() ?? [], {
      x: -COVER_SHIFT,
      duration: 0.3,
      ease: 'power3.inOut',
      overwrite: 'auto',
      onComplete: () => void nextTick(() => gsap.set(covers() ?? [], { x: 0 })),
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
      // 布局此刻已切回展开态（封面缩进回到 8px）：先瞬移 -6px 保持与折叠态视觉连续，
      // 再随宽度展开平滑滑回 0，消掉展开方向的同源跳变
      const coverEls = covers()
      if (coverEls?.length) {
        gsap.set(coverEls, { x: -COVER_SHIFT })
        gsap.to(coverEls, { x: 0, duration: 0.3, ease: 'power3.inOut', overwrite: 'auto' })
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
  /** 选中态使用的 bold 图标（与 icon 同形不同粗细） */
  iconActive: typeof MusicBold
  count?: () => number
  /** 喜欢入口：图标悬停/选中固定红色系，不跟随主题色 */
  heart?: boolean
}

const { statsEnabled } = useStatsEntry()

const entries = computed<NavEntry[]>(() => [
  { route: { view: 'tracks' }, label: t('library.allTracks'), icon: Music, iconActive: MusicBold, count: () => library.stats.tracks },
  { route: { view: 'tracks', favorites: true }, label: t('library.myFavorites'), icon: Heart, iconActive: HeartBold, count: () => library.stats.favorites, heart: true },
  { route: { view: 'albums' }, label: t('library.albums'), icon: Disc3, iconActive: Disc3Bold, count: () => library.stats.albums },
  { route: { view: 'artists' }, label: t('library.artists'), icon: Mic, iconActive: MicBold, count: () => library.stats.artists },
  { route: { view: 'tracks', recent: true }, label: t('nav.recent'), icon: History, iconActive: HistoryBold },
  // 听歌统计入口默认隐藏（设置 → 通用可开启）；统计流水始终在记录，开关只控制入口显示
  ...(statsEnabled.value ? [{ route: { view: 'stats' as const }, label: t('nav.stats'), icon: GraphUp, iconActive: GraphUpBold }] : []),
])

/** 导航图标配色：默认跟随主题色（violet），喜欢入口固定红色系（悬停/选中红，未选中中性灰） */
function navIconClass(e: NavEntry): string {
  if (e.heart) {
    return isActive(e)
      ? 'text-red-500'
      : 'text-zinc-400 group-hover:text-red-500 dark:group-hover:text-red-400'
  }
  return isActive(e)
    ? 'text-violet-500'
    : 'text-zinc-400 group-hover:text-violet-500 dark:group-hover:text-violet-400'
}

function isActive(e: NavEntry) {
  const r = current.value
  // 专辑/艺人详情页高亮所属的父级标签
  if (e.route.view === 'albums') {
    return r.view === 'albums' || (r.view === 'tracks' && r.albumId != null)
  }
  if (e.route.view === 'artists') {
    return r.view === 'artists' || (r.view === 'tracks' && r.artistId != null)
  }
  if (e.route.view === 'tracks') {
    if (e.route.favorites) return r.view === 'tracks' && !!r.favorites && !r.search
    if (e.route.recent) return r.view === 'tracks' && !!r.recent && !r.search
    return r.view === 'tracks' && !r.albumId && !r.artistId && !r.recent && !r.favorites && !r.search
  }
  if (e.route.view === 'stats') {
    return r.view === 'stats'
  }
  if (e.route.view === 'settings') {
    return r.view === e.route.view
  }
  return false
}

// ---- 歌单：新建 / 重命名（同一个 PlaylistEditDialog 弹窗）/ 删除 ----
/** 新建歌单弹窗（PlaylistEditDialog 新建模式，无删除按钮） */
const createOpen = ref(false)
/** 重命名歌单弹窗（同一弹窗的仅重命名模式：与新建同款轻量形态，只改名称，不带简介 / 删除） */
const renameTarget = ref<{ id: number; name: string } | null>(null)

/** 新建成功：跳转到新歌单 */
function onCreated(p: { id: number; name: string }) {
  go({ view: 'playlist', playlistId: p.id, playlistName: p.name })
}
/** 重命名成功：正在看这个歌单时同步页面标题上的名字（侧栏列表由 store 自行刷新） */
function onRenamed(name?: string) {
  const target = renameTarget.value
  if (!target || !name) return
  if (current.value.playlistId === target.id) {
    current.value = { ...current.value, playlistName: name }
  }
}

const playlistMenu = ref<{ x: number; y: number; id: number; name: string } | null>(null)
const playlistMenuItems = ref<MenuItem[]>([])
function openPlaylistMenu(e: MouseEvent, p: { id: number; name: string }) {
  e.preventDefault()
  e.stopPropagation()
  playlistMenu.value = { x: e.clientX, y: e.clientY, id: p.id, name: p.name }
  playlistMenuItems.value = [
    { label: t('common.rename'), action: () => (renameTarget.value = { id: p.id, name: p.name }) },
    {
      label: t('playlist.delete'),
      danger: true,
      action: () => {
        confirmDialog({
          title: t('playlist.deleteTitle'),
          message: t('playlist.deleteMessage', { name: p.name }),
          danger: true,
          confirmText: t('common.delete'),
        })
          .then(async (ok) => {
            if (!ok) return
            await library.deletePlaylist(p.id)
            if (current.value.playlistId === p.id) go({ view: 'tracks' })
          })
          .catch((err) => toast(errorText(err), 'error'))
      },
    },
  ]
}
</script>

<template>
  <nav
    ref="navEl"
    class="app-surface-blur flex shrink-0 select-none flex-col overflow-hidden rounded-2xl bg-(--app-surface)"
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
        {{ $t('library.myMusic') }}
      </p>
      <button
        v-for="e in entries"
        :key="e.label"
        v-tooltip:right="collapsed ? e.label : ''"
        class="group mb-0.5 flex h-9 w-full shrink-0 cursor-pointer items-center rounded-lg text-sm transition"
        :class="[
          showText ? 'gap-2.5 px-2.5' : 'justify-center px-0',
          isActive(e)
            ? 'bg-violet-100 font-medium text-violet-700 dark:bg-violet-500/15 dark:text-violet-300'
            : 'hover-accent-line text-zinc-600 hover:bg-zinc-200/60 dark:text-zinc-300 dark:hover:bg-zinc-800/60',
        ]"
        @click="go(e.route)"
      >
        <component
          :is="isActive(e) ? e.iconActive : e.icon"
          class="nav-icon h-4 w-4 shrink-0 transition-colors duration-150"
          :class="navIconClass(e)"
        />
        <span v-if="showText" class="sidebar-fade flex-1 text-left">{{ e.label }}</span>
        <span v-if="showText && e.count" class="sidebar-fade text-xs tabular-nums text-zinc-400">{{ e.count() }}</span>
      </button>
    </div>

    <!-- 歌单：收起态仍可点击图标进入歌单（悬停有高亮 + title 提示） -->
    <div class="mt-3 min-h-0 flex-1 overflow-y-auto px-3">
      <div class="flex items-center justify-between px-2 pb-1">
        <p class="sidebar-fade text-[11px] font-semibold tracking-wider text-zinc-400 uppercase dark:text-zinc-600">{{ $t('playlist.title') }}</p>
        <button
          class="transition-colors duration-150 sidebar-fade flex h-5 w-5 cursor-pointer items-center justify-center rounded-lg text-zinc-400 hover:bg-zinc-200 hover:text-zinc-600 disabled:cursor-default dark:hover:bg-zinc-700 dark:hover:text-zinc-300"
          v-tooltip="$t('playlist.createNew')"
          :disabled="collapsed"
          @click="createOpen = true"
        >
          <Plus class="h-3.5 w-3.5" />
        </button>
      </div>

      <div v-for="p in library.playlists" :key="p.id">
        <button
          v-tooltip:right="p.name"
          class="group mb-1 flex h-10 w-full cursor-pointer items-center rounded-lg text-sm transition"
          :class="[
            showText ? 'gap-2.5 px-2' : 'justify-center px-0',
            current.view === 'playlist' && current.playlistId === p.id
              ? 'bg-violet-100 font-medium text-violet-700 dark:bg-violet-500/15 dark:text-violet-300'
              : 'hover-accent-line text-zinc-600 hover:bg-zinc-200/60 dark:text-zinc-300 dark:hover:bg-zinc-800/60',
          ]"
          @click="go({ view: 'playlist', playlistId: p.id, playlistName: p.name })"
          @contextmenu="openPlaylistMenu($event, p)"
        >
          <CoverImg
            class="nav-icon playlist-cover h-8 w-8 shrink-0 overflow-hidden shadow-sm"
            :album-id="p.coverAlbumId"
            rounded="rounded-md"
          />
          <span v-if="showText" class="sidebar-fade min-w-0 flex-1">
            <span class="block truncate text-left leading-tight">{{ p.name }}</span>
            <span class="block truncate text-left text-[11px] leading-tight font-normal text-zinc-400">
              {{ songCount(p.trackCount) }}
            </span>
          </span>
        </button>
      </div>
      <p v-if="showText && !library.playlists.length" class="sidebar-fade px-2.5 py-2 text-sm text-zinc-400 dark:text-zinc-600">
        {{ $t('library.noPlaylistsHint') }}
      </p>
    </div>

    <ContextMenu
      v-if="playlistMenu"
      :x="playlistMenu.x"
      :y="playlistMenu.y"
      :items="playlistMenuItems"
      @close="playlistMenu = null"
    />

    <!-- 新建 / 重命名歌单弹窗：同一组件的两种模式（重命名走 rename-only，只改名称） -->
    <PlaylistEditDialog v-if="createOpen" @close="createOpen = false" @created="onCreated" />
    <PlaylistEditDialog
      v-if="renameTarget"
      :playlist-id="renameTarget.id"
      rename-only
      @close="renameTarget = null"
      @saved="onRenamed"
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
