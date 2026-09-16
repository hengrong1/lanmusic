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
import { ensureEcharts, useChartTheme, CHART_COLORS } from '@/composables/useEcharts'
import { useEChart } from '@/composables/useEChart'
import { errorText } from '@/i18n/error'
import { toast } from '@/composables/useToast'
import { BaseButtonGroup } from '@/components/ui'
import CoverImg from '@/components/CoverImg.vue'
import EmptyState from '@/components/EmptyState.vue'

ensureEcharts()

const { t: tr } = useI18n()
const nav = useNav()
const { axisLabel, axisLine, splitLine, tooltipBase, cardBg, emptyText } = useChartTheme()

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
/**
 * 图表挂载门控：dataReady = 首次数据就绪（汇总卡/空态判定依赖）。
 * 图表 init 由 useEChart 门控（容器尺寸非 0 才 init），这里不再需要 chartsReady。
 */
const dataReady = ref(false)
// 图表容器 refs（useEChart 逐实例接管：init 门控 / setOption / resize / dispose）
const trendEl = ref<HTMLElement | null>(null)
const heatEl = ref<HTMLElement | null>(null)
const modeEl = ref<HTMLElement | null>(null)
const sourceEl = ref<HTMLElement | null>(null)
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
    dataReady.value = true
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

/** 全空判定：从未有过收听流水 → 展示空态引导 */
const isEmpty = computed(() => (summary.value?.totalSeconds ?? 0) === 0)

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

/** 趋势数据：day 粒度补零铺满 30 天；week/month 直接用非空桶（label 去掉年份前缀） */
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
const trendPeak = computed(() => fmtSeconds(Math.max(0, ...trendBars.value.map((b) => b.seconds))))

/** Y 轴刻度：秒 → 可读时长（h / m） */
function fmtYTick(v: number): string {
  if (v >= 3600) return `${(v / 3600).toFixed(1)}h`
  if (v >= 60) return `${Math.round(v / 60)}m`
  return `${v}s`
}

const trendOption = computed(() => ({
  grid: { left: 8, right: 8, top: 16, bottom: 0, containLabel: true },
  tooltip: {
    trigger: 'axis',
    axisPointer: { type: 'shadow' },
    ...tooltipBase.value,
    formatter: (ps: { name: string; value: number }[]) => `${ps[0]?.name}<br/>${fmtSeconds(ps[0]?.value ?? 0)}`,
  },
  xAxis: {
    type: 'category',
    data: trendBars.value.map((b) => b.label),
    axisLabel: { color: axisLabel.value, fontSize: 10, hideOverlap: true },
    axisLine: { lineStyle: { color: axisLine.value } },
    axisTick: { show: false },
  },
  yAxis: {
    type: 'value',
    axisLabel: { color: axisLabel.value, fontSize: 10, formatter: fmtYTick },
    splitLine: { lineStyle: { color: splitLine.value, type: 'dashed' } },
  },
  series: [
    {
      type: 'bar',
      data: trendBars.value.map((b) => b.seconds),
      barCategoryGap: '25%',
      itemStyle: {
        borderRadius: [3, 3, 0, 0],
        color: {
          type: 'linear',
          x: 0,
          y: 0,
          x2: 0,
          y2: 1,
          colorStops: [
            { offset: 0, color: '#8b5cf6' },
            { offset: 1, color: '#c4b5fd' },
          ],
        },
      },
    },
  ],
}))

