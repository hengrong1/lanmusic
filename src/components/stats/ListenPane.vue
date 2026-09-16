<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  CalendarIcon as Calendar,
  CalendarMarkIcon as CalendarMark,
  ClockCircleIcon as ClockCircle,
  FireIcon as Fire,
  GraphUpIcon as GraphUp,
  MicrophoneIcon as Microphone,
  MusicNoteIcon as MusicNote,
  PlayIcon as Play,
  VerifiedCheckIcon as VerifiedCheck,
} from '@solar-icons/vue/linear'
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
import EmptyState from '@/components/EmptyState.vue'

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

/** 汇总卡（带图标）：数值为 0 也显示，整体空态由 isEmpty 决定 */
const cards = computed(() => [
  { icon: ClockCircle, label: tr('stats.totalTime'), value: fmtSeconds(summary.value?.totalSeconds ?? 0) },
  { icon: CalendarMark, label: tr('stats.today'), value: fmtSeconds(summary.value?.todaySeconds ?? 0) },
  { icon: Calendar, label: tr('stats.week'), value: fmtSeconds(summary.value?.weekSeconds ?? 0) },
  { icon: Play, label: tr('stats.totalPlays'), value: String(summary.value?.totalTrackPlays ?? 0) },
  { icon: VerifiedCheck, label: tr('stats.effectivePlays'), value: String(summary.value?.effectivePlays ?? 0) },
  { icon: MusicNote, label: tr('stats.uniqueTracks'), value: String(summary.value?.uniqueTracks ?? 0) },
  { icon: Microphone, label: tr('stats.uniqueArtists'), value: String(summary.value?.uniqueArtists ?? 0) },
  {
    icon: Fire,
    label: tr('stats.streak'),
    value: String(streak.value[0]),
    sub: tr('stats.streakLongest', { n: streak.value[1] }),
  },
])

/** 全空判定：从未有过收听流水 → 展示空态引导 */
const isEmpty = computed(() => (summary.value?.totalSeconds ?? 0) === 0)

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
const trendPeak = computed(() => fmtSeconds(Math.max(0, ...trendBars.value.map((b) => b.seconds))))

/** 热力图矩阵：行 = 周一…周日（dow 1..6,0），列 = 0..23 点；GitHub 风格 5 档色 */
const dowNames = computed(() => tr('stats.dowNames').split(','))
const heatMax = computed(() => Math.max(1, ...heat.value.map((c) => c.seconds)))
const heatRows = computed(() => {
  const map = new Map(heat.value.map((c) => [`${c.dow}:${c.hour}`, c.seconds]))
  return [1, 2, 3, 4, 5, 6, 0].map((dow) => ({
    dow,
    label: dowNames.value[(dow + 6) % 7] ?? '',
    cells: Array.from({ length: 24 }, (_, hour) => ({
      hour,
      level: heatLevel(map.get(`${dow}:${hour}`) ?? 0),
    })),
  }))
})
function heatLevel(seconds: number): number {
  if (seconds <= 0) return 0
  const r = seconds / heatMax.value
  if (r <= 0.25) return 1
  if (r <= 0.5) return 2
  if (r <= 0.75) return 3
  return 4
}
const HEAT_CLASSES = [
  'bg-zinc-100 dark:bg-zinc-800',
  'bg-violet-500/25',
  'bg-violet-500/45',
  'bg-violet-500/70',
  'bg-violet-500',
]

