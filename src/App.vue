<script setup lang="ts">
import { computed, defineAsyncComponent, onMounted, ref, watch, type Component } from 'vue'
import gsap from 'gsap'
import Sidebar from '@/components/Sidebar.vue'
import TopBar from '@/components/TopBar.vue'
import PlayerBar from '@/components/PlayerBar.vue'
import QueuePanel from '@/components/QueuePanel.vue'
import ConfirmDialog from '@/components/ConfirmDialog.vue'
import UpdateDialog from '@/components/UpdateDialog.vue'
import NowPlayingView from '@/components/NowPlayingView.vue'
import Toast from '@/components/Toast.vue'
import MvPlayer from '@/components/MvPlayer.vue'
import { useUpdater } from '@/composables/useUpdater'
import TracksView from '@/views/TracksView.vue'
// 非默认视图懒加载：首屏只需要 TracksView，其余视图切过去时再加载（本地加载，几乎无感）。
// chunk 加载失败时用 AsyncViewError 兜底（可重试），而不是渲染空白
import AsyncViewError from '@/components/AsyncViewError.vue'
const lazyView = (loader: () => Promise<{ default: Component }>) =>
  defineAsyncComponent({ loader, errorComponent: AsyncViewError })
const AlbumsView = lazyView(() => import('@/views/AlbumsView.vue'))
const ArtistsView = lazyView(() => import('@/views/ArtistsView.vue'))
const PlaylistView = lazyView(() => import('@/views/PlaylistView.vue'))
const SettingsView = lazyView(() => import('@/views/SettingsView.vue'))
import { useLibraryStore } from '@/stores/library'
import { usePlayerStore } from '@/stores/player'
import { useNav } from '@/composables/useNav'
import { useAmbient } from '@/composables/useAmbient'
import { useSkin, useSkinOpen } from '@/composables/useSkin'
import { ensureAnalyser } from '@/composables/useSpectrum'
import { useDesktopLyrics } from '@/composables/useDesktopLyrics'
import { useTrayMenu } from '@/composables/useTrayMenu'
import { useMvPlayer } from '@/composables/useMvPlayer'
import { useThemeColor } from '@/composables/useThemeColor'
import { useBackground } from '@/composables/useBackground'
import { bgUrl } from '@/api/scheme'

const library = useLibraryStore()
const player = usePlayerStore()
const nav = useNav()
const mv = useMvPlayer()
const { palette, setAlbum } = useAmbient()
// 自定义背景：设置 → 外观选择图片（useBackground 模块级单例，设置页改后这里即时生效）
const { file: bgFile, blur: bgBlur } = useBackground()
// 有背景图时给 html 挂 has-bg：主卡片经 --app-surface 变量转毛玻璃透出底图（见 style.css）
watch(
  bgFile,
  (f) => document.documentElement.classList.toggle('has-bg', !!f),
  { immediate: true },
)

// 环境色预热：切歌后立即在后台提取专辑主色（原先等到进入播放页才提取，
// 首次进入要现拉大图 + 解码，会和进场动画撞车造成卡顿）
watch(
  () => player.current?.albumId,
  (id) => void setAlbum(id),
  { immediate: true },
)

const skin = useSkin()
// 频谱链路预热：开启频谱（皮肤）时，首次开始播放就建立 Web Audio 链路。
// 若等到进入播放页挂载频谱画布时才 createMediaElementSource，WebKit 重配置音频管线
// 会让正在播放的歌曲停顿约半秒（表现为进场动画结束后声音才恢复）；播放前重路由则无感知。
// （ensureAnalyser 内部有用户手势检查，无手势时自动推迟，不会创建出静音的挂起 context）
watch(
  [() => player.playing, () => skin.value.on],
  ([playing, on]) => {
    if (playing && on) ensureAnalyser()
  },
  { immediate: true },
)

const queueOpen = ref(false)
const nowPlaying = ref(false)

// 桌面歌词：初始化同步监听并恢复上次开启状态（主窗口内仅此一次）
useDesktopLyrics()
// 自定义主题色：恢复上次选择并覆盖 --color-violet-* 变量（模块导入时已应用，此处保持初始化口径一致）
useThemeColor()
// 系统托盘菜单：向 tray 弹窗同步播放状态并处理其系统级指令
useTrayMenu()

