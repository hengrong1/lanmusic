<script setup lang="ts">
import { computed, defineAsyncComponent, onMounted, ref, watch, type Component } from 'vue'
import gsap from 'gsap'
import Sidebar from '@/components/Sidebar.vue'
import TopBar from '@/components/TopBar.vue'
import PlayerBar from '@/components/PlayerBar.vue'
import QueuePanel from '@/components/QueuePanel.vue'
import ConfirmDialog from '@/components/ConfirmDialog.vue'
import CloseConfirmDialog from '@/components/CloseConfirmDialog.vue'
import UpdateDialog from '@/components/UpdateDialog.vue'
import NowPlayingView from '@/components/NowPlayingView.vue'
import NpAmbientBg from '@/components/NpAmbientBg.vue'
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
const StatsView = lazyView(() => import('@/views/StatsView.vue'))
const FolderView = lazyView(() => import('@/views/FolderView.vue'))
import { useLibraryStore } from '@/stores/library'
import { usePlayerStore } from '@/stores/player'
import { useNav } from '@/composables/useNav'
import { useAmbient } from '@/composables/useAmbient'
import { useSkin, useSkinOpen } from '@/composables/useSkin'
import { ensureGraph, eqEnabled, normEnabled } from '@/composables/useAudioGraph'
import { useMediaControls } from '@/composables/useMediaControls'
import { useNowPlaying } from '@/composables/useNowPlaying'
import { useDesktopLyrics } from '@/composables/useDesktopLyrics'
import { isFocusModeEnabled } from '@/composables/useFocusMode'
import { useTrayMenu } from '@/composables/useTrayMenu'
import { useMvPlayer } from '@/composables/useMvPlayer'
import { useThemeColor } from '@/composables/useThemeColor'
import { useBackground } from '@/composables/useBackground'
import { bgUrl } from '@/api/scheme'
import { api } from '@/api/commands'
import { toast } from '@/composables/useToast'
import { t as translate } from '@/i18n/translate'
import {
  SHORTCUT_DEFS,
  useShortcuts,
  useGlobalShortcuts,
  eventMatches,
  type ShortcutAction,
} from '@/composables/useShortcuts'

const library = useLibraryStore()
const player = usePlayerStore()
const nav = useNav()
const mv = useMvPlayer()
// 快捷键：应用内快捷键表（设置 → 通用 → 快捷键）；全局快捷键在此初始化（事件分发 + 启动恢复）
const shortcuts = useShortcuts()
useGlobalShortcuts()
// 系统媒体键：启动时若此前已启用则重新注册（模块单例，内部只初始化一次监听）
useMediaControls()
// 系统级「正在播放」（SMTC / Now Playing / MPRIS）：推送当前曲目与进度，响应系统控制
useNowPlaying()
const { palette, setAlbum } = useAmbient()
// 自定义背景：设置 → 外观选择图片（useBackground 模块级单例，设置页改后这里即时生效）
const { file: bgFile, blur: bgBlur, set: setBg } = useBackground()
/** 背景副本丢失自愈：副本存在 appData/backgrounds/，被清理工具删掉后 bg:// 请求 404、
    img 加载失败 → 自动关闭自定义背景并摘除 has-bg（否则玻璃样式残留而无图可透，
    亮色下白字叠白底不可读），并提示用户 */
function onBgError() {
  setBg(null)
  toast(translate('toast.bgMissing'), 'error')
}
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
// 频谱链路预热：开启频谱（皮肤）或任一音频处理特性（均衡器 / 音量归一化）时，
// 首次开始播放就建立共享 Web Audio 链路（ensureGraph 内部含用户手势检查与创建失败降级）。
// 若等到进入播放页挂载频谱画布时才 createMediaElementSource，WebKit 重配置音频管线
// 会让正在播放的歌曲停顿约半秒；播放前重路由则无感知。
watch(
  [() => player.playing, () => skin.value.on, () => eqEnabled.value, () => normEnabled.value],
  ([playing, on]) => {
    if (playing && (on || eqEnabled.value || normEnabled.value)) ensureGraph()
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
const focusEnabled = isFocusModeEnabled
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

/** 播放页环境强调色：歌词强调色变量从这里下发；渐变背景移入 NpAmbientBg（双层交叉淡入 + 光斑呼吸） */
const npAccent = computed(() => palette.value?.accent ?? '#a78bfa')

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
    case 'stats':
      return StatsView
    case 'folder':
      return FolderView
    default:
      return TracksView
  }
})

