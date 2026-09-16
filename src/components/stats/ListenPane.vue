<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { api } from '@/api/commands'
import type {
  ListenBreakdownPoint,
  ListenDailyPoint,
  ListenHeatCell,
  ListenSummary,
  ListenTopItem,
} from '@/types'
import { useNav } from '@/composables/useNav'
import { errorText } from '@/i18n/error'
import { toast } from '@/composables/useToast'
import { BaseButtonGroup } from '@/components/ui'
import CoverImg from '@/components/CoverImg.vue'

const { t: tr } = useI18n()
const nav = useNav()

const summary = ref<ListenSummary | null>(null)
const streak = ref<[number, number]>([0, 0])
const granularity = ref<'day' | 'week' | 'month'>('day')
const trend = ref<ListenDailyPoint[]>([])
const heat = ref<ListenHeatCell[]>([])
const breakdownMode = ref<ListenBreakdownPoint[]>([])
const breakdownSource = ref<ListenBreakdownPoint[]>([])
const topKind = ref<'track' | 'artist' | 'album' | 'genre'>('track')
const topRange = ref<'week' | 'month' | 'all'>('month')
const top = ref<ListenTopItem[]>([])

const emit = defineEmits<{ loaded: [] }>()
onMounted(async () => {
  try {
    const [s, st, hm, bm, bs, top0, tr0] = await Promise.all([
      api.listenStatsSummary(),
      api.listenStreak(),
      api.listenHeatmap(),
      api.listenBreakdown('mode'),
      api.listenBreakdown('source'),
      api.listenTopTracks(topKind.value, topRange.value, 20),
      api.listenDaily(30, 'day'),
    ])
    summary.value = s
    streak.value = st
    heat.value = hm
    breakdownMode.value = bm
    breakdownSource.value = bs
    top.value = top0
    trend.value = tr0
    emit('loaded')
  } catch (e) {
    toast(errorText(e), 'error')
  }
})
watch(granularity, loadTrend)
watch([topKind, topRange], loadTop)

async function loadTrend() {
  try {
    trend.value = await api.listenDaily(granularity.value === 'day' ? 30 : granularity.value === 'week' ? 84 : 365, granularity.value)
  } catch (e) {
    toast(errorText(e), 'error')
  }
}
async function loadTop() {
  try {
    top.value = await api.listenTopTracks(topKind.value, topRange.value, 20)
  } catch (e) {
    toast(errorText(e), 'error')
  }
}

/** 秒 → 「X 小时 Y 分」/「Y 分 Z 秒」/「Z 秒」 */
function fmtSeconds(s: number): string {
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  const sec = Math.floor(s % 60)
  if (h > 0) return tr('stats.hoursMinutes', { h, m })
  if (m > 0) return tr('stats.minutesSeconds', { m, s: sec })
  return tr('stats.secondsOnly', { s: sec })
}

/** 趋势柱数据：day 粒度补零铺满 30 天；week/month 直接画非空桶 */
const trendBars = computed(() => {
  const map = new Map(trend.value.map((d) => [d.day, d.seconds]))
  const out: { key: string; label: string; seconds: number }[] = []
  if (granularity.value !== 'day') {
    for (const p of trend.value) {
      out.push({ key: p.day, label: p.day.replace(/^\d{4}-/, ''), seconds: p.seconds })
    }
    return out
  }
  const now = new Date()
  for (let i = 29; i >= 0; i--) {
    const d = new Date(now.getFullYear(), now.getMonth(), now.getDate() - i)
    const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
    out.push({ key, label: `${d.getMonth() + 1}/${d.getDate()}`, seconds: map.get(key) ?? 0 })
  }
  return out
})
const trendMax = computed(() => Math.max(1, ...trendBars.value.map((b) => b.seconds)))