/** 播放页专注模式：仅当正在播放且播放页打开时启用；鼠标不在底部/顶栏控制区，5s 无移动则隐藏控制 */
const npFocus = ref(false)
const PLAYER_BAR_H = 80 // 播放条 h-20
const HEADER_H = 56 // 顶栏 h-14
const REGION_GAP = 12 // 卡片分区间距 p-3 / gap-3（与布局保持一致）
let focusTimer: ReturnType<typeof setTimeout> | undefined

/** 专注模式启用条件：播放页打开 且 正在播放（暂停时不做专注隐藏）；皮肤设置弹层 / 播放队列面板展开时暂停专注，
 * 避免用户调整皮肤或翻看队列时停留超过 5s 被触发隐藏（队列面板锚定播放条，播放条一藏面板就悬空） */
const skinOpen = useSkinOpen()
/** 专注模式开关与进入延时（设置 → 播放；默认开启、5 秒）。
 * computed 依赖播放/面板状态，设置变更在下一次状态变化后生效 */
const focusEnabled = () => localStorage.getItem('lm.focusMode') !== '0'
const focusDelayMs = () => {
  const s = Number(localStorage.getItem('lm.focusDelay'))
  return Number.isFinite(s) && s > 0 ? s * 1000 : 5000
}
const focusActive = computed(
  () => focusEnabled() && nowPlaying.value && player.playing && !skinOpen.value && !queueOpen.value,
)

/** 启动/重置专注计时（延时可在设置调整） */
function armFocusTimer() {
  clearTimeout(focusTimer)
  focusTimer = setTimeout(() => (npFocus.value = true), focusDelayMs())
}
/** 取消专注并清掉计时 */
function clearFocus() {
  npFocus.value = false
  clearTimeout(focusTimer)
}

window.addEventListener(
  'mousemove',
  (e) => {
    if (!focusActive.value) return
    // 鼠标已移出窗口范围：启动专注计时（窗口中不再有 mousemove，需在此兜底）
    if (e.clientX < 0 || e.clientX >= window.innerWidth || e.clientY < 0 || e.clientY >= window.innerHeight) {
      armFocusTimer()
      return
    }
    // 卡片化布局：播放条卡片顶部在窗口底缘上方 PLAYER_BAR_H + REGION_GAP 处，顶栏卡片下缘在 HEADER_H + REGION_GAP 处
    const inBarZone = e.clientY >= window.innerHeight - PLAYER_BAR_H - REGION_GAP
    const inHeaderZone = e.clientY <= HEADER_H + REGION_GAP
    if (inBarZone || inHeaderZone) {
      // 鼠标在底部播放条 / 顶部控制区：保持控制可见
      clearTimeout(focusTimer)
      if (npFocus.value) npFocus.value = false
      return
    }
    // 内容区移动：恢复显示并重置 5s 无操作计时
    if (npFocus.value) npFocus.value = false
    armFocusTimer()
  },
  { passive: true },
)
// 鼠标完全离开窗口（移出 document 边界）即启动 5s 计时
document.addEventListener('mouseleave', () => {
  if (focusActive.value) armFocusTimer()
})
// 窗口失焦（切到别的窗口）也当作鼠标离开
window.addEventListener('blur', () => {
  if (focusActive.value && !npFocus.value) armFocusTimer()
})
watch(focusActive, (active) => {
  if (!active) {
    clearFocus()
  } else {
    // 恢复播放/打开播放页时启动 5s 无操作计时
    armFocusTimer()
  }
})

/** 播放页环境渐变：铺满全窗（含播放条背后），歌词强调色变量也从这里下发 */
const npAccent = computed(() => palette.value?.accent ?? '#a78bfa')
const npBgStyle = computed(() => {
  const p = palette.value
  return {
    background: p
      ? `linear-gradient(to bottom, ${p.glow} 0%, ${p.deep} 55%, #09090b 100%)`
      : 'linear-gradient(to bottom, #2e1065 0%, #09090b 55%, #09090b 100%)',
  }
})

const viewComponent = computed(() => {
  switch (nav.current.value.view) {
    case 'albums':
      return AlbumsView
    case 'artists':
      return ArtistsView
    case 'playlist':
      return PlaylistView
    case 'settings':
      return SettingsView
    default:
      return TracksView
  }
})

/** 视图切换的 key：路由任一参数变化都触发过渡 */
const viewKey = computed(() => JSON.stringify(nav.current.value))

