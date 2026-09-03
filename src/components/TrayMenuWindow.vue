<script setup lang="ts">
/**
 * 系统托盘菜单弹窗：圆角玻璃卡片。
 * 顶部=封面+歌名/歌手；中部=上一首/播放暂停/下一首/喜欢；底部=桌面歌词/设置/退出。
 * 播放类指令走既有 'tray' 事件（player store 处理），系统级指令走 'tray:action'。
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { emit, listen } from '@tauri-apps/api/event'
import { SkipPreviousIcon as SkipBack } from '@solar-icons/vue/bold/skip-previous'
import { SkipNextIcon as SkipForward } from '@solar-icons/vue/bold/skip-next'
import { PauseIcon as Pause } from '@solar-icons/vue/bold/pause'
import { PlayIcon as Play } from '@solar-icons/vue/bold/play'
import { HeartIcon as HeartFilled } from '@solar-icons/vue/bold/heart'
import { HeartIcon as HeartOutline } from '@solar-icons/vue/linear/heart'
import { SubtitlesIcon as Subtitles } from '@solar-icons/vue/linear/subtitles'
import { SettingsIcon as Settings } from '@solar-icons/vue/linear/settings'
import { PowerIcon as Power } from '@solar-icons/vue/linear/power'
import { MusicNoteIcon as Music } from '@solar-icons/vue/linear/music-note'
import { coverUrl } from '@/api/scheme'
import type { TraySyncPayload } from '@/composables/useTrayMenu'

const state = ref<TraySyncPayload>({
  title: '',
  artist: '',
  albumId: null,
  playing: false,
  fav: false,
  deskLyrics: false,
  font: '',
})

const cover = computed(() => coverUrl(state.value.albumId))
const coverFailed = ref(false)
watch(
  () => state.value.albumId,
  () => (coverFailed.value = false),
)
const showFallback = computed(() => !cover.value || coverFailed.value)

let unlisten: (() => void) | undefined
onMounted(async () => {
  // 透明背景：让 CSS 圆角卡片从桌面浮起
  document.documentElement.style.background = 'transparent'
  document.body.style.background = 'transparent'
  unlisten = await listen<TraySyncPayload>('tray:sync', (e) => {
    const p = e.payload
    state.value = { ...state.value, ...p }
    // 全局字体（与设置页即时联动）
    if (typeof p?.font === 'string') document.body.style.fontFamily = p.font
  })
  // 通知主窗口：弹窗已就绪，请求推送当前状态
  void emit('tray:ready')
})
onBeforeUnmount(() => unlisten?.())

/** 播放控制指令（player store 监听 'tray'） */
function playback(c: 'prev' | 'toggle' | 'next' | 'fav') {
  void emit('tray', c)
}

/** 系统级指令（useTrayMenu 监听 'tray:action'） */
function action(a: 'show' | 'lyrics' | 'settings' | 'quit') {
  void emit('tray:action', a)
}
</script>
<template>
  <div class="tray-root">
    <!-- 顶部信息展示区：封面 + 歌名/歌手 -->
    <button class="info" title="打开主窗口" @click="action('show')">
      <div class="cover">
        <img
          v-if="!showFallback"
          :src="cover!"
          class="h-full w-full object-cover"
          draggable="false"
          @error="coverFailed = true"
        />
        <div v-else class="fallback">
          <Music class="h-[46%] w-[46%]" :stroke-width="1.5" />
        </div>
      </div>
      <div class="meta">
        <p class="song" :class="{ empty: !state.title }">
          {{ state.title || '未在播放' }}
        </p>
        <p class="artist" :class="{ empty: !state.artist }">
          {{ state.artist || 'LanMusic' }}
        </p>
      </div>
    </button>

    <!-- 核心媒体控制栏：上一首 / 播放暂停 / 下一首 / 喜欢 -->
    <div class="controls">
      <button class="ctrl" title="上一首" @click="playback('prev')">
        <SkipBack class="h-[18px] w-[18px]" />
      </button>
      <button class="ctrl play" :title="state.playing ? '暂停' : '播放'" @click="playback('toggle')">
        <Pause v-if="state.playing" class="h-[20px] w-[20px]" />
        <Play v-else class="h-[20px] w-[20px] translate-x-[1px]" />
      </button>
      <button class="ctrl" title="下一首" @click="playback('next')">
        <SkipForward class="h-[18px] w-[18px]" />
      </button>
      <button
        class="ctrl heart"
        :class="{ active: state.fav }"
        :title="state.fav ? '取消喜欢' : '喜欢'"
        @click="playback('fav')"
      >
        <HeartFilled v-if="state.fav" class="h-[18px] w-[18px]" />
        <HeartOutline v-else class="h-[18px] w-[18px]" />
      </button>
    </div>

    <!-- 分隔线：与系统级操作区隔开，避免误触 -->
    <div class="divider"></div>

    <!-- 底部系统操作区 -->
    <div class="bottom">
      <button class="act" :class="{ active: state.deskLyrics }" @click="action('lyrics')">
        <Subtitles :class="state.deskLyrics ? 'is-active' : ''" class="h-[15px] w-[15px]" />
        桌面歌词
      </button>
      <button class="act" @click="action('settings')">
        <Settings class="h-[15px] w-[15px]" />
        设置
      </button>
      <button class="act quit" @click="action('quit')">
        <Power class="h-[15px] w-[15px]" />
        退出
      </button>
    </div>
  </div>