/** 热力图矩阵：行 = 周一…周日（dow 1..6,0），列 = 0..23 点 */
const dowNames = computed(() => tr('stats.dowNames').split(','))
const heatMax = computed(() => Math.max(1, ...heat.value.map((c) => c.seconds)))
const heatRows = computed(() => {
  const map = new Map(heat.value.map((c) => [`${c.dow}:${c.hour}`, c.seconds]))
  return [1, 2, 3, 4, 5, 6, 0].map((dow) => ({
    dow,
    label: dowNames.value[(dow + 6) % 7] ?? '',
    cells: Array.from({ length: 24 }, (_, hour) => ({
      hour,
      seconds: map.get(`${dow}:${hour}`) ?? 0,
    })),
  }))
})

function heatAlpha(seconds: number): number {
  if (seconds <= 0) return 0
  return 0.15 + 0.85 * Math.min(1, seconds / heatMax.value)
}

/** 占比条数据：kind 显示名 + 占比 */
function breakdownRows(list: ListenBreakdownPoint[]) {
  const total = Math.max(1, list.reduce((a, b) => a + b.seconds, 0))
  return list.map((p) => ({
    ...p,
    label: kindLabel(p.kind),
    pct: Math.round((p.seconds / total) * 100),
  }))
}
function kindLabel(kind: string): string {
  const modeMap: Record<string, string> = {
    order: tr('stats.modeOrder'),
    loop: tr('stats.modeLoop'),
    one: tr('stats.modeOne'),
    shuffle: tr('stats.modeShuffle'),
    unknown: tr('stats.modeUnknown'),
  }
  return modeMap[kind] ?? (kind === 'local' ? tr('stats.srcLocal') : kind === 'webdav' ? tr('stats.srcWebdav') : kind)
}

const modeRows = computed(() => breakdownRows(breakdownMode.value))
const sourceRows = computed(() => breakdownRows(breakdownSource.value))

function fmtDate(ts: number | null): string {
  if (!ts) return '—'
  return new Date(ts * 1000).toLocaleDateString(localeDateKey(), { year: 'numeric', month: 'short', day: 'numeric' })
}
function localeDateKey(): string {
  return localStorage.getItem('lm.locale') === 'en' ? 'en-US' : 'zh-CN'
}

/** 榜单行点击：曲目/专辑跳专辑，艺人跳艺人页，流派不跳 */
function goItem(t: ListenTopItem) {
  if (t.kind === 'track' || t.kind === 'album') {
    if (t.albumId != null) nav.go({ view: 'tracks', albumId: t.albumId, albumTitle: t.kind === 'album' ? t.name : undefined })
  } else if (t.kind === 'artist') {
    nav.go({ view: 'tracks', artistId: t.id, artistName: t.name })
  }
}
</script>

