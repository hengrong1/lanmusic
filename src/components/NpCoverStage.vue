<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { usePlayerStore } from '@/stores/player'
import { useAmbient } from '@/composables/useAmbient'
import { useSkin, useSpectrumMode } from '@/composables/useSkin'
import { ensureAnalyser, readSpectrum } from '@/composables/useSpectrum'
import { coverUrl } from '@/api/scheme'
import CoverImg from '@/components/CoverImg.vue'

const props = withDefaults(defineProps<{ maxSize?: number }>(), { maxSize: 340 })

const player = usePlayerStore()
const { palette } = useAmbient()
const skin = useSkin()
const spectrumMode = useSpectrumMode()

/** 圆形封面：圆形粒子频谱激活时为圆（给粒子环留位），否则圆角矩形 */
const circular = computed(() => spectrumMode.value === 'particles')

/** 封面辉光随主色 */
const coverStyle = computed(() =>
  palette.value ? { boxShadow: `0 25px 80px -20px ${palette.value.glow}` } : undefined,
)

// ---- 氛围层：封面背后放大 + 模糊的同图光晕，垫出空间感（Apple Music 风）----
// 低透明度 + 大模糊下，切歌时旧图消失/新图淡入的跳变几乎不可感知，单层淡入即可
const ambientLoaded = ref(false)
watch(() => player.current?.albumId, () => (ambientLoaded.value = false))

// ---- 封面尺寸：恒为正方形，边长 = min(容器宽, 容器高, maxSize)，ResizeObserver 实测 ----
// 不用 aspect-square + w-full/max-h 组合：宽高比失衡的容器（如上下布局）会把它压成矩形，
// 而粒子数学假设封面是居中正方形，必须共用同一个边长。
const coverBox = ref<HTMLElement | null>(null)
const boxSize = ref({ w: 0, h: 0 })
let resizeObserver: ResizeObserver | null = null
onMounted(() => {
  resizeObserver = new ResizeObserver((entries) => {
    const r = entries[0]?.contentRect
    if (r) boxSize.value = { w: r.width, h: r.height }
  })
  if (coverBox.value) resizeObserver.observe(coverBox.value)
})
onBeforeUnmount(() => resizeObserver?.disconnect())
const coverSide = computed(() => {
  const { w, h } = boxSize.value
  // 圆形粒子激活时封面只占容器高的 ~75%，其余留给粒子环呼吸（环带太窄会糊成一团）
  const heightCap = spectrumMode.value === 'particles' ? h * 0.75 : h
  return Math.max(0, Math.min(w, heightCap, props.maxSize))
})

/** 氛围层边长：封面 × 1.4，模糊后自然向外晕开 */
const ambientSide = computed(() => Math.round(coverSide.value * 1.4))

// 圆形粒子频谱：粒子沿封面外圈分布，幅度驱动半径与亮度；画布锚定封面容器，随布局移动
const particleCanvas = ref<HTMLCanvasElement | null>(null)
const particleFreq = new Uint8Array(256)
let particleRaf = 0

function drawParticles() {
  const c = particleCanvas.value
  if (!c) return
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
  const color2 = palette.value?.accent2 ?? '#e879f9'
  // 粒子沿封面外圈分布：半径与封面共用同一个边长（coverSide），保证严格贴合
  const coverR = coverSide.value / 2
  // 粒子最大扩散半径适配画布可用空间，保证不出界被裁切
  const maxR = Math.min(w, h) / 2 - 4
  const spread = Math.max(12, maxR - coverR - 14)
  const cx = w / 2
  const cy = h / 2
  const t = performance.now() / 1000
  const n = particleFreq.length
  const half = n / 2
  // 密度控制：环带周长越小，粒子抽稀越狠（小画布上 256 个点会糊成一团看不清）
  const ringCircumference = 2 * Math.PI * (coverR + 14 + spread / 2)
  const stride = ringCircumference < 700 ? 3 : ringCircumference < 1100 ? 2 : 1
  // 环带太窄时跳过交叉连接线（细环里会糊成蜘蛛网）
  const drawLinks = spread >= 45

  // 第一层：正向旋转的粒子环
  for (let i = 0; i < n; i += stride) {
    const amp = ok ? particleFreq[i <= half ? i : n - i] / 255 : 0
    if (amp < 0.05) continue
    const angle = (i / n) * Math.PI * 2 - Math.PI / 2 + t * 0.12
    const r = coverR + 14 + amp * spread
    g.globalAlpha = 0.18 + amp * 0.82
    g.fillStyle = color
    g.beginPath()
    g.arc(cx + Math.cos(angle) * r, cy + Math.sin(angle) * r, 1 + amp * 2.6, 0, Math.PI * 2)
    g.fill()
  }

  // 第二层：反向旋转的粒子环（双线交叉效果）
  for (let i = 0; i < n; i += stride) {
    const amp = ok ? particleFreq[i <= half ? i : n - i] / 255 : 0
    if (amp < 0.05) continue
    // 反向旋转 + 相位偏移，形成交叉
    const angle = (i / n) * Math.PI * 2 - Math.PI / 2 - t * 0.12 + Math.PI / n
    const r = coverR + 14 + amp * spread
    g.globalAlpha = 0.12 + amp * 0.6
    g.fillStyle = color2
    g.beginPath()
    g.arc(cx + Math.cos(angle) * r, cy + Math.sin(angle) * r, 0.8 + amp * 2, 0, Math.PI * 2)
    g.fill()
  }

  // 连接线：在交叉点处绘制连接线（环带太窄时跳过，避免糊成蜘蛛网）
  g.strokeStyle = color
  g.lineWidth = 0.5
  if (drawLinks) {
    for (let i = 0; i < n; i += 8) {
      const amp = ok ? particleFreq[i <= half ? i : n - i] / 255 : 0
      if (amp < 0.1) continue
      const angle1 = (i / n) * Math.PI * 2 - Math.PI / 2 + t * 0.12
      const angle2 = (i / n) * Math.PI * 2 - Math.PI / 2 - t * 0.12 + Math.PI / n
      const r = coverR + 14 + amp * spread
      g.globalAlpha = 0.08 + amp * 0.3
      g.beginPath()
      g.moveTo(cx + Math.cos(angle1) * r, cy + Math.sin(angle1) * r)
      g.lineTo(cx + Math.cos(angle2) * r, cy + Math.sin(angle2) * r)
      g.stroke()
    }
  }

  g.globalAlpha = 1
}