// ---- GSAP 过渡：主视图切换（简短淡入淡出，不做缩放避免文字模糊）----
function viewEnter(el: Element, done: () => void) {
  // 懒加载视图首次进入时可能先渲染注释占位节点：直接完成，避免动画挂在空目标上
  if (el.nodeType !== 1) {
    done()
    return
  }
  gsap.fromTo(
    el,
    { opacity: 0, y: 18 },
    { opacity: 1, y: 0, duration: 0.32, ease: 'power2.out', clearProps: 'all', onComplete: done },
  )
}
function viewLeave(el: Element, done: () => void) {
  gsap.to(el, { opacity: 0, y: -14, duration: 0.16, ease: 'power1.in', onComplete: done })
}

// ---- GSAP 过渡：播放页环境背景（进入淡入；退出与内容层同步下滑，全程保持不透明，避免中途透出底层视图）----
function npBgEnter(el: Element, done: () => void) {
  gsap.fromTo(el, { opacity: 0 }, { opacity: 1, duration: 0.5, ease: 'power2.out', onComplete: done })
}
function npBgLeave(el: Element, done: () => void) {
  gsap.to(el, { yPercent: 100, duration: 0.42, ease: 'power3.in', onComplete: done })
}
// ---- GSAP 过渡：播放页上滑进入 / 下滑退出 ----
function nowPlayingEnter(el: Element, done: () => void) {
  gsap.fromTo(el, { yPercent: 100 }, { yPercent: 0, duration: 0.55, ease: 'power3.out', onComplete: done })
  const root = el as HTMLElement
  gsap.from(root.querySelectorAll('.np-cover'), {
    opacity: 0,
    scale: 0.9,
    y: 30,
    duration: 0.55,
    delay: 0.22,
    ease: 'power2.out',
    clearProps: 'opacity,scale,transform',
  })
  gsap.from(root.querySelectorAll('.np-fade'), {
    opacity: 0,
    y: 24,
    duration: 0.45,
    delay: 0.3,
    stagger: 0.07,
    ease: 'power2.out',
    clearProps: 'all',
  })
}
function nowPlayingLeave(el: Element, done: () => void) {
  gsap.to(el, { yPercent: 100, duration: 0.42, ease: 'power3.in', onComplete: done })
}

/** 播放条显隐：未载入任何歌曲（从未播放且无启动恢复）时隐藏 */
const playerBarVisible = computed(() => player.current != null)
// 播放条消失时收起依附于它的浮层（播放页 / 队列面板），避免悬空显示
watch(playerBarVisible, (v) => {
  if (!v) {
    nowPlaying.value = false
    queueOpen.value = false
  }
})

// ---- GSAP 过渡：底部播放条从窗口底缘滑入 / 滑出 ----
// 同步动画 yPercent 与负 marginBottom：条滑入的同时布局高度展开（内容区不被瞬间挤压），
// 滑出时反向收回，之后组件卸载不会造成布局跳变。h-20 = 80px；收起时连卡片上方 12px 间距一起回收，
// 只剩根容器 p-3 的 12px 底边距（与 PLAYER_BAR_H / REGION_GAP 对应）。
const BAR_DISMISS = PLAYER_BAR_H + REGION_GAP
function playerBarEnter(el: Element, done: () => void) {
  gsap.fromTo(
    el,
    { yPercent: 100, marginBottom: -BAR_DISMISS },
    { yPercent: 0, marginBottom: 0, duration: 0.5, ease: 'power3.out', onComplete: done },
  )
}
function playerBarLeave(el: Element, done: () => void) {
  gsap.to(el, { yPercent: 100, marginBottom: -BAR_DISMISS, duration: 0.35, ease: 'power3.in', onComplete: done })
}

onMounted(() => {
  void library.init()
  void player.restore()
  // 启动时静默检查应用更新（有新版本弹出更新弹窗，失败不打扰）
  void useUpdater().checkForUpdate(true)
})

