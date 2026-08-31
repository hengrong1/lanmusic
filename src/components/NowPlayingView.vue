<script setup lang="ts">
import { computed, watch } from 'vue'
import { ChevronDown } from '@lucide/vue'
import { usePlayerStore } from '@/stores/player'
import { useNav } from '@/composables/useNav'
import { useAmbient } from '@/composables/useAmbient'
import { CUSTOM_WINDOW_CONTROLS, IS_MAC } from '@/utils/platform'
import CoverImg from '@/components/CoverImg.vue'
import LyricsPanel from '@/components/LyricsPanel.vue'
import WindowControls from '@/components/WindowControls.vue'

const emit = defineEmits<{ close: [] }>()
const props = defineProps<{ focusHidden?: boolean }>()
const player = usePlayerStore()
const nav = useNav()

// ---- 环境色：跟随专辑封面主色（提取结果全局共享，播放条也使用；页面背景由 App 渲染）----
const { palette, setAlbum } = useAmbient()
watch(() => player.current?.albumId, (id) => void setAlbum(id), { immediate: true })

const coverStyle = computed(() =>
  palette.value ? { boxShadow: `0 25px 80px -20px ${palette.value.glow}` } : undefined,
)

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
      data-tauri-drag-region
      class="np-fade flex h-14 shrink-0 items-center justify-between transition-transform duration-500 ease-out will-change-transform"
      :class="[IS_MAC ? 'pl-[76px] pr-4' : 'pl-4 pr-0', props.focusHidden ? '-translate-y-full' : 'translate-y-0']"
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
      <!-- 左：封面（约 40% 宽，垂直居中），辉光随主色 -->
      <div class="np-cover flex h-full min-w-0 flex-1 basis-2/5 items-center justify-center">
        <CoverImg
          :album-id="player.current?.albumId ?? null"
          class="aspect-square max-h-full w-full max-w-[340px] rounded-2xl"
          rounded="rounded-2xl"
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