</template>

<style scoped>
.tray-root {
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: 12px;
  box-sizing: border-box;
  border-radius: 14px;
  background:
    radial-gradient(120% 120% at 0% 0%, rgba(255, 255, 255, 0.1), rgba(255, 255, 255, 0) 60%),
    rgba(28, 28, 30, 0.92);
  box-shadow:
    0 16px 40px rgba(0, 0, 0, 0.35),
    0 3px 10px rgba(0, 0, 0, 0.3),
    inset 0 1px 0 rgba(255, 255, 255, 0.08);
  backdrop-filter: blur(24px);
  user-select: none;
}

/* ---- 顶部信息区 ---- */
.info {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 0;
  border: 0;
  border-radius: 10px;
  background: transparent;
  color: inherit;
  text-align: left;
  cursor: pointer;
}
.info:hover .cover {
  transform: scale(1.03);
}
.cover {
  position: relative;
  flex-shrink: 0;
  width: 48px;
  height: 48px;
  overflow: hidden;
  border-radius: 10px;
  background: linear-gradient(135deg, #3f3f46, #27272a);
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.35);
  transition: transform 0.15s ease;
}
.fallback {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: 100%;
  color: rgba(255, 255, 255, 0.35);
}
.meta {
  min-width: 0;
  flex: 1;
}
.song {
  margin: 0;
  overflow: hidden;
  font-size: 14px;
  font-weight: 600;
  line-height: 1.3;
  color: #fff;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.song.empty {
  color: rgba(255, 255, 255, 0.55);
  font-weight: 500;
}
.artist {
  margin: 2px 0 0;
  overflow: hidden;
  font-size: 12px;
  line-height: 1.3;
  color: #a1a1aa;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.artist.empty {
  color: rgba(255, 255, 255, 0.35);
}

/* ---- 核心媒体控制栏 ---- */
.controls {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 4px;
  margin-top: 10px;
}
.ctrl {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 38px;
  height: 38px;
  padding: 0;
  border: 0;
  border-radius: 50%;
  background: transparent;
  color: #e4e4e7;
  cursor: pointer;
  transition: background 0.15s ease, color 0.15s ease, transform 0.1s ease;
}
.ctrl:hover {
  background: rgba(255, 255, 255, 0.12);
}
.ctrl:active {
  transform: scale(0.92);
}
/* 播放/暂停按钮：略大于其他三键，强调核心操作 */
.ctrl.play {
  width: 46px;
  height: 46px;
  margin: 0 2px;
  background: rgba(255, 255, 255, 0.12);
  color: #fff;
}
.ctrl.play:hover {
  background: rgba(255, 255, 255, 0.2);
}
.ctrl.heart.active {
  color: #fb2c5c;
}

/* ---- 分隔线 ---- */
.divider {
  height: 1px;
  margin: 10px 0;
  background: rgba(255, 255, 255, 0.1);
}

/* ---- 底部系统操作区 ---- */
.bottom {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 4px;
}
.act {
  display: flex;
  align-items: center;
  justify-content: center;
  flex: 1;
  gap: 5px;
  height: 34px;
  padding: 0 6px;
  border: 0;
  border-radius: 8px;
  background: transparent;
  font-size: 12px;
  color: #d4d4d8;
  cursor: pointer;
  transition: background 0.15s ease, color 0.15s ease;
}
.act:hover {
  background: rgba(255, 255, 255, 0.1);
  color: #fff;
}
.act.active {
  color: #a78bfa;
}
.act .is-active {
  color: #a78bfa;
}
.act.quit {
  color: #f87171;
}
.act.quit:hover {
  background: rgba(248, 113, 113, 0.15);
  color: #fca5a5;
}
</style>