/** 占比条分色：各项独立颜色，视觉可分 */
const BAR_COLORS = ['bg-violet-500', 'bg-sky-500', 'bg-amber-500', 'bg-emerald-500', 'bg-rose-500', 'bg-zinc-400']
function breakdownRows(list: ListenBreakdownPoint[]) {
  const total = Math.max(1, list.reduce((a, b) => a + b.seconds, 0))
  return list.map((p, i) => ({
    ...p,
    label: kindLabel(p.kind),
    pct: Math.round((p.seconds / total) * 100),
    color: BAR_COLORS[i % BAR_COLORS.length],
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
  return new Date(ts * 1000).toLocaleDateString(localStorage.getItem('lm.locale') === 'en' ? 'en-US' : 'zh-CN', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  })
}

/** 榜单前三名奖牌色（金/银/铜），其余灰色数字 */
const MEDALS = [
  'bg-amber-400 text-white',
  'bg-zinc-300 text-zinc-600 dark:bg-zinc-600 dark:text-zinc-100',
  'bg-orange-400/90 text-white',
]

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
    <!-- 汇总卡（图标 + 数值） -->
    <div class="grid grid-cols-2 gap-3 lg:grid-cols-4">
      <div
        v-for="card in cards"
        :key="card.label"
        class="flex items-start gap-3 rounded-xl border border-zinc-200 bg-white px-4 py-3.5 dark:border-zinc-800 dark:bg-zinc-900"
      >
        <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-violet-500/10">
          <component :is="card.icon" class="h-4.5 w-4.5 text-violet-500" :stroke-width="1.5" />
        </div>
        <div class="min-w-0">
          <p class="truncate text-xs text-zinc-400">{{ card.label }}</p>
          <p class="mt-0.5 text-lg font-semibold tabular-nums leading-tight text-zinc-900 dark:text-zinc-50">
            {{ card.value }}
          </p>
          <p v-if="'sub' in card && card.sub" class="mt-0.5 text-[11px] text-zinc-400">{{ card.sub }}</p>
        </div>
      </div>
    </div>

    <!-- 空态：从未收听时不再展示后续图表 -->
    <EmptyState
      v-if="isEmpty"
      :icon="GraphUp"
      :title="$t('stats.emptyTitle')"
      :description="$t('stats.emptyDesc')"
    />

    <template v-else>
      <!-- 收听趋势 -->
      <section class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900">
        <div class="flex flex-wrap items-center justify-between gap-2">
          <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ $t('stats.trendTitle') }}</h2>
          <div class="flex flex-wrap items-center gap-3">
            <span class="text-xs tabular-nums text-zinc-400">
              {{ $t('stats.peak') }} <span class="font-medium text-zinc-600 dark:text-zinc-300">{{ trendPeak }}</span>
            </span>
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
        </div>
        <div class="mt-4 flex h-36 items-end gap-[3px]">
          <div
            v-for="bar in trendBars"
            :key="bar.key"
            class="group flex min-w-0 flex-1 cursor-default flex-col items-center justify-end self-stretch"
            v-tooltip="`${bar.label} · ${fmtSeconds(bar.seconds)}`"
          >
            <div
              class="w-full rounded-t-sm bg-gradient-to-t from-violet-500 to-violet-400 transition-colors group-hover:from-violet-600 group-hover:to-violet-500"
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
        <div class="flex flex-wrap items-center justify-between gap-2">
          <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ $t('stats.heatTitle') }}</h2>
          <div class="flex items-center gap-1 text-[10px] text-zinc-400">
            <span>{{ $t('stats.heatLess') }}</span>
            <span v-for="(cls, i) in HEAT_CLASSES" :key="i" class="h-3 w-3 rounded-[3px]" :class="cls" />
            <span>{{ $t('stats.heatMore') }}</span>
          </div>
        </div>
        <div class="mt-4 flex flex-col gap-1">
          <div v-for="row in heatRows" :key="row.dow" class="flex items-center gap-1">
            <span class="w-8 shrink-0 text-right text-[11px] text-zinc-400">{{ row.label }}</span>
            <div class="grid min-w-0 flex-1 grid-cols-[repeat(24,minmax(0,1fr))] gap-[2px]">
              <div
                v-for="cell in row.cells"
                :key="cell.hour"
                class="h-5 cursor-default rounded-[3px]"
                :class="HEAT_CLASSES[cell.level]"
                v-tooltip="`${row.label} ${String(cell.hour).padStart(2, '0')}:00 · ${fmtSeconds(Math.round((cell.level / 4) * heatMax))}`"
              />
            </div>
          </div>
          <!-- 小时刻度 -->
          <div class="flex items-center gap-1">
            <span class="w-8 shrink-0" />
            <div class="grid min-w-0 flex-1 grid-cols-[repeat(24,minmax(0,1fr))] gap-[2px] text-[9px] tabular-nums text-zinc-400">
              <span v-for="h in 24" :key="h" class="text-center">
                {{ [0, 6, 12, 18, 23].includes(h - 1) ? h - 1 : '' }}
              </span>
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
              class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full text-xs font-bold tabular-nums"
              :class="i < 3 ? MEDALS[i] : 'text-sm font-semibold text-zinc-400'"
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
              class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-violet-500/10 text-sm font-medium text-violet-500"
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
          <div class="mt-3 flex flex-col gap-2.5">
            <div v-for="row in modeRows" :key="row.kind" class="flex items-center gap-2 text-xs">
              <span class="w-16 shrink-0 text-zinc-500">{{ row.label }}</span>
              <div class="h-2.5 min-w-0 flex-1 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
                <div class="h-full rounded-full" :class="row.color" :style="{ width: `${row.pct}%` }" />
              </div>
              <span class="w-24 shrink-0 text-right tabular-nums text-zinc-400">
                {{ row.pct }}% · {{ fmtSeconds(row.seconds) }}
              </span>
            </div>
          </div>
        </section>
        <section class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900">
          <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ $t('stats.bySourceTitle') }}</h2>
          <div class="mt-3 flex flex-col gap-2.5">
            <div v-for="row in sourceRows" :key="row.kind" class="flex items-center gap-2 text-xs">
              <span class="w-16 shrink-0 text-zinc-500">{{ row.label }}</span>
              <div class="h-2.5 min-w-0 flex-1 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
                <div class="h-full rounded-full" :class="row.color" :style="{ width: `${row.pct}%` }" />
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
    </template>
  </div>
</template>
