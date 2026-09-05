<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted } from 'vue'
import { AltArrowDownIcon as ArrowDown } from '@solar-icons/vue/linear/alt-arrow-down'
import { RefreshIcon as LoaderCircle } from '@solar-icons/vue/linear/refresh'
import { RestartIcon as RotateCcw } from '@solar-icons/vue/linear/restart'
import { useUpdater } from '@/composables/useUpdater'

const updater = useUpdater()

/** 下载进度百分比（total 未知时为 -1，显示不定进度） */
const progressPct = computed(() =>
  updater.status.value === 'downloading' && updater.progress.value >= 0
    ? Math.round(updater.progress.value * 100)
    : -1,
)

/** 下载中禁止关闭弹窗（避免误触关闭后丢失进度提示；进度仍在后台继续） */
function dismiss(): void {
  if (updater.status.value !== 'downloading') updater.closeUpdateDialog()
}

function onKey(e: KeyboardEvent) {
  if (!updater.dialogOpen.value) return
  if (e.key === 'Escape') dismiss()
}
onMounted(() => window.addEventListener('keydown', onKey))
onBeforeUnmount(() => window.removeEventListener('keydown', onKey))
</script>

<template>
  <Teleport to="body">
    <Transition
      enter-active-class="transition duration-150 ease-out"
      enter-from-class="opacity-0"
      enter-to-class="opacity-100"
      leave-active-class="transition duration-100 ease-in"
      leave-to-class="opacity-0"
    >
      <div
        v-if="updater.dialogOpen.value"
        class="fixed inset-0 z-[70] flex items-center justify-center bg-black/40 backdrop-blur-sm"
        @click.self="dismiss"
      >
        <div class="w-[420px] rounded-2xl border border-zinc-200 bg-white p-5 shadow-2xl dark:border-zinc-700 dark:bg-zinc-800">
          <div class="flex items-start gap-3">
            <div class="mt-0.5 flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-violet-100 text-violet-500 dark:bg-violet-500/15">
              <ArrowDown class="h-4.5 w-4.5" />
            </div>
            <div class="min-w-0 flex-1">
              <h2 class="text-base font-semibold text-zinc-900 dark:text-zinc-50">发现新版本</h2>
              <p class="mt-1 text-sm text-zinc-500 dark:text-zinc-300">
                <template v-if="updater.currentVersion.value">当前版本 v{{ updater.currentVersion.value }} → </template>新版本
                <span class="font-medium text-violet-500">v{{ updater.newVersion.value }}</span>
              </p>
            </div>
          </div>

          <!-- 更新版说明 -->
          <div v-if="updater.releaseNotes.value" class="mt-4 max-h-44 overflow-y-auto rounded-xl bg-zinc-50 p-3 dark:bg-zinc-900/60">
            <p class="whitespace-pre-wrap text-xs leading-relaxed text-zinc-500 dark:text-zinc-300">{{ updater.releaseNotes.value }}</p>
          </div>

          <!-- 下载进度条（total 未知时显示不定进度动画） -->
          <div v-if="updater.status.value === 'downloading'" class="mt-4 flex items-center gap-2">
            <div class="h-1.5 flex-1 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
              <div
                v-if="progressPct >= 0"
                class="h-full rounded-full bg-violet-500 transition-all"
                :style="{ width: `${progressPct}%` }"
              ></div>
              <div v-else class="h-full w-1/3 animate-pulse rounded-full bg-violet-400"></div>
            </div>
            <span class="shrink-0 text-xs tabular-nums text-zinc-400">
              {{ progressPct >= 0 ? `${progressPct}%` : `${updater.downloadedMb.value.toFixed(1)}MB` }}
            </span>
          </div>
          <p v-if="updater.status.value === 'ready'" class="mt-4 text-xs text-zinc-400">更新已下载完成，重启应用后生效。</p>

          <div class="mt-5 flex justify-end gap-2">
            <button
              v-if="updater.status.value !== 'downloading'"
              class="cursor-pointer rounded-full px-4 py-1.5 text-sm text-zinc-600 transition hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-700"
              @click="dismiss"
            >
              稍后
            </button>
            <button
              v-if="updater.status.value === 'available'"
              class="flex cursor-pointer items-center gap-1.5 rounded-full bg-violet-500 px-4 py-1.5 text-sm font-medium text-white transition hover:bg-violet-400"
              @click="updater.downloadAndInstall()"
            >
              <ArrowDown class="h-3.5 w-3.5" />
              立即更新
            </button>
            <button
              v-else-if="updater.status.value === 'downloading'"
              class="flex cursor-not-allowed items-center gap-1.5 rounded-full bg-violet-500 px-4 py-1.5 text-sm font-medium text-white opacity-80"
              disabled
            >
              <LoaderCircle class="h-3.5 w-3.5 animate-spin" />
              正在下载…
            </button>
            <button
              v-else-if="updater.status.value === 'ready'"
              class="flex cursor-pointer items-center gap-1.5 rounded-full bg-violet-500 px-4 py-1.5 text-sm font-medium text-white transition hover:bg-violet-400"
              @click="updater.restartToUpdate()"
            >
              <RotateCcw class="h-3.5 w-3.5" />
              重启应用
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
