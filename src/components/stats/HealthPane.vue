<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { api } from '@/api/commands'
import type { LibraryHealth, NameCount, ScanHistoryItem } from '@/types'
import { errorText } from '@/i18n/error'
import { toast } from '@/composables/useToast'

const { t: tr } = useI18n()

const health = ref<LibraryHealth | null>(null)
const scans = ref<ScanHistoryItem[]>([])
const loading = ref(true)

const emit = defineEmits<{ loaded: [] }>()
onMounted(async () => {
  try {
    const [h, s] = await Promise.all([api.libraryHealth(), api.scanHistoryList()])
    health.value = h
    scans.value = s
  } catch (e) {
    toast(errorText(e), 'error')
  } finally {
    loading.value = false
    emit('loaded')
  }
})

function fmtSeconds(s: number): string {
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  return h > 0 ? tr('stats.hoursMinutes', { h, m }) : tr('stats.minutesSeconds', { m, s })
}
function fmtSize(bytes: number): string {
  if (bytes <= 0) return '—'
  const gb = bytes / 1024 ** 3
  if (gb >= 1) return tr('stats.sizeGb', { n: gb.toFixed(1) })
  return tr('stats.sizeMb', { n: Math.round(bytes / 1024 ** 2) })
}
function pct(part: number, total: number): number {
  return total > 0 ? Math.round((part / total) * 100) : 0
}
function scanDate(ts: number): string {
  const d = new Date(ts * 1000)
  return `${d.getMonth() + 1}/${d.getDate()} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

const overview = computed(() => [
  { label: tr('stats.hpTracks'), value: String(health.value?.tracks ?? 0) },
  { label: tr('stats.hpAlbums'), value: String(health.value?.albums ?? 0) },
  { label: tr('stats.hpArtists'), value: String(health.value?.artists ?? 0) },
  { label: tr('stats.hpGenres'), value: String(health.value?.genres ?? 0) },
  { label: tr('stats.hpTotalDuration'), value: fmtSeconds(health.value?.totalSeconds ?? 0) },
  { label: tr('stats.hpTotalSize'), value: fmtSize(health.value?.totalSize ?? 0) },
])

/** 覆盖率行：label + 百分比 + 细节小字 */
const coverage = computed(() => {
  const h = health.value
  if (!h) return []
  return [
    {
      label: tr('stats.covLyrics'),
      pct: pct(h.tracksWithLyrics, h.tracks),
      detail: tr('stats.covLyricsDetail', { e: h.lyricsEmbedded, x: h.lyricsExternal, q: h.lyricsQrc }),
    },
    {
      label: tr('stats.covCover'),
      pct: pct(h.tracksWithCover, h.tracks),
      detail: tr('stats.covCoverDetail', { a: h.albumsWithCover, t: h.tracksWithCover }),
    },
    {
      label: tr('stats.covMv'),
      pct: pct(h.mvCount, h.tracks),
      detail: tr('stats.covMvDetail', { n: h.mvCount }),
    },
    {
      label: tr('stats.covMeta'),
      pct: pct(
        h.meta.title + h.meta.artist + h.meta.album + h.meta.year + h.meta.genre + h.meta.trackNo,
        h.meta.total * 6,
      ),
      detail: '',
    },
  ]
})

/** 元数据六字段完整率（title 恒满，仍列出保持对称） */
const metaRows = computed(() => {
  const m = health.value?.meta
  if (!m) return []
  return [
    { label: tr('stats.metaTitle'), n: m.title },
    { label: tr('stats.metaArtist'), n: m.artist },
    { label: tr('stats.metaAlbum'), n: m.album },
    { label: tr('stats.metaYear'), n: m.year },
    { label: tr('stats.metaGenre'), n: m.genre },
    { label: tr('stats.metaTrackNo'), n: m.trackNo },
  ].map((r) => ({ ...r, pct: pct(r.n, m.total) }))
})

/** 分布条：截前 8 项 + 占比 */
function distRows(list: NameCount[] | undefined, translate: (name: string) => string = (n) => n) {
  const h = health.value
  const total = h?.tracks ?? 0
  return (list ?? []).slice(0, 8).map((r) => ({
    label: translate(r.name),
    count: r.count,
    pct: pct(r.count, total),
  }))
}
const sourceLabel = (kind: string) => (kind === 'local' ? tr('stats.srcLocal') : tr('stats.srcWebdav'))
const formatRows = computed(() => distRows(health.value?.formats))
const sourceRows = computed(() => distRows(health.value?.sources, sourceLabel))
const sampleRateRows = computed(() => distRows(health.value?.sampleRates, (n) => `${n} Hz`))
const bitDepthRows = computed(() => distRows(health.value?.bitDepths, (n) => `${n} bit`))
const yearRows = computed(() => distRows(health.value?.years?.map((y) => ({ name: String(y.year), count: y.count }))))
</script>

<template>
  <div class="flex flex-col gap-6">
    <!-- 概览卡 -->
    <div class="grid grid-cols-2 gap-3 lg:grid-cols-3 xl:grid-cols-6">
      <div
        v-for="card in overview"
        :key="card.label"
        class="rounded-xl border border-zinc-200 bg-white px-4 py-3.5 dark:border-zinc-800 dark:bg-zinc-900"
      >
        <p class="text-xs text-zinc-400">{{ card.label }}</p>
        <p class="mt-1 text-lg font-semibold tabular-nums text-zinc-900 dark:text-zinc-50">{{ card.value }}</p>
      </div>
    </div>

    <!-- 覆盖率 -->
    <section class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900">
      <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ $t('stats.coverageTitle') }}</h2>
      <div class="mt-4 grid grid-cols-1 gap-x-8 gap-y-3 lg:grid-cols-2">
        <div v-for="row in coverage" :key="row.label">
          <div class="flex items-center justify-between text-xs">
            <span class="text-zinc-600 dark:text-zinc-300">{{ row.label }}</span>
            <span class="tabular-nums text-zinc-400">{{ row.pct }}%</span>
          </div>
          <div class="mt-1 h-2 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
            <div class="h-full rounded-full bg-violet-500/70" :style="{ width: `${row.pct}%` }" />
          </div>
          <p v-if="row.detail" class="mt-1 text-[11px] text-zinc-400">{{ row.detail }}</p>
        </div>
      </div>
      <!-- 元数据完整率 -->
      <h3 class="mt-5 text-xs font-semibold text-zinc-500">{{ $t('stats.covMeta') }}</h3>
      <div class="mt-2 grid grid-cols-2 gap-x-8 gap-y-1.5 text-xs sm:grid-cols-3">
        <div v-for="row in metaRows" :key="row.label" class="flex items-center justify-between gap-2">
          <span class="text-zinc-500">{{ row.label }}</span>
          <span class="tabular-nums text-zinc-400">{{ row.pct }}%</span>
        </div>
      </div>
      <p v-if="health && health.metaIncomplete > 0" class="mt-3 text-[11px] text-zinc-400">
        {{ tr('stats.incompleteHint', { n: health.metaIncomplete }) }}
      </p>
    </section>

    <!-- 分布 -->
    <section class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900">
      <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ $t('stats.distTitle') }}</h2>
      <div class="mt-4 grid grid-cols-1 gap-x-8 gap-y-5 lg:grid-cols-2">
        <div v-for="group in [
          { title: tr('stats.distFormats'), rows: formatRows },
          { title: tr('stats.distSources'), rows: sourceRows },
          { title: tr('stats.distSampleRate'), rows: sampleRateRows },
          { title: tr('stats.distBitDepth'), rows: bitDepthRows },
          { title: tr('stats.distYears'), rows: yearRows },
        ]" :key="group.title">
          <h3 class="text-xs font-medium text-zinc-500">{{ group.title }}</h3>
          <div v-if="!group.rows.length" class="mt-2 text-xs text-zinc-400">{{ $t('stats.distEmpty') }}</div>
          <div v-else class="mt-2 flex flex-col gap-1.5">
            <div v-for="row in group.rows" :key="row.label" class="flex items-center gap-2 text-xs">
              <span class="w-20 shrink-0 truncate text-zinc-500">{{ row.label }}</span>
              <div class="h-2 min-w-0 flex-1 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
                <div class="h-full rounded-full bg-violet-500/70" :style="{ width: `${row.pct}%` }" />
              </div>
              <span class="w-12 shrink-0 text-right tabular-nums text-zinc-400">{{ row.count }}</span>
            </div>
          </div>
        </div>
      </div>
    </section>

    <!-- 扫描历史 -->
    <section class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900">
      <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ $t('stats.scanTitle') }}</h2>
      <p v-if="!scans.length" class="mt-2 text-xs text-zinc-400">{{ $t('stats.scanEmpty') }}</p>
      <div v-else class="mt-3 flex flex-col gap-1.5">
        <div
          v-for="s in scans"
          :key="s.at + s.sourceName"
          class="flex flex-wrap items-center justify-between gap-2 text-xs"
        >
          <span class="text-zinc-600 dark:text-zinc-300">{{ scanDate(s.at) }} · {{ s.sourceName }}</span>
          <span class="tabular-nums text-zinc-400">
            <span class="text-emerald-600 dark:text-emerald-400">+{{ s.added }}</span>
            ·
            <span class="text-violet-500 dark:text-violet-400">~{{ s.updated }}</span>
            ·
            <span class="text-red-500 dark:text-red-400">-{{ s.removed }}</span>
            ·
            {{ s.ms }} ms
          </span>
        </div>
      </div>
    </section>
  </div>
</template>