/** 热力图：星期（行）× 小时（列），铺满 7×24 补零；色带随明暗主题 */
const dowNames = computed(() => tr('stats.dowNames').split(','))
const heatOption = computed(() => {
  const map = new Map(heat.value.map((c) => [`${c.dow}:${c.hour}`, c.seconds]))
  const dows = [1, 2, 3, 4, 5, 6, 0] // 行序：周一…周日（yAxis inverse 后首行在顶）
  const data: [number, number, number][] = []
  dows.forEach((dow, yi) => {
    for (let hh = 0; hh < 24; hh++) data.push([hh, yi, map.get(`${dow}:${hh}`) ?? 0])
  })
  const max = Math.max(1, ...heat.value.map((c) => c.seconds))
  return {
    grid: { left: 8, right: 8, top: 6, bottom: 0, containLabel: true },
    tooltip: {
      ...tooltipBase.value,
      formatter: (p: { value: [number, number, number] }) =>
        `${dowNames.value[p.value[1]] ?? ''} ${String(p.value[0]).padStart(2, '0')}:00 · ${fmtSeconds(p.value[2])}`,
    },
    xAxis: {
      type: 'category',
      data: Array.from({ length: 24 }, (_, h) => String(h)),
      axisLabel: { color: axisLabel.value, fontSize: 10, interval: 5 },
      axisLine: { show: false },
      axisTick: { show: false },
    },
    yAxis: {
      type: 'category',
      data: dowNames.value,
      inverse: true,
      axisLabel: { color: axisLabel.value, fontSize: 11 },
      axisLine: { show: false },
      axisTick: { show: false },
    },
    visualMap: {
      show: false,
      min: 0,
      max,
      inRange: {
        color: axisLabel.value === '#a1a1aa'
          ? ['#27272a', '#312e81', '#5b21b6', '#7c3aed', '#8b5cf6']
          : ['#f4f4f5', '#ede9fe', '#ddd6fe', '#a78bfa', '#7c3aed'],
      },
    },
    series: [
      {
        type: 'heatmap',
        data,
        itemStyle: { borderRadius: 3, borderColor: cardBg.value, borderWidth: 1 },
      },
    ],
  }
})

/** 占比环形图：by=mode/source 各一个；空数据返回 null 由模板隐藏 */
function donutOption(list: ListenBreakdownPoint[]) {
  const total = list.reduce((a, b) => a + b.seconds, 0)
  if (!list.length || total <= 0) return null
  return {
    tooltip: {
      trigger: 'item',
      ...tooltipBase.value,
      formatter: (p: { name: string; percent: number; value: number }) =>
        `${p.name}<br/>${p.percent}% · ${fmtSeconds(p.value)}`,
    },
    legend: {
      bottom: 0,
      left: 'center',
      itemWidth: 10,
      itemHeight: 10,
      icon: 'circle',
      textStyle: { color: axisLabel.value, fontSize: 11 },
    },
    series: [
      {
        type: 'pie',
        radius: ['52%', '78%'],
        center: ['50%', '44%'],
        itemStyle: { borderRadius: 4, borderColor: cardBg.value, borderWidth: 2 },
        label: { show: false },
        emphasis: { scaleSize: 4 },
        data: list.map((p, i) => ({
          name: kindLabel(p.kind),
          value: p.seconds,
          itemStyle: { color: CHART_COLORS[i % CHART_COLORS.length] },
        })),
      },
    ],
  }
}
const modeDonut = computed(() => donutOption(breakdownMode.value))
const sourceDonut = computed(() => donutOption(breakdownSource.value))

// ECharts 实例接管（init 门控 / option 增量 / resize / dispose）
useEChart(trendEl, trendOption)
useEChart(heatEl, heatOption)
useEChart(modeEl, modeDonut)
useEChart(sourceEl, sourceDonut)

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
    <!-- 数据就绪后渲染；图表 init 由 useEChart 门控（容器尺寸非 0 才挂实例） -->
    <template v-if="dataReady">
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
        <div ref="trendEl" class="h-56 w-full" />
      </section>

      <!-- 星期 × 小时热力图 -->
      <section class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900">
        <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ $t('stats.heatTitle') }}</h2>
        <div ref="heatEl" class="h-52 w-full" />
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

      <!-- 播放模式 / 来源占比（环形图） -->
      <div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
        <section class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900">
          <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ $t('stats.byModeTitle') }}</h2>
          <div v-if="modeDonut" ref="modeEl" class="h-52 w-full" />
          <p v-else class="flex h-52 items-center justify-center text-sm" :style="{ color: emptyText }">
            {{ $t('stats.emptyRange') }}
          </p>
        </section>
        <section class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900">
          <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ $t('stats.bySourceTitle') }}</h2>
          <div v-if="sourceDonut" ref="sourceEl" class="h-52 w-full" />
          <p v-else class="flex h-52 items-center justify-center text-sm" :style="{ color: emptyText }">
            {{ $t('stats.emptyRange') }}
          </p>
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
    </template>
  </div>
</template>
