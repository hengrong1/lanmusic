<script setup lang="ts">
import { Play } from 'lucide-vue-next'
import type { AlbumItem } from '@/types'
import CoverImg from '@/components/CoverImg.vue'

defineProps<{ albums: AlbumItem[] }>()
defineEmits<{ open: [album: AlbumItem]; play: [album: AlbumItem] }>()
</script>

<template>
  <div class="grid gap-5" style="grid-template-columns: repeat(auto-fill, minmax(160px, 1fr))">
    <div
      v-for="album in albums"
      :key="album.id"
      class="group cursor-pointer"
      @dblclick="$emit('play', album)"
      @click="$emit('open', album)"
    >
      <div class="relative">
        <CoverImg :album-id="album.id" class="aspect-square w-full rounded-xl shadow-sm transition group-hover:shadow-lg" />
        <button
          class="absolute right-2 bottom-2 flex h-10 w-10 items-center justify-center rounded-full bg-violet-500 text-white shadow-lg transition hover:scale-105 hover:bg-violet-400"
          :class="album.trackCount === 0 ? 'hidden' : ''"
          title="播放专辑"
          @click.stop="$emit('play', album)"
        >
          <Play class="ml-0.5 h-4 w-4" fill="currentColor" />
        </button>
      </div>
      <p class="mt-2 truncate text-sm font-medium text-zinc-800 dark:text-zinc-100" :title="album.title">
        {{ album.title }}
      </p>
      <p class="truncate text-xs text-zinc-500 dark:text-zinc-400">
        {{ album.artist ?? '未知艺人' }}{{ album.year ? ` · ${album.year}` : '' }}
      </p>
    </div>
  </div>
</template>
