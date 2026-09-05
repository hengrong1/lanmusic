<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import Plyr from 'plyr'
import 'plyr/dist/plyr.css'
// 控件图标 sprite 本地化：默认 iconUrl 指向 cdn.plyr.io，打包后会被 CSP(connect-src 'self') 拦截导致图标全空
import plyrIconUrl from '@/assets/plyr.svg?url'
import { CloseIcon as X } from '@solar-icons/vue/linear/close'
import { toast } from '@/composables/useToast'
import { useMvPlayer } from '@/composables/useMvPlayer'

const { t, locale } = useI18n()
const { track, url, close } = useMvPlayer()

const videoEl = ref<HTMLVideoElement | null>(null)
let plyr: Plyr | null = null

/** Plyr 控件文案（中文；英文走 Plyr 内置默认文案，无需覆盖） */
const PLYR_I18N_ZH = {
  restart: '重播', rewind: '后退 {seektime} 秒', play: '播放', pause: '暂停',
  fastForward: '快进 {seektime} 秒', seek: '进度', played: '已播放', buffered: '已缓冲',
  currentTime: '当前时间', duration: '总时长', volume: '音量', mute: '静音', unmute: '取消静音',
  enableCaptions: '开启字幕', disableCaptions: '关闭字幕', enterFullscreen: '进入全屏',
  exitFullscreen: '退出全屏', frameTitle: '{title} 播放器', captions: '字幕', settings: '设置',
  pip: '画中画', menuBack: '返回', speed: '速度', normal: '正常', quality: '画质', loop: '循环',
  start: '开始', end: '结束', all: '全部', reset: '重置', disabled: '禁用', enabled: '启用',
  advertisement: '广告', qualityBadge: { 2160: '4K', 1440: 'HD', 1080: 'HD', 720: 'HD', 576: 'SD', 480: 'SD' },
}

function destroyPlyr() {
  plyr?.destroy()
  plyr = null
}

async function initPlyr() {
  destroyPlyr()
  await nextTick()
  if (!videoEl.value) return
  plyr = new Plyr(videoEl.value, {
    ratio: '16:9',
    iconUrl: plyrIconUrl,
    controls: ['play-large', 'play', 'progress', 'current-time', 'duration', 'mute', 'volume', 'settings', 'pip', 'fullscreen'],
    // 语言跟随应用语言设置（设置 → 语言，存 lm.locale）
    i18n: locale.value === 'zh' ? PLYR_I18N_ZH : {},
    keyboard: { focused: true, global: true },
    tooltips: { controls: true, seek: true },
  })
  // 点击 MV 按钮本身就是用户手势，可自动起播（play() 类型为 Promise | void，统一包一层）
  void Promise.resolve(plyr.play()).catch(() => {})
}

// 打开/切换 MV 时初始化播放器；关闭时销毁实例
watch(track, (t) => {
  if (t) void initPlyr()
  else destroyPlyr()
})

// 语言切换时同步更新已打开播放器的控件文案
watch(locale, () => {
  if (track.value) void initPlyr()
})

function onVideoError() {
  // 视频 404 / 解码失败等：提示后直接收起
  toast(t('mv.loadFailed'), 'error')
  close()
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && track.value) {
    e.stopPropagation()
    close()
  }
}

onMounted(() => window.addEventListener('keydown', onKeydown, true))
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown, true)
  destroyPlyr()
})
</script>

<template>
  <Transition
    enter-active-class="transition-opacity duration-200"
    enter-from-class="opacity-0"
    leave-active-class="transition-opacity duration-150"
    leave-to-class="opacity-0"
  >
    <div
      v-if="track"
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-6 backdrop-blur-sm dark:bg-black/80"
      @click.self="close"
    >
      <div class="mv-shell w-full max-w-4xl overflow-hidden rounded-2xl bg-black shadow-2xl ring-1 ring-zinc-800/60">
        <!-- 标题栏：曲目名（含艺人），右上角关闭 -->
        <div class="flex items-center justify-between gap-3 px-4 py-2.5">
          <div class="min-w-0 truncate text-sm text-zinc-100">
            <span class="font-medium">{{ track.title }}</span>
            <span v-if="track.artist" class="ml-2 text-zinc-400">{{ track.artist }}</span>
          </div>
          <button
            class="shrink-0 cursor-pointer rounded-full p-1.5 text-zinc-400 transition hover:bg-white/10 hover:text-white"
            :title="t('common.close')"
            @click="close"
          >
            <X class="h-4 w-4" />
          </button>
        </div>
        <!-- Plyr 播放器：控件主题色/菜单配色见下方样式，语言跟随应用语言 -->
        <video ref="videoEl" :key="track.id" :src="url" playsinline @error="onVideoError"></video>
      </div>
    </div>
  </Transition>
</template>

<!-- 非 scoped：Plyr 实例挂在 video 元素上生成的控件类名需要全局命中 -->
<style>
.mv-shell video {
  width: 100%;
  aspect-ratio: 16 / 9;
  display: block;
}
/* 主题色：跟随应用主色（violet-500） */
.mv-shell .plyr {
  --plyr-color-main: #8b5cf6;
  --plyr-badge-background: #8b5cf6;
  --plyr-range-thumb-size: 13px;
}
/* 菜单/设置面板配色跟随深浅色主题 */
.mv-shell .plyr {
  --plyr-menu-background: #ffffff;
  --plyr-menu-color: #27272a;
}
.dark .mv-shell .plyr {
  --plyr-menu-background: rgba(24, 24, 27, 0.95);
  --plyr-menu-color: #f4f4f5;
}
/* 控件悬停底色随主题色 */
.mv-shell .plyr__control--overlaid {
  background: rgba(139, 92, 246, 0.85);
}
.mv-shell .plyr__control--overlaid:hover,
.mv-shell .plyr__control--overlaid:focus {
  background: #8b5cf6;
}
</style>