/** 视图切换的 key：路由任一参数变化都触发过渡 */
const viewKey = computed(() => JSON.stringify(nav.current.value))

// ---- 视图渲染自检（自愈保险）----
// 已知故障（2026-09-17 定位）：GSAP JS 过渡的 leave 完成回调在部分环境不触发 → out-in 状态机
// 永久挂起 → main 只剩注释占位、此后所有切换空白（视图过渡已改 CSS，见 style.css .view-*）。
// 这里保留最终保险：切换后 900ms 检查 main 首个元素，异常时记 error（含现场快照便于排查）
// **并置 forcePlainRender 绕过 Transition 直接渲染视图**，保证内容必定出现。
const mainEl = ref<HTMLElement | null>(null)
/** true = 放弃过渡动画、直接渲染视图（检测到 Transition 卡死时的自愈路径） */
const forcePlainRender = ref(false)
watch(viewKey, () => {
  window.setTimeout(() => {
    const view = nav.current.value.view
    const node = mainEl.value?.firstElementChild as HTMLElement | null
    if (node && node.nodeType === 1 && node.offsetHeight > 0 && Number(getComputedStyle(node).opacity) >= 0.05) {
      return // 正常渲染，无需干预
    }
    const cs = node && node.nodeType === 1 ? getComputedStyle(node) : null
    const snapshot =
      `h=${node?.offsetHeight ?? -1} w=${node?.offsetWidth ?? -1} op=${cs?.opacity ?? '-'} ` +
      `text=${(node?.innerText || '').length} win=${window.innerWidth}x${window.innerHeight} ` +
      `html=${document.documentElement.className} children=${mainEl.value?.childNodes.length ?? -1}`
    forcePlainRender.value = true
    if (node && node.nodeType === 1 && Number(cs?.opacity) < 0.05) {
      node.style.opacity = ''
      node.style.transform = ''
      node.style.translate = ''
    }
    api
      .frontendLog('error', `[view-guard] 视图 ${view} 未正常渲染，已切换为无过渡渲染 | ${snapshot}`)
      .catch(() => {})
  }, 900)
})

// ---- 主视图切换过渡 ----
// 已改为 **CSS 过渡**（见 style.css 的 .view-* 类）：
// 原 GSAP JS 过渡在部分环境下 leave 的完成回调不触发（实测用户机：leave 动画停在 6.6%、
// main 里只剩注释占位、且此后所有切换永久空白），out-in 状态机会被永久挂起。
// CSS 过渡由合成器驱动、由 Vue 内置的 transitionend + 超时兜底接管，不存在「回调不来」的死角。
// 另见下方 view-guard 的 forcePlainRender：万一仍检测到视图未渲染，直接绕过 Transition 渲染。

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
/**
 * 焦点是不是「鼠标点出来的」。
 * 鼠标点过的元素，随后按下任意一个键，Chromium 都会把它标成 :focus-visible（键盘聚焦态）
 * → 按钮 / 侧栏项上凭空冒出聚焦框（用户反馈「按空格会显示 focus 聚焦并暂停/播放」：
 * 点过「全部歌曲」再按空格，那一项就被套上浏览器默认聚焦框）。
 * 实测按键处理的那一刻 :focus-visible 就已经是 true（鼠标态、Tab 态都一样），处理器里分不出来，
 * 所以自己记来源：pointerdown 视为鼠标，Tab 视为键盘。纯输入控件与 body 永远不动。
 */
