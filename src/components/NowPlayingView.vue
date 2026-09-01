<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import gsap from 'gsap'
import { ChevronDown } from '@lucide/vue'
import { usePlayerStore } from '@/stores/player'
import { useNav } from '@/composables/useNav'
import { useAmbient } from '@/composables/useAmbient'
import { useSkin } from '@/composables/useSkin'
import { ensureAnalyser, readSpectrum } from '@/composables/useSpectrum'
import { CUSTOM_WINDOW_CONTROLS, IS_MAC } from '@/utils/platform'
import CoverImg from '@/components/CoverImg.vue'
import LyricsPanel from '@/components/LyricsPanel.vue'
import WindowControls from '@/components/WindowControls.vue'

const emit = defineEmits<{ close: [] }>()
const props = defineProps<{ focusHidden?: boolean }>()
const player = usePlayerStore()
const nav = useNav()

const headerEl = ref<HTMLElement | null>(null)
/** 专注模式：顶部控制栏上滑隐藏 / 鼠标移动时滑回 */
watch(
  () => props.focusHidden,
  (hidden, prev) => {
    if (prev === undefined) return // 初始渲染不做动画
    if (!headerEl.value) return
    gsap.to(headerEl.value, {
      yPercent: hidden ? -100 : 0,
      duration: 0.45,
      ease: 'power3.out',
      overwrite: 'auto',
    })
  },
)

// ---- 环境色：跟随专辑封面主色（提取结果全局共享，播放条也使用；页面背景由 App 渲染）----
const { palette, setAlbum } = useAmbient()
watch(() => player.current?.albumId, (id) => void setAlbum(id), { immediate: true })

const coverStyle = computed(() =>
  palette.value ? { boxShadow: `0 25px 80px -20px ${palette.value.glow}` } : undefined,
)

// ---- 皮肤：圆形粒子样式下封面改为圆形，并在其周围绘制频谱粒子 ----
const skin = useSkin()
/** 圆形粒子皮肤激活时封面显示为圆形，并适当缩小给粒子环留出空间 */
const coverCircular = computed(() => skin.value.style === 'particles')
const coverClass = computed(() =>
  coverCircular.value ? 'rounded-full max-w-[300px]' : 'rounded-2xl max-w-[340px]',
)

// 圆形粒子频谱：粒子沿圆形封面外圈分布，幅度驱动半径与亮度
const coverBox = ref<HTMLElement | null>(null)
const particleCanvas = ref<HTMLCanvasElement | null>(null)
const particleFreq = new Uint8Array(256)
let particleRaf = 0

function drawParticles() {
  const c = particleCanvas.value
  const box = coverBox.value
  if (!c || !box) return
  const g = c.getContext('2d')
  if (!g) return
  const dpr = window.devicePixelRatio || 1
  const w = c.clientWidth
  const h = c.clientHeight
  if (!w || !h) return
  if (c.width !== Math.round(w * dpr) || c.height !== Math.round(h * dpr)) {
    c.width = Math.round(w * dpr)
    c.height = Math.round(h * dpr)
  }
  g.setTransform(dpr, 0, 0, dpr, 0, 0)
  g.clearRect(0, 0, w, h)

  const ok = readSpectrum(particleFreq)
  const color = palette.value?.accent ?? '#a78bfa'
  // 封面为容器内居中的正方形（粒子模式下 max 300px），粒子沿其外圈分布
  const coverR = Math.min(box.clientWidth, box.clientHeight, coverCircular.value ? 300 : 340) / 2
  // 粒子最大扩散半径适配画布可用空间，保证不出界被裁切
  const maxR = Math.min(w, h) / 2 - 4
  const spread = Math.max(12, maxR - coverR - 14)
  const cx = w / 2
  const cy = h / 2
  const t = performance.now() / 1000
  const n = particleFreq.length
  const half = n / 2
  for (let i = 0; i < n; i++) {
    // 镜像对称取样：频段沿圆环左右对称展开，两处接缝（顶部与底部）两侧为相邻频段，
    // 幅度连续、头尾自然闭合；若直接按 0..n 顺排，首尾会从低频跳到高频形成断口
    const amp = ok ? particleFreq[i <= half ? i : n - i] / 255 : 0
    if (amp < 0.05) continue
    // 从顶部起笔、缓慢旋转，幅度越大离封面越远、粒子越大越亮
    const angle = (i / n) * Math.PI * 2 - Math.PI / 2 + t * 0.12
    const r = coverR + 14 + amp * spread
    g.globalAlpha = 0.18 + amp * 0.82
    g.fillStyle = color
    g.beginPath()
    g.arc(cx + Math.cos(angle) * r, cy + Math.sin(angle) * r, 1 + amp * 2.6, 0, Math.PI * 2)
    g.fill()
  }
  g.globalAlpha = 1
}

