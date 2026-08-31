<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import gsap from 'gsap'
import Sidebar from '@/components/Sidebar.vue'
import TopBar from '@/components/TopBar.vue'
import PlayerBar from '@/components/PlayerBar.vue'
import QueuePanel from '@/components/QueuePanel.vue'
import ConfirmDialog from '@/components/ConfirmDialog.vue'
import NowPlayingView from '@/components/NowPlayingView.vue'
import Toast from '@/components/Toast.vue'
import TracksView from '@/views/TracksView.vue'
import AlbumsView from '@/views/AlbumsView.vue'
import ArtistsView from '@/views/ArtistsView.vue'
import PlaylistView from '@/views/PlaylistView.vue'
import NetworkView from '@/views/NetworkView.vue'
import SettingsView from '@/views/SettingsView.vue'
import { useLibraryStore } from '@/stores/library'
import { usePlayerStore } from '@/stores/player'
import { useNav } from '@/composables/useNav'
import { useAmbient } from '@/composables/useAmbient'

const library = useLibraryStore()
const player = usePlayerStore()
const nav = useNav()
const { palette } = useAmbient()

const queueOpen = ref(false)
const nowPlaying = ref(false)

/** 播放页专注模式：鼠标不在底部/顶栏控制区，5s 无移动则隐藏控制，移动鼠标恢复 */
const npFocus = ref(false)
const PLAYER_BAR_H = 80 // 播放条 h-20
const HEADER_H = 56 // 顶栏 h-14
let focusTimer: ReturnType<typeof setTimeout> | undefined
window.addEventListener(
  'mousemove',
  (e) => {
    if (!nowPlaying.value) return
    const inBarZone = e.clientY >= window.innerHeight - PLAYER_BAR_H
    const inHeaderZone = e.clientY <= HEADER_H
    if (inBarZone || inHeaderZone) {
      // 鼠标在底部播放条 / 顶部控制区：保持控制可见
      clearTimeout(focusTimer)
      if (npFocus.value) npFocus.value = false
      return
    }
    // 内容区移动：恢复显示并重置 5s 无操作计时
    if (npFocus.value) npFocus.value = false
    clearTimeout(focusTimer)
    focusTimer = setTimeout(() => (npFocus.value = true), 5000)
  },
  { passive: true },
)
watch(nowPlaying, (v) => {
  if (!v) {
    npFocus.value = false
    clearTimeout(focusTimer)
  } else {
    // 打开播放页即启动 5s 无操作计时（鼠标在控制区会被 mousemove 逻辑打断）
    clearTimeout(focusTimer)
    focusTimer = setTimeout(() => (npFocus.value = true), 5000)
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
    case 'network':
      return NetworkView
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

onMounted(() => {
  void library.init()
  void player.restore()
})

// 全局快捷键
window.addEventListener('keydown', (e) => {
  const target = e.target as HTMLElement
  const typing = target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable
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
  } else if (!typing && e.key.toLowerCase() === 'n') {
    player.next()
  } else if (!typing && e.key.toLowerCase() === 'p') {
    player.prev()
  }
})
</script>

<template>
  <div class="relative flex h-screen select-none flex-col overflow-hidden bg-zinc-50 text-zinc-900 dark:bg-zinc-950 dark:text-zinc-100">
    <div class="flex min-h-0 flex-1">
      <Sidebar />
      <div class="flex min-w-0 flex-1 flex-col">
        <TopBar />
        <main class="min-h-0 flex-1 overflow-hidden">
          <Transition :css="false" mode="out-in" @enter="viewEnter" @leave="viewLeave">
            <component :is="viewComponent" :key="viewKey" />
          </Transition>
        </main>
      </div>
      <QueuePanel v-if="queueOpen" @close="queueOpen = false" />
    </div>
    <PlayerBar
      :now-playing-open="nowPlaying"
      :focus-hidden="npFocus"
      @toggle-queue="queueOpen = !queueOpen"
      @toggle-now-playing="nowPlaying = !nowPlaying"
    />
    <Toast />
    <ConfirmDialog />
    <!-- 播放页环境：全窗渐变（含播放条背后）+ 上滑的内容层。z-15 低于播放条，播放条透明浮于其上 -->
    <div class="pointer-events-none absolute inset-0 z-[15] overflow-hidden" :style="{ '--np-accent': npAccent }">
      <Transition :css="false" @enter="npBgEnter" @leave="npBgLeave">
        <div v-if="nowPlaying" class="absolute inset-0" :style="npBgStyle"></div>
      </Transition>
      <Transition :css="false" @enter="nowPlayingEnter" @leave="nowPlayingLeave">
        <div v-if="nowPlaying" class="pointer-events-auto absolute inset-x-0 top-0 bottom-20 overflow-hidden">
          <NowPlayingView :focus-hidden="npFocus" @close="nowPlaying = false" />
        </div>
      </Transition>
    </div>
  </div>
</template>