// ---- 绘制循环管理：播放与暂停衰减期统一 60fps rAF，能量衰减到静默后完全停帧省电。
//      衰减期（暂停后频谱余韵缩回）不能降频：低频定时器抽帧会像幻灯片一样一顿一顿；
//      60fps 下 analyser 平滑衰减约 0.25s 收尾，视觉与播放时完全一致，归零后即停帧 ----
function hasEnergy(): boolean {
  for (let i = 0; i < particleFreq.length; i++) {
    if (particleFreq[i] > 4) return true
  }
  return false
}

function stopLoops() {
  cancelAnimationFrame(particleRaf)
}

function syncLoop(on: boolean, style: string, el: HTMLCanvasElement | null, playing: boolean) {
  stopLoops()
  if (!on || style !== 'particles' || !el) return
  ensureAnalyser()
  const loop = () => {
    try {
      drawParticles()
    } catch {
      /* 单帧绘制失败不中断循环 */
    }
    if (!playing && !hasEnergy()) {
      stopLoops() // 画布已空：取消未触发的下一帧并退出，循环终止
      return
    }
    particleRaf = requestAnimationFrame(loop)
  }
  particleRaf = requestAnimationFrame(loop)
}

watch(
  [() => skin.value.on, () => skin.value.style, particleCanvas, () => player.playing],
  ([on, style, el, playing]) => syncLoop(on, style, el, playing),
  { immediate: true },
)
onBeforeUnmount(() => stopLoops())
</script>

<template>
  <div ref="coverBox" class="np-cover relative flex h-full min-w-0 w-full items-center justify-center">
    <!-- 氛围层：封面背后放大 + 模糊的同图光晕，垫出空间感；容器统一控透明度，img 自身淡入。
         必须排在粒子画布之前（同为 z-auto 定位元素按 DOM 序绘制）：粒子是前景动效，画在氛围层之上 -->
    <div
      v-if="player.current?.albumId != null"
      class="ambient-art pointer-events-none absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2"
      :style="{ width: `${ambientSide}px`, height: `${ambientSide}px` }"
    >
      <img
        :key="player.current.albumId"
        :src="coverUrl(player.current.albumId) ?? ''"
        alt=""
        class="h-full w-full object-cover transition-opacity duration-700"
        :class="ambientLoaded ? 'opacity-100' : 'opacity-0'"
        draggable="false"
        @load="ambientLoaded = true"
        @error="ambientLoaded = false"
      />
    </div>
    <!-- 粒子画布：向四周扩出 32px，围绕封面外圈绘制不被裁切
         （canvas 是替换元素，必须显式给定宽高，否则 -inset-8 不会拉伸，会退化为 300x150 内在尺寸） -->
    <canvas
      v-if="skin.on && skin.style === 'particles'"
      ref="particleCanvas"
      class="pointer-events-none absolute -top-8 -left-8 h-[calc(100%+4rem)] w-[calc(100%+4rem)]"
    ></canvas>
    <CoverImg
      :album-id="player.current?.albumId ?? null"
      class="shrink-0"
      :rounded="circular ? 'rounded-full' : 'rounded-2xl'"
      :style="{ width: `${coverSide}px`, height: `${coverSide}px`, ...coverStyle }"
    />
  </div>
</template>

<style scoped>
/* 氛围层：大模糊 + 提饱和，整体低透明度叠在深底渐变上形成光晕 */
.ambient-art {
  opacity: 0.5;
  filter: blur(56px) saturate(1.35);
}
</style>