function loopParticles() {
  try {
    drawParticles()
  } catch {
    /* 单帧绘制失败不中断循环 */
  }
  particleRaf = requestAnimationFrame(loopParticles)
}

watch(
  [() => skin.value.on, () => skin.value.style, particleCanvas],
  ([on, style, el]) => {
    cancelAnimationFrame(particleRaf)
    if (on && style === 'particles' && el) {
      ensureAnalyser()
      particleRaf = requestAnimationFrame(loopParticles)
    }
  },
  { immediate: true },
)
onBeforeUnmount(() => cancelAnimationFrame(particleRaf))

function openAlbum() {
  const t = player.current
  if (t?.albumId == null) return
  nav.go({ view: 'tracks', albumId: t.albumId, albumTitle: t.album ?? '未知专辑' })
  emit('close')
}

function openArtist() {
  const t = player.current
  if (t?.artistId == null) return
  nav.go({ view: 'tracks', artistId: t.artistId, artistName: t.artist ?? '未知艺人' })
  emit('close')
}
</script>

<template>
  <!-- 控制按钮在常驻播放条上；顶栏为自定义标题栏（拖拽 + 窗口控制按钮），页面背景（环境渐变）由 App 渲染，这里保持透明 -->
  <div class="pointer-events-auto flex h-full w-full flex-col">
    <!-- 顶栏：自定义标题栏，空白处可拖拽移动窗口（播放页遮住了 TopBar 的拖拽区，这里补上）；
         Windows/Linux 右侧自绘窗口控制按钮，macOS 用原生红绿灯（左侧留出约 76px 偏移） -->
    <header
      ref="headerEl"
      data-tauri-drag-region
      class="np-fade flex h-14 shrink-0 items-center justify-between"
      :class="IS_MAC ? 'pl-[76px] pr-4' : 'pl-4 pr-0'"
    >
      <!-- 左：关闭播放页（其余空白仍为拖拽区） -->
      <button
        class="flex h-8 w-8 shrink-0 items-center justify-center rounded-full text-white/70 transition hover:bg-white/10 hover:text-white"
        title="收起播放页 (Esc)"
        @click="emit('close')"
      >
        <ChevronDown class="h-5 w-5" />
      </button>
      <WindowControls v-if="CUSTOM_WINDOW_CONTROLS" ambient />
    </header>

    <div class="flex min-h-0 flex-1 gap-12 px-10 pb-8">
      <!-- 左：封面（约 40% 宽，垂直居中），辉光随主色；圆形粒子皮肤下封面为圆形 -->
      <div ref="coverBox" class="np-cover relative flex h-full min-w-0 flex-1 basis-2/5 items-center justify-center">
        <!-- 圆形粒子频谱：画布向四周扩出 32px，粒子围绕圆形封面外圈绘制不被裁切
             （canvas 是替换元素，必须显式给定宽高，否则 -inset-8 不会拉伸，会退化为 300x150 内在尺寸） -->
        <canvas
          v-if="skin.on && skin.style === 'particles'"
          ref="particleCanvas"
          class="pointer-events-none absolute -top-8 -left-8 h-[calc(100%+4rem)] w-[calc(100%+4rem)]"
        ></canvas>
        <CoverImg
          :album-id="player.current?.albumId ?? null"
          class="aspect-square max-h-full w-full"
          :class="coverClass"
          :rounded="coverClass"
          :style="coverStyle"
        />
      </div>

      <!-- 右：曲目信息 + 歌词（约 60% 宽） -->
      <div class="np-fade flex h-full min-w-0 flex-1 basis-3/5 flex-col">
        <div
          class="flex shrink-0 flex-col items-center pb-4 pt-6 text-center"
        >
          <h1 class="max-w-full truncate text-2xl font-bold text-white">{{ player.current?.title ?? '未在播放' }}</h1>
          <p class="mt-1 max-w-full truncate text-sm text-white/60">
            <button
              v-if="player.current?.artistId != null"
              class="transition hover:text-white hover:underline"
              :title="`查看艺人：${player.current?.artist ?? '未知艺人'}`"
              @click="openArtist"
            >{{ player.current?.artist ?? '未知艺人' }}</button>
            <span v-else>{{ player.current?.artist ?? '' }}</span>
          </p>
          <button
            v-if="player.current?.albumId != null"
            class="mt-0.5 max-w-full truncate text-xs text-white/40 transition hover:text-white/80"
            title="查看专辑"
            @click="openAlbum"
          >
            {{ player.current?.album }}
          </button>
        </div>
        <div class="min-h-0 flex-1">
          <LyricsPanel />
        </div>
      </div>
    </div>
  </div>
</template>


