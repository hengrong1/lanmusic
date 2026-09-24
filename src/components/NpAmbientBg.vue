<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useAmbient } from '@/composables/useAmbient'
import type { AmbientPalette } from '@/utils/color'

/**
 * 播放页环境背景：全窗渐变双层交叉淡入 + 顶部光斑呼吸。
 * 切歌时提取出的新环境色直接换 background 会瞬跳（渐变不能 CSS transition），
 * 这里用 A/B 两层轮换：新色淡入、旧色淡出（0.8s），色相差大的两首歌之间平滑过渡。
 * 元素初次渲染即带最终类（started 分支在 immediate watch 同步设置），首次挂载不触发过渡，
 * 播放页展开的淡入仍由 App.vue 的 npBgEnter 独立驱动。
 */
const { palette } = useAmbient()

/** 两层渐变的环境色参数：null = 该层未放置 */
const layers = ref<[AmbientPalette | null, AmbientPalette | null]>([null, null])
const active = ref<0 | 1>(0)
const started = ref(false)

watch(
  palette,
  (p) => {
    if (!started.value) {
      layers.value[0] = p
      active.value = 0
      started.value = true
      return
    }
    const next: 0 | 1 = active.value === 0 ? 1 : 0
    layers.value[next] = p
    active.value = next
  },
  { immediate: true },
)

function gradStyle(p: AmbientPalette | null) {
  return {
    background: p
      ? `linear-gradient(to bottom, ${p.glow} 0%, ${p.deep} 55%, #09090b 100%)`
      : 'linear-gradient(to bottom, #2e1065 0%, #09090b 55%, #09090b 100%)',
  }
}
const layerStyles = computed(() => layers.value.map(gradStyle))

/** 光斑颜色：跟随环境强调色（低透明度 + 大模糊，颜色随切歌跳变几乎不可感知） */
const accent = computed(() => palette.value?.accent ?? '#a78bfa')
</script>

<template>
  <!-- isolate 铁律：根必须自建层叠上下文，否则内部 z-10/z-20 会逃到外层容器参与排序，
       渐变与光斑浮到播放页内容层（z-auto、DOM 靠后）之上，整个播放页被背景盖成空白 -->
  <div class="absolute inset-0 isolate">
    <div
      v-for="i in [0, 1] as const"
      :key="i"
      class="absolute inset-0 transition-opacity duration-[800ms] ease-in-out"
      :class="active === i ? 'z-10 opacity-100' : 'z-0 opacity-0'"
      :style="layerStyles[i]"
    ></div>
    <!-- 顶部光斑呼吸：低透明度环境色大光斑缓慢漂移，让背景「活」起来。
         transform/opacity 全走合成器，动画本身不吃主线程 -->
    <div
      class="ambient-blob pointer-events-none absolute top-[-45%] left-1/2 z-20 h-[110%] w-[120%] -translate-x-1/2"
      :style="{ background: `radial-gradient(ellipse 55% 55% at 50% 40%, ${accent}, transparent 72%)` }"
    ></div>
  </div>
</template>

<style scoped>
.ambient-blob {
  opacity: 0.14;
  filter: blur(40px);
  animation: ambient-drift 22s ease-in-out infinite alternate;
  will-change: transform, opacity;
}
@keyframes ambient-drift {
  0% {
    transform: translateX(-50%) translateX(-3%) scale(1);
    opacity: 0.11;
  }
  50% {
    opacity: 0.18;
  }
  100% {
    transform: translateX(-50%) translateX(3%) scale(1.06);
    opacity: 0.14;
  }
}
</style>