let focusFromPointer = false
window.addEventListener('pointerdown', () => (focusFromPointer = true), true)
window.addEventListener('keydown', (e) => e.key === 'Tab' && (focusFromPointer = false), true)

function blurShortcutFocus() {
  // Tab 导航出来的焦点本来就该看得见，不清（清了键盘用户会丢失自己的位置）
  if (!focusFromPointer) return
  const el = document.activeElement as HTMLElement | null
  if (!el || el === document.body) return
  if (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable) return
  el.blur()
}

/** 执行快捷键动作（应用内快捷键与全局快捷键共用动作表，见 useShortcuts.ts） */
function runShortcut(action: ShortcutAction) {
  switch (action) {
    case 'toggle':
      player.toggle()
      break
    case 'search':
      document.getElementById('search-input')?.focus()
      break
    case 'next':
      player.next()
      break
    case 'prev':
      player.prev()
      break
    case 'lyricForward':
      // 歌词校准：提前 0.5s（歌词显示慢了按这个）
      player.setLyricOffset(-0.5)
      break
    case 'lyricBack':
      // 歌词校准：延后 0.5s（歌词显示快了按这个）
      player.setLyricOffset(0.5)
      break
  }
}

window.addEventListener('keydown', (e) => {
  const target = e.target as HTMLElement
  const typing = target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable
  // MV 播放层打开时让位：快捷键交给播放器（Esc 关闭由 MvPlayer 处理）
  if (mv.opened.value) return
  // 任意弹窗打开时让位：快捷键不穿透弹窗操作背景（空格/搜索/切歌等），
  // Esc 也交由弹窗自己处理（各弹窗均有自己的 Esc 关闭逻辑）
  if (document.querySelector('[data-dialog-panel]')) return
  if (e.key === 'Escape' && nowPlaying.value) {
    blurShortcutFocus()
    nowPlaying.value = false
    return
  }
  // 应用内快捷键全部来自可配置表（设置 → 通用 → 快捷键），默认行为与旧硬编码一致：
  // 空格播放/暂停、Ctrl/Cmd+F 聚焦搜索、N/P 切歌、[ ] 歌词校准；清空（null）即禁用
  for (const def of SHORTCUT_DEFS) {
    if (!eventMatches(e, shortcuts.value[def.action])) continue
    if (typing && !def.ignoresTyping) continue
    e.preventDefault()
    blurShortcutFocus()
    runShortcut(def.action)
    return
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
        @error="onBgError"
      />
    </div>
    <div class="flex min-h-0 flex-1 gap-3">
      <Sidebar />
      <div class="flex min-w-0 flex-1 flex-col gap-3">
        <TopBar />
        <!-- 内容卡片：白色圆角浮于灰色底框上，与侧栏/顶栏/播放条形成圆角卡片分区 -->
        <main ref="mainEl" class="app-surface-blur min-h-0 flex-1 overflow-hidden rounded-2xl bg-(--app-surface)">
          <!-- 常规：CSS 过渡（合成器驱动）；一旦检测到视图未渲染 → 绕过 Transition 直接渲染（自愈） -->
          <Transition v-if="!forcePlainRender" name="view" mode="out-in">
            <component :is="viewComponent" :key="viewKey" />
          </Transition>
          <component v-else :is="viewComponent" :key="viewKey" />
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
    <CloseConfirmDialog />
    <UpdateDialog />
    <MvPlayer />
    <!-- 播放页环境：全窗渐变（含播放条背后）+ 上滑的内容层。z-15 低于播放条，播放条透明浮于其上 -->
    <div class="pointer-events-none absolute inset-0 z-[15] overflow-hidden" :style="{ '--np-accent': npAccent }">
      <Transition :css="false" @enter="npBgEnter" @leave="npBgLeave">
        <NpAmbientBg v-if="nowPlaying" />
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