<template>
  <div class="flex flex-col gap-6">
    <!-- 汇总卡 -->
    <div class="grid grid-cols-2 gap-3 lg:grid-cols-4">
      <div
        v-for="card in [
          { label: tr('stats.totalTime'), value: fmtSeconds(summary?.totalSeconds ?? 0) },
          { label: tr('stats.today'), value: fmtSeconds(summary?.todaySeconds ?? 0) },
          { label: tr('stats.week'), value: fmtSeconds(summary?.weekSeconds ?? 0) },
          { label: tr('stats.totalPlays'), value: String(summary?.totalTrackPlays ?? 0) },
          { label: tr('stats.effectivePlays'), value: String(summary?.effectivePlays ?? 0) },
          { label: tr('stats.uniqueTracks'), value: String(summary?.uniqueTracks ?? 0) },
          { label: tr('stats.uniqueArtists'), value: String(summary?.uniqueArtists ?? 0) },
          { label: tr('stats.streak'), value: String(streak[0]), sub: tr('stats.streakLongest', { n: streak[1] }) },
        ]"
        :key="card.label"
        class="rounded-xl border border-zinc-200 bg-white px-4 py-3.5 dark:border-zinc-800 dark:bg-zinc-900"
      >
        <p class="text-xs text-zinc-400">{{ card.label }}</p>
        <p class="mt-1 text-lg font-semibold tabular-nums text-zinc-900 dark:text-zinc-50">{{ card.value }}</p>
        <p v-if="'sub' in card && card.sub" class="mt-0.5 text-[11px] text-zinc-400">{{ card.sub }}</p>
      </div>
    </div>

    <!-- 收听趋势 -->
    <section class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ $t('stats.trendTitle') }}</h2>
        <BaseButtonGroup
          :model-value="granularity"
          :items="[
            { value: 'day', label: tr('stats.granDay') },
            { value: 'week', label: tr('stats.granWeek') },
            { value: 'month', label: tr('stats.granMonth') },
          ]"
          size="sm"
          @update:model-value="granularity = $event as typeof granularity"
        />
      </div>
      <div class="mt-4 flex h-36 items-end gap-[3px]">
        <div
          v-for="bar in trendBars"
          :key="bar.key"
          class="group flex min-w-0 flex-1 cursor-default flex-col items-center justify-end self-stretch"
          v-tooltip="`${bar.label} · ${fmtSeconds(bar.seconds)}`"
        >
          <div
            class="w-full rounded-t-sm bg-violet-500/70 transition-colors group-hover:bg-violet-500"
            :style="{ height: `${Math.max(bar.seconds > 0 ? 4 : 0, (bar.seconds / trendMax) * 100)}%` }"
          />
        </div>
      </div>
      <div v-if="trendBars.length" class="mt-1.5 flex gap-[3px] text-[10px] tabular-nums text-zinc-400">
        <span class="min-w-0 flex-1 text-left">{{ trendBars[0]?.label }}</span>
        <span class="min-w-0 flex-1 text-center">{{ trendBars[Math.floor(trendBars.length / 2)]?.label }}</span>
        <span class="min-w-0 flex-1 text-right">{{ trendBars[trendBars.length - 1]?.label }}</span>
      </div>
    </section>

    <!-- 星期 × 小时热力图 -->
    <section class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900">
      <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ $t('stats.heatTitle') }}</h2>
      <div class="mt-4 flex flex-col gap-1">
        <div
          v-for="row in heatRows"
          :key="row.dow"
          class="flex items-center gap-1"
        >
          <span class="w-8 shrink-0 text-right text-[11px] text-zinc-400">{{ row.label }}</span>
          <div class="grid min-w-0 flex-1 grid-cols-[repeat(24,minmax(0,1fr))] gap-[2px]">
            <div
              v-for="cell in row.cells"
              :key="cell.hour"
              class="h-5 cursor-default rounded-[3px]"
              :class="cell.seconds > 0 ? 'bg-violet-500' : 'bg-zinc-100 dark:bg-zinc-800'"
              :style="cell.seconds > 0 ? { opacity: heatAlpha(cell.seconds) } : undefined"
              v-tooltip="`${row.label} ${String(cell.hour).padStart(2, '0')}:00 · ${fmtSeconds(cell.seconds)}`"
            />
          </div>
        </div>
      </div>
    </section>

    <!-- 榜单 -->
    <section class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ $t('stats.topTitle') }}</h2>
        <div class="flex flex-wrap items-center gap-2">
          <BaseButtonGroup
            :model-value="topKind"
            :items="[
              { value: 'track', label: tr('stats.topKindTrack') },
              { value: 'artist', label: tr('stats.topKindArtist') },
              { value: 'album', label: tr('stats.topKindAlbum') },
              { value: 'genre', label: tr('stats.topKindGenre') },
            ]"
            size="sm"
            @update:model-value="topKind = $event as typeof topKind"
          />
          <BaseButtonGroup
            :model-value="topRange"
            :items="[
              { value: 'week', label: tr('stats.rangeWeek') },
              { value: 'month', label: tr('stats.rangeMonth') },
              { value: 'all', label: tr('stats.rangeAll') },
            ]"
            size="sm"
            @update:model-value="topRange = $event as typeof topRange"
          />
        </div>
      </div>

      <p v-if="!top.length" class="py-6 text-center text-sm text-zinc-400">{{ $t('stats.emptyRange') }}</p>
      <div v-else class="mt-3 flex flex-col">
        <button
          v-for="(t, i) in top"
          :key="`${t.kind}-${t.id}`"
          class="group flex items-center gap-3 rounded-lg px-2 py-1.5 text-left transition-colors hover:bg-zinc-100/80 disabled:cursor-default disabled:opacity-60 disabled:hover:bg-transparent dark:hover:bg-zinc-800/60"
          :disabled="t.kind === 'genre'"
          @click="goItem(t)"
        >
          <span
            class="w-6 shrink-0 text-center text-sm font-semibold tabular-nums"
            :class="i < 3 ? 'text-violet-500' : 'text-zinc-400'"
          >
            {{ i + 1 }}
          </span>
          <CoverImg
            v-if="t.albumId != null"
            :album-id="t.albumId"
            class="h-10 w-10 shrink-0"
            rounded="rounded-lg"
          />
          <span
            v-else
            class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-zinc-100 text-zinc-400 dark:bg-zinc-800"
          >
            {{ t.name.slice(0, 1) }}
          </span>
          <span class="min-w-0 flex-1">
            <span class="block truncate text-sm text-zinc-900 dark:text-zinc-100">{{ t.name }}</span>
            <span v-if="t.artist" class="block truncate text-xs text-zinc-400">{{ t.artist }}</span>
          </span>
          <span class="shrink-0 text-right text-xs tabular-nums text-zinc-500">
            <span class="block">{{ fmtSeconds(t.seconds) }}</span>
            <span class="block text-zinc-400">{{ tr('stats.playsWithCount', { count: t.plays }) }}</span>
          </span>
        </button>
      </div>
    </section>

    <!-- 播放模式 / 来源占比 -->
    <div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
      <section class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900">
        <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ $t('stats.byModeTitle') }}</h2>
        <div class="mt-3 flex flex-col gap-2">
          <div v-for="row in modeRows" :key="row.kind" class="flex items-center gap-2 text-xs">
            <span class="w-16 shrink-0 text-zinc-500">{{ row.label }}</span>
            <div class="h-2 min-w-0 flex-1 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
              <div class="h-full rounded-full bg-violet-500/70" :style="{ width: `${row.pct}%` }" />
            </div>
            <span class="w-24 shrink-0 text-right tabular-nums text-zinc-400">
              {{ row.pct }}% · {{ fmtSeconds(row.seconds) }}
            </span>
          </div>
        </div>
      </section>
      <section class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900">
        <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ $t('stats.bySourceTitle') }}</h2>
        <div class="mt-3 flex flex-col gap-2">
          <div v-for="row in sourceRows" :key="row.kind" class="flex items-center gap-2 text-xs">
            <span class="w-16 shrink-0 text-zinc-500">{{ row.label }}</span>
            <div class="h-2 min-w-0 flex-1 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
              <div class="h-full rounded-full bg-violet-500/70" :style="{ width: `${row.pct}%` }" />
            </div>
            <span class="w-24 shrink-0 text-right tabular-nums text-zinc-400">
              {{ row.pct }}% · {{ fmtSeconds(row.seconds) }}
            </span>
          </div>
        </div>
      </section>
    </div>

    <!-- 首末收听 -->
    <p class="text-center text-xs text-zinc-400">
      {{ $t('stats.firstListened') }} {{ fmtDate(summary?.firstListened ?? null) }}
      ·
      {{ $t('stats.lastListened') }} {{ fmtDate(summary?.lastListened ?? null) }}
      ·
      {{ tr('stats.listenDays', { n: summary?.listenDays ?? 0 }) }}
    </p>
  </div>
</template>
