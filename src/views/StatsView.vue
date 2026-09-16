<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { GraphUpIcon as GraphUp } from '@solar-icons/vue/linear/graph-up'
import { api } from '@/api/commands'
import type { ListenDailyPoint, ListenHourPoint, ListenSummary, ListenTopTrack } from '@/types'
import { useNav } from '@/composables/useNav'
import { useStagger } from '@/composables/useStagger'
import { errorText } from '@/i18n/error'
import { toast } from '@/composables/useToast'
import { BaseButtonGroup } from '@/components/ui'
import CoverImg from '@/components/CoverImg.vue'
import EmptyState from '@/components/EmptyState.vue'

const { t: tr } = useI18n()
const nav = useNav()
const root = ref<HTMLElement | null>(null)

const loading = ref(true)
const summary = ref<ListenSummary | null>(null)
const daily = ref<ListenDailyPoint[]>([])
const hourly = ref<ListenHourPoint[]>([])
const range = ref<'week' | 'month' | 'all'>('month')
const top = ref<ListenTopTrack[]>([])

useStagger(root, computed(() => !loading.value))

onMounted(refreshAll)
watch(range, loadTop)

async function refreshAll() {
  loading.value = true
  try {
    const [s, d, h] = await Promise.all([
      api.listenStatsSummary(),
      api.listenDaily(30),
      api.listenHourly(),
    ])
    summary.value = s
    daily.value = d
    hourly.value = h
    await loadTop()
  } catch (e) {
    toast(errorText(e), 'error')
  } finally {
    loading.value = false
  }
}

