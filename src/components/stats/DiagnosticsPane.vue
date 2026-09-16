<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { api } from '@/api/commands'
import type { DiagnosticsSnapshot } from '@/types'
import { errorText } from '@/i18n/error'
import { toast } from '@/composables/useToast'

const { t: tr } = useI18n()

const snap = ref<DiagnosticsSnapshot | null>(null)
let timer: ReturnType<typeof setInterval> | null = null

const emit = defineEmits<{ loaded: [] }>()
onMounted(() => {
  refresh()
  // 轮询：运行时长/内存/CPU 是即时值；sysinfo 的 CPU 百分比第二次采样起才有效。
  // 5s 而非更短：snapshot 里有进程采样，别让轮询成为常态负载
  timer = setInterval(refresh, 5000)
})
onUnmounted(() => {
  if (timer) clearInterval(timer)
})

async function refresh() {
  try {
    snap.value = await api.diagnosticsSnapshot()
    emit('loaded')
  } catch (e) {
    toast(errorText(e), 'error')
    if (timer) clearInterval(timer)
  }
}

function fmtUptime(s: number): string {
  const m = Math.floor(s / 60)
  const h = Math.floor(m / 60)
  if (h > 0) return tr('stats.hoursMinutes', { h, m })
  return tr('stats.minutesSeconds', { m, s })
}
function successRate(requests: number, failures: number): string {
  if (requests === 0) return '—'
  return `${Math.round(((requests - failures) / requests) * 100)}%`
}
function avg(ms: number, count: number): string {
  return count > 0 ? `${Math.round(ms / count)} ms` : '—'
}

const infoRows = computed(() => [
  { label: tr('diag.appVersion'), value: snap.value?.appVersion ?? '—' },
  { label: tr('diag.tauriVersion'), value: snap.value?.tauriVersion ?? '—' },
  { label: tr('diag.webviewVersion'), value: snap.value?.webviewVersion ?? '—' },
  { label: tr('diag.os'), value: snap.value ? `${snap.value.os} (${snap.value.arch})` : '—' },
  { label: tr('diag.uptime'), value: snap.value ? fmtUptime(snap.value.uptimeSeconds) : '—' },
  { label: tr('diag.setupTime'), value: snap.value ? `${snap.value.setupMs} ms` : '—' },
  { label: tr('diag.memory'), value: snap.value ? `${snap.value.memoryMb.toFixed(1)} MB` : '—' },
  { label: tr('diag.cpu'), value: snap.value ? `${snap.value.cpuPercent.toFixed(1)}%` : '—' },
])

const webdavRows = computed(() => {
  const w = snap.value?.webdav
  if (!w) return []
  return [
    { label: tr('diag.requests'), value: String(w.requests) },
    { label: tr('diag.failures'), value: String(w.failures) },
    { label: tr('diag.successRate'), value: successRate(w.requests, w.failures) },
    { label: tr('diag.avgLatency'), value: avg(w.totalMs, w.requests) },
  ]
})

const coverRows = computed(() => {
  const c = snap.value?.covers
  if (!c) return []
  const attempts = c.cacheHits + c.cacheMisses
  return [
    { label: tr('diag.cacheHits'), value: String(c.cacheHits) },
    { label: tr('diag.cacheMisses'), value: String(c.cacheMisses) },
    { label: tr('diag.hitRate'), value: successRate(attempts, c.cacheMisses) },
    { label: tr('diag.extractOk'), value: String(c.extractOk) },
    { label: tr('diag.extractFail'), value: String(c.extractFail) },
    { label: tr('diag.avgLatency'), value: avg(c.totalMs, attempts) },
  ]
})

const lyricsRows = computed(() => {
  const l = snap.value?.lyrics
  if (!l) return []
  const total = l.ok + l.fail
  return [
    { label: tr('diag.ok'), value: String(l.ok) },
    { label: tr('diag.failures'), value: String(l.fail) },
    { label: tr('diag.successRate'), value: successRate(total, l.fail) },
    { label: tr('diag.avgLatency'), value: avg(l.totalMs, total) },
  ]
})

const latencyRows = computed(() => {
  const p = snap.value?.playLatency
  if (!p) return []
  return [
    { label: tr('stats.playsWithCount', { count: p.count }), value: `${p.avgMs} ms` },
    { label: tr('diag.maxLatency'), value: `${p.maxMs} ms` },
  ]
})

const errorRows = computed(() => [
  { label: tr('diag.panics'), value: String(snap.value?.panics ?? 0) },
  { label: tr('diag.frontendErrors'), value: String(snap.value?.frontendErrors ?? 0) },
])

const scanSegRows = computed(() => [
  { label: tr('diag.scanEnum'), value: `${snap.value?.lastScanEnumMs ?? 0} ms` },
  { label: tr('diag.scanParse'), value: `${snap.value?.lastScanParseMs ?? 0} ms` },
])
</script>

<template>
  <div class="flex flex-col gap-6">
    <p class="text-xs text-zinc-400">{{ $t('diag.hint') }}</p>

    <!-- 环境与版本 -->
    <section class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900">
      <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ $t('diag.envTitle') }}</h2>
      <div class="mt-3 grid grid-cols-1 gap-x-8 gap-y-1.5 text-xs sm:grid-cols-2">
        <div v-for="row in infoRows" :key="row.label" class="flex items-center justify-between gap-2">
          <span class="text-zinc-500">{{ row.label }}</span>
          <span class="truncate tabular-nums text-zinc-700 dark:text-zinc-200">{{ row.value }}</span>
        </div>
      </div>
    </section>

    <!-- WebDAV -->
    <section class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900">
      <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ $t('diag.webdavTitle') }}</h2>
      <div class="mt-3 grid grid-cols-2 gap-x-8 gap-y-1.5 text-xs sm:grid-cols-4">
        <div v-for="row in webdavRows" :key="row.label">
          <p class="text-zinc-400">{{ row.label }}</p>
          <p class="mt-0.5 text-base font-semibold tabular-nums text-zinc-900 dark:text-zinc-50">{{ row.value }}</p>
        </div>
      </div>
    </section>

    <!-- 封面 / 歌词 -->
    <div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
      <section class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900">
        <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ $t('diag.coversTitle') }}</h2>
        <div class="mt-3 flex flex-col gap-1.5 text-xs">
          <div v-for="row in coverRows" :key="row.label" class="flex items-center justify-between gap-2">
            <span class="text-zinc-500">{{ row.label }}</span>
            <span class="tabular-nums text-zinc-700 dark:text-zinc-200">{{ row.value }}</span>
          </div>
        </div>
      </section>
      <section class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900">
        <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ $t('diag.lyricsTitle') }}</h2>
        <div class="mt-3 flex flex-col gap-1.5 text-xs">
          <div v-for="row in lyricsRows" :key="row.label" class="flex items-center justify-between gap-2">
            <span class="text-zinc-500">{{ row.label }}</span>
            <span class="tabular-nums text-zinc-700 dark:text-zinc-200">{{ row.value }}</span>
          </div>
        </div>
      </section>
    </div>

    <!-- 播放启动延迟 / 错误 -->
    <div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
      <section class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900">
        <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ $t('diag.latencyTitle') }}</h2>
        <div class="mt-3 flex flex-col gap-1.5 text-xs">
          <div v-for="row in latencyRows" :key="row.label" class="flex items-center justify-between gap-2">
            <span class="text-zinc-500">{{ row.label }}</span>
            <span class="tabular-nums text-zinc-700 dark:text-zinc-200">{{ row.value }}</span>
          </div>
        </div>
      </section>
      <section class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900">
        <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ $t('diag.errorsTitle') }}</h2>
        <div class="mt-3 flex flex-col gap-1.5 text-xs">
          <div v-for="row in errorRows" :key="row.label" class="flex items-center justify-between gap-2">
            <span class="text-zinc-500">{{ row.label }}</span>
            <span class="tabular-nums" :class="row.value !== '0' ? 'text-red-500' : 'text-zinc-700 dark:text-zinc-200'">
              {{ row.value }}
            </span>
          </div>
        </div>
      </section>
    </div>

    <!-- 最近一次扫描分段 -->
    <section class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900">
      <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ $t('diag.lastScanSeg') }}</h2>
      <div class="mt-3 grid grid-cols-2 gap-x-8 gap-y-1.5 text-xs">
        <div v-for="row in scanSegRows" :key="row.label" class="flex items-center justify-between gap-2">
          <span class="text-zinc-500">{{ row.label }}</span>
          <span class="tabular-nums text-zinc-700 dark:text-zinc-200">{{ row.value }}</span>
        </div>
      </div>
    </section>
  </div>
</template>
