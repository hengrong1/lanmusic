<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { MagicWand2Icon as MagicWand } from '@solar-icons/vue/linear/magic-wand-2'
import { RefreshIcon as Refresh } from '@solar-icons/vue/linear/refresh'
import { PlayIcon as Play } from '@solar-icons/vue/bold/play'
import TrackTable from '@/components/TrackTable.vue'
import EmptyState from '@/components/EmptyState.vue'
import { BaseButton } from '@/components/ui'
import { useNav } from '@/composables/useNav'
import { usePlayerStore } from '@/stores/player'
import { useStagger } from '@/composables/useStagger'
import { toast } from '@/composables/useToast'
import { api } from '@/api/commands'
import { errorText } from '@/i18n/error'
import type { Track } from '@/types'

// 智能歌单：不落库的动态曲目集合，按规则实时计算（Rust 侧 smart_playlist）。
// 顶部标签切换规则；规则本身记在路由里（smartKind），返回键可回到上一个规则。

const { t } = useI18n()
const nav = useNav()
const player = usePlayerStore()

const KINDS = ['recent', 'recentPlayed', 'frequent', 'favorite', 'neverPlayed', 'random'] as const
type Kind = (typeof KINDS)[number]

/** 单次展示上限（随机漫游等不设无限） */
const LIMIT = 500

const tracks = ref<Track[]>([])
const loading = ref(false)

const kind = computed<Kind>(() => {
  const k = nav.current.value.smartKind
  return (KINDS as readonly string[]).includes(k ?? '') ? (k as Kind) : 'recent'
})

const tabs = computed(() =>
  KINDS.map((k) => ({ value: k, label: t(`smart.kind.${k}`), hint: t(`smart.desc.${k}`) })),
)
const activeHint = computed(() => tabs.value.find((x) => x.value === kind.value)?.hint ?? '')

const root = ref<HTMLElement | null>(null)
useStagger(
  root,
  computed(() => !loading.value),
)

/** 带参翻译（模板 $t 无带参重载，须在 setup 内生成） */
const songCount = (n: number) => t('common.songsCount', { count: n })

let loadSeq = 0
async function load() {
  const my = ++loadSeq
  loading.value = true
  try {
    const items = await api.smartPlaylist(kind.value, LIMIT)
    if (my !== loadSeq) return
    tracks.value = items
  } catch (e) {
    if (my === loadSeq) toast(errorText(e), 'error')
  } finally {
    if (my === loadSeq) loading.value = false
  }
}

onMounted(load)
watch(kind, load)

function selectKind(k: Kind) {
  if (k === kind.value) return
  nav.go({ view: 'smart', smartKind: k })
}
function refresh() {
  void load()
}
function playAll() {
  if (tracks.value.length) player.playList(tracks.value, 0)
}
</script>

<template>
  <div ref="root" class="flex h-full min-h-0 flex-col">
    <div class="shrink-0 px-6 pt-5 pb-3">
      <div class="flex items-center gap-4">
        <div class="min-w-0 flex-1">
          <p data-stagger class="text-xs font-semibold tracking-wider text-violet-500 uppercase">{{ $t('nav.smart') }}</p>
          <h1 data-stagger class="mt-0.5 truncate text-2xl font-bold text-zinc-900 dark:text-zinc-50">
            {{ $t(`smart.kind.${kind}`) }}
          </h1>
          <p data-stagger class="mt-1 text-xs text-zinc-400">{{ activeHint }}</p>
        </div>
        <div class="flex shrink-0 items-center gap-2">
          <BaseButton data-stagger variant="outline" size="sm" :icon="Refresh" @click="refresh">
            {{ $t('common.refresh') }}
          </BaseButton>
          <BaseButton v-if="tracks.length" data-stagger variant="primary" size="sm" :icon="Play" @click="playAll">
            {{ $t('playlist.playAll') }}
          </BaseButton>
        </div>
      </div>

      <!-- 规则标签 -->
      <div data-stagger class="mt-3 flex flex-wrap gap-1.5">
        <button
          v-for="tab in tabs"
          :key="tab.value"
          type="button"
          class="rounded-lg border px-3 py-1.5 text-xs transition-colors"
          :class="
            tab.value === kind
              ? 'border-violet-500 bg-violet-500 text-white'
              : 'border-zinc-200 text-zinc-500 hover-accent-border dark:border-zinc-700 dark:text-zinc-300'
          "
          @click="selectKind(tab.value)"
        >
          {{ tab.label }}
        </button>
      </div>

      <p data-stagger class="mt-2 text-xs text-zinc-400">{{ songCount(tracks.length) }}</p>
    </div>

    <div v-if="loading" class="flex min-h-0 flex-1 items-center justify-center">
      <Refresh class="h-6 w-6 animate-spin text-violet-500" />
    </div>
    <div v-else-if="!tracks.length" class="min-h-0 flex-1">
      <EmptyState :icon="MagicWand" :title="$t('empty.smartTitle')" :description="$t('empty.smartHint')" />
    </div>
    <div v-else class="min-h-0 flex-1">
      <TrackTable :tracks="tracks" />
    </div>
  </div>
</template>