async function loadTop() {
  try {
    top.value = await api.listenTopTracks(range.value, 20)
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

/** 近 30 天逐日柱状数据：后端只回有记录的天，这里补零补齐到完整窗口 */
const dailyBars = computed(() => {
  const map = new Map(daily.value.map((d) => [d.day, d.seconds]))
  const out: { key: string; label: string; seconds: number }[] = []
  const now = new Date()
  for (let i = 29; i >= 0; i--) {
    const d = new Date(now.getFullYear(), now.getMonth(), now.getDate() - i)
    const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
    out.push({ key, label: `${d.getMonth() + 1}/${d.getDate()}`, seconds: map.get(key) ?? 0 })
  }
  return out
})
const dailyMax = computed(() => Math.max(1, ...dailyBars.value.map((b) => b.seconds)))

/** 24 小时分布：补齐 0-23 空桶 */
const hourlyBars = computed(() => {
  const map = new Map(hourly.value.map((h) => [h.hour, h.seconds]))
  return Array.from({ length: 24 }, (_, i) => {
    const key = String(i).padStart(2, '0')
    return { hour: key, seconds: map.get(key) ?? 0 }
  })
})
const hourlyMax = computed(() => Math.max(1, ...hourlyBars.value.map((b) => b.seconds)))

/** 点击榜单行：跳转到该曲所在专辑（无专辑信息的忽略） */
function goAlbum(t: ListenTopTrack) {
  if (t.albumId == null) return
  nav.go({ view: 'tracks', albumId: t.albumId, albumTitle: t.title })
}
</script>

<template>
  <div ref="root" class="flex h-full min-h-0 flex-col overflow-y-auto px-6 pb-8">
    <div class="flex shrink-0 flex-wrap items-center justify-between gap-3 pt-5 pb-4">
      <div>
        <h1 data-stagger class="mt-0.5 text-2xl font-bold text-zinc-900 dark:text-zinc-50">
          {{ $t('stats.title') }}
        </h1>
        <p data-stagger class="mt-1 text-xs text-zinc-400">{{ $t('stats.subtitle') }}</p>
      </div>
    </div>

    <!-- 无任何数据时的空态 -->
    <EmptyState
      v-if="summary && summary.totalSeconds === 0"
      :icon="GraphUp"
      :title="$t('stats.emptyTitle')"
      :description="$t('stats.emptyDesc')"
    />

    <div v-else class="flex min-w-0 flex-col gap-6">
      <!-- 汇总卡 -->
      <div class="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <div
          v-for="card in [
            { label: tr('stats.totalTime'), value: fmtSeconds(summary?.totalSeconds ?? 0) },
            { label: tr('stats.today'), value: fmtSeconds(summary?.todaySeconds ?? 0) },
            { label: tr('stats.week'), value: fmtSeconds(summary?.weekSeconds ?? 0) },
            { label: tr('stats.sessions'), value: String(summary?.totalPlays ?? 0) },
          ]"
          :key="card.label"
          data-stagger
          class="rounded-xl border border-zinc-200 bg-white px-4 py-3.5 dark:border-zinc-800 dark:bg-zinc-900"
        >
          <p class="text-xs text-zinc-400">{{ card.label }}</p>
          <p class="mt-1 text-lg font-semibold tabular-nums text-zinc-900 dark:text-zinc-50">
            {{ card.value }}
          </p>
        </div>
      </div>

      <!-- 近 30 天每日收听时长 -->
      <section
        data-stagger
        class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900"
      >
        <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">
          {{ $t('stats.dailyTitle') }}
        </h2>
        <div class="mt-4 flex h-36 items-end gap-[3px]">
          <div
            v-for="bar in dailyBars"
            :key="bar.key"
            class="group flex min-w-0 flex-1 cursor-default flex-col items-center justify-end self-stretch"
            v-tooltip="`${bar.label} · ${fmtSeconds(bar.seconds)}`"
          >
            <div
              class="w-full rounded-t-sm bg-violet-500/70 transition-colors group-hover:bg-violet-500"
              :style="{ height: `${Math.max(bar.seconds > 0 ? 4 : 0, (bar.seconds / dailyMax) * 100)}%` }"
            />
          </div>
        </div>
        <div class="mt-1.5 flex gap-[3px] text-[10px] tabular-nums text-zinc-400">
          <span class="min-w-0 flex-1 text-left">{{ dailyBars[0]?.label }}</span>
          <span>{{ dailyBars[14]?.label }}</span>
          <span class="min-w-0 flex-1 text-right">{{ dailyBars[29]?.label }}</span>
        </div>
      </section>

      <!-- 收听时段分布（0-23 点，两列） -->
      <section
        data-stagger
        class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900"
      >
        <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">
          {{ $t('stats.hourlyTitle') }}
        </h2>
        <div class="mt-4 grid grid-cols-1 gap-x-8 gap-y-1.5 sm:grid-cols-2">
          <div
            v-for="bar in hourlyBars"
            :key="bar.hour"
            class="flex items-center gap-2 text-xs tabular-nums text-zinc-400"
          >
            <span class="w-8 shrink-0 text-right">{{ bar.hour }}</span>
            <div class="h-2 min-w-0 flex-1 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
              <div
                class="h-full rounded-full bg-violet-500/70"
                :style="{ width: `${(bar.seconds / hourlyMax) * 100}%` }"
              />
            </div>
            <span class="w-20 shrink-0 truncate text-right">{{ fmtSeconds(bar.seconds) }}</span>
          </div>
        </div>
      </section>

      <!-- 听得最多的歌 -->
      <section
        data-stagger
        class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900"
      >
        <div class="flex flex-wrap items-center justify-between gap-2">
          <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">
            {{ $t('stats.topTitle') }}
          </h2>
          <BaseButtonGroup
            :model-value="range"
            :items="[
              { value: 'week', label: tr('stats.rangeWeek') },
              { value: 'month', label: tr('stats.rangeMonth') },
              { value: 'all', label: tr('stats.rangeAll') },
            ]"
            size="sm"
            @update:model-value="range = $event as typeof range"
          />
        </div>

        <p v-if="!top.length" class="py-6 text-center text-sm text-zinc-400">
          {{ $t('stats.emptyRange') }}
        </p>
        <div v-else class="mt-3 flex flex-col">
          <button
            v-for="(t, i) in top"
            :key="t.trackId"
            class="group flex items-center gap-3 rounded-lg px-2 py-1.5 text-left transition-colors hover:bg-zinc-100/80 disabled:opacity-40 dark:hover:bg-zinc-800/60"
            :disabled="t.albumId == null"
            @click="goAlbum(t)"
          >
            <span
              class="w-6 shrink-0 text-center text-sm font-semibold tabular-nums"
              :class="i < 3 ? 'text-violet-500' : 'text-zinc-400'"
            >
              {{ i + 1 }}
            </span>
            <CoverImg
              :album-id="t.albumId"
              class="h-10 w-10 shrink-0"
              rounded="rounded-lg"
            />
            <span class="min-w-0 flex-1">
              <span class="block truncate text-sm text-zinc-900 dark:text-zinc-100">
                {{ t.title }}
              </span>
              <span class="block truncate text-xs text-zinc-400">
                {{ t.artist ?? $t('album.unknownArtist') }}
              </span>
            </span>
            <span class="shrink-0 text-right text-xs tabular-nums text-zinc-500">
              <span class="block">{{ fmtSeconds(t.seconds) }}</span>
              <span class="block text-zinc-400">
                {{ tr('stats.playsWithCount', { count: t.plays }) }}
              </span>
            </span>
          </button>
        </div>
      </section>
    </div>
  </div>
</template>
