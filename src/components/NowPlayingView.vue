<script setup lang="ts">
import { computed, watch } from 'vue'
import { ChevronDown } from 'lucide-vue-next'
import { usePlayerStore } from '@/stores/player'
import { useNav } from '@/composables/useNav'
import { useAmbient } from '@/composables/useAmbient'
import CoverImg from '@/components/CoverImg.vue'
import LyricsPanel from '@/components/LyricsPanel.vue'

const emit = defineEmits<{ close: [] }>()
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
  <!-- 控制按钮在常驻播放条上，这里只保留封面 / 曲目信息 / 歌词 -->
  <!-- 控制按钮在常驻播放条上；页面背景（环境渐变）由 App 渲染，这里保持透明 -->
  <div class="pointer-events-auto flex h-full w-full flex-col">
    <!-- 顶栏 -->
    <div class="np-fade flex h-14 shrink-0 items-center justify-between px-5">
      <span class="text-sm font-medium text-white/80">正在播放</span>
      <button
        class="flex h-9 w-9 items-center justify-center rounded-full text-white/70 transition hover:bg-white/10 hover:text-white"
        title="收起 (Esc)"
        @click="emit('close')"
      >
        <ChevronDown class="h-5 w-5" />
      </button>
    </div>

    <div class="flex min-h-0 flex-1 gap-12 px-10 pb-8">
      <!-- 左：仅封面（垂直居中），辉光随主色 -->
      <div class="np-cover flex h-full w-[30%] max-w-[340px] shrink-0 items-center justify-center">
        <CoverImg
          :album-id="player.current?.albumId ?? null"
          class="aspect-square w-full rounded-2xl"
          rounded="rounded-2xl"
          :style="coverStyle"
        />
      </div>

      <!-- 右：曲目信息 + 歌词 -->
      <div class="np-fade flex h-full min-w-0 flex-1 flex-col">
        <div class="shrink-0 pb-4 pt-6 text-center">
          <h1 class="truncate text-2xl font-bold text-white">{{ player.current?.title ?? '未在播放' }}</h1>
          <p class="mt-1 truncate text-sm text-white/60">
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