// 全局快捷键
window.addEventListener('keydown', (e) => {
  const target = e.target as HTMLElement
  const typing = target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable
  // MV 播放层打开时让位：快捷键交给播放器（Esc 关闭由 MvPlayer 处理）
  if (mv.opened.value) return
  if (e.key === 'Escape' && nowPlaying.value) {
    nowPlaying.value = false
    return
  }
  if (e.key === ' ' && !typing) {
    e.preventDefault()
    player.toggle()
  } else if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'f') {
    e.preventDefault()
    document.getElementById('search-input')?.focus()
  } else if (!typing && !e.metaKey && !e.ctrlKey && !e.altKey && e.key.toLowerCase() === 'n') {
    player.next()
  } else if (!typing && !e.metaKey && !e.ctrlKey && !e.altKey && e.key.toLowerCase() === 'p') {
    player.prev()
  } else if (!typing && !e.metaKey && !e.ctrlKey && !e.altKey && e.key === '[') {
    // 歌词校准：提前 0.5s（歌词显示慢了按这个）
    player.setLyricOffset(-0.5)
  } else if (!typing && !e.metaKey && !e.ctrlKey && !e.altKey && e.key === ']') {
    // 歌词校准：延后 0.5s（歌词显示快了按这个）
    player.setLyricOffset(0.5)
  }
})
</script>

<template>
  <div
    class="relative isolate flex h-screen flex-col gap-3 overflow-hidden bg-zinc-100 p-3 text-zinc-900 dark:bg-zinc-950 dark:text-zinc-100"
    :class="npFocus ? 'cursor-none [&_*]:!cursor-none' : ''"
  >
    <!-- 自定义背景图：铺满窗口底色区，卡片浮于其上（根节点 isolate 使 -z 层画在背景色之上、内容之下）。
         播放页展开时被环境渐变层（z-15）覆盖。模糊时轻微放大防止边缘晕出露底色 -->
    <div v-if="bgFile" class="pointer-events-none absolute inset-0 -z-10 overflow-hidden">
      <img
        :src="bgUrl(bgFile)"
        alt=""
        class="h-full w-full object-cover"
        :style="{
          filter: bgBlur > 0 ? `blur(${bgBlur}px)` : undefined,
          transform: bgBlur > 0 ? 'scale(1.08)' : undefined,
        }"
        draggable="false"
      />
    </div>
    <div class="flex min-h-0 flex-1 gap-3">
      <Sidebar />
      <div class="flex min-w-0 flex-1 flex-col gap-3">
        <TopBar />
        <!-- 内容卡片：白色圆角浮于灰色底框上，与侧栏/顶栏/播放条形成圆角卡片分区 -->
        <main class="app-surface-blur min-h-0 flex-1 overflow-hidden rounded-2xl bg-(--app-surface)">
          <Transition :css="false" mode="out-in" @enter="viewEnter" @leave="viewLeave">
            <component :is="viewComponent" :key="viewKey" />
          </Transition>
        </main>
      </div>
      <QueuePanel :open="queueOpen" :now-playing="nowPlaying" @close="queueOpen = false" />
    </div>
    <!-- 底部播放条卡片：未载入任何歌曲时隐藏；双击播放 / 启动恢复上一首时从底部滑入 -->
    <Transition :css="false" @enter="playerBarEnter" @leave="playerBarLeave">
      <PlayerBar
        v-if="playerBarVisible"
        :now-playing-open="nowPlaying"
        :focus-hidden="npFocus"
        @toggle-queue="queueOpen = !queueOpen"
        @toggle-now-playing="nowPlaying = !nowPlaying"
      />
    </Transition>
    <Toast />
    <ConfirmDialog />
    <UpdateDialog />
    <MvPlayer />
    <!-- 播放页环境：全窗渐变（含播放条背后）+ 上滑的内容层。z-15 低于播放条，播放条透明浮于其上 -->
    <div class="pointer-events-none absolute inset-0 z-[15] overflow-hidden" :style="{ '--np-accent': npAccent }">
      <Transition :css="false" @enter="npBgEnter" @leave="npBgLeave">
        <div v-if="nowPlaying" class="absolute inset-0" :style="npBgStyle"></div>
      </Transition>
      <!-- 内容层底缘对齐播放条卡片上沿（92px = 播放条 80px + 上方间距 12px） -->
      <Transition :css="false" @enter="nowPlayingEnter" @leave="nowPlayingLeave">
        <div v-if="nowPlaying" class="pointer-events-auto absolute inset-x-0 top-0 bottom-23 overflow-hidden">
          <NowPlayingView :focus-hidden="npFocus" @close="nowPlaying = false" />
        </div>
      </Transition>
    </div>
  </div>
</template>
