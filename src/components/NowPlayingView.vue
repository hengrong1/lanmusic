<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import gsap from 'gsap'
import { AltArrowDownIcon as ChevronDown } from '@solar-icons/vue/linear/alt-arrow-down'
import { usePlayerStore } from '@/stores/player'
import { useAmbient } from '@/composables/useAmbient'
import { useNowPlayingStyle } from '@/composables/useNowPlayingStyle'
import { CUSTOM_WINDOW_CONTROLS, IS_MAC } from '@/utils/platform'
import { useI18n } from 'vue-i18n'
import WindowControls from '@/components/WindowControls.vue'
import NpSideLayout from '@/components/NpSideLayout.vue'
import NpStackedLayout from '@/components/NpStackedLayout.vue'

const emit = defineEmits<{ close: [] }>()
const props = defineProps<{ focusHidden?: boolean }>()
const { t: tr } = useI18n()
const player = usePlayerStore()

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

// ---- 环境色：跟随专辑封面主色（提取结果全局共享，播放条/封面组件读取；页面背景由 App 渲染）----
const { setAlbum } = useAmbient()
watch(() => player.current?.albumId, (id) => void setAlbum(id), { immediate: true })

const collapseTip = computed(() => `${tr('player.collapseNowPlaying')} (Esc)`)

// ---- 布局切换：装扮面板选中的预设映射到 stage 组件 ----
const npStyle = useNowPlayingStyle()
const stage = computed(() => (npStyle.value === 'stacked' ? NpStackedLayout : NpSideLayout))
</script>

<template>
  <!-- 控制按钮在常驻播放条上；顶栏为自定义标题栏（拖拽 + 窗口控制按钮），页面背景（环境渐变）由 App 渲染，这里保持透明 -->
  <div class="pointer-events-auto flex h-full w-full flex-col">
    <header
      ref="headerEl"
      data-tauri-drag-region
      class="np-fade flex h-14 shrink-0 items-center justify-between"
      :class="IS_MAC ? 'pl-[76px] pr-4' : 'pl-4 pr-0'"
    >
      <!-- 左：关闭播放页（其余空白仍为拖拽区） -->
      <button
        class="flex h-8 w-8 shrink-0 cursor-pointer items-center justify-center rounded-lg text-white/70 transition hover:bg-white/10 hover:text-white"
        v-tooltip="collapseTip"
        @click="emit('close')"
      >
        <ChevronDown class="h-5 w-5" />
      </button>
      <WindowControls v-if="CUSTOM_WINDOW_CONTROLS" ambient />
    </header>

    <!-- 中部舞台：布局由装扮面板的预设决定（即时切换）；布局内点击艺人/专辑跳转后关闭播放页 -->
    <component :is="stage" :key="npStyle" @navigate="emit('close')" />
  </div>
</template>
