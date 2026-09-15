<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted } from 'vue'
import { AltArrowDownIcon as ArrowDown } from '@solar-icons/vue/linear/alt-arrow-down'
import { LinkMinimalisticIcon as LinkIcon } from '@solar-icons/vue/linear/link-minimalistic'
import { RefreshIcon as LoaderCircle } from '@solar-icons/vue/linear/refresh'
import { RestartIcon as RotateCcw } from '@solar-icons/vue/linear/restart'
import { BaseButton } from '@/components/ui'
import { useUpdater } from '@/composables/useUpdater'
import { dialogOverlayClass, dialogPanelTransition, dialogDraggable } from '@/composables/useDialogPrefs'
import { openUrl } from '@tauri-apps/plugin-opener'

const updater = useUpdater()

/** 说明里的链接不放行 webview 导航：拦截后交系统浏览器打开（仅 http/https） */
async function onNotesClick(e: MouseEvent) {
  const anchor = (e.target as HTMLElement).closest('a')
  if (!anchor) return
  e.preventDefault()
  const href = anchor.getAttribute('href') ?? ''
  if (/^https?:\/\//i.test(href)) {
    await openUrl(href).catch(() => {})
  }
}

/** 下载进度百分比（总量未知时为 -1，显示不定进度） */
const progressPct = computed(() =>
  updater.status.value === 'downloading' && updater.progress.value >= 0
    ? Math.round(updater.progress.value * 100)
    : -1,
)

function onKey(e: KeyboardEvent) {
  if (!updater.dialogOpen.value) return
  if (e.key === 'Escape') updater.closeUpdateDialog()
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
      <div v-if="updater.dialogOpen.value" :class="dialogOverlayClass()" @click.self="updater.closeUpdateDialog()">
        <Transition v-bind="dialogPanelTransition">
          <div
            v-if="updater.dialogOpen.value"
            data-dialog-panel
            class="app-surface-blur w-[420px] rounded-2xl border border-white/15 bg-(--app-surface) p-5 shadow-2xl"
          >
            <div
              v-drag-dialog
              class="flex items-start gap-3"
              :class="dialogDraggable() ? 'cursor-move select-none' : ''"
            >
              <div class="mt-0.5 flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-violet-100 text-violet-500 dark:bg-violet-500/15">
                <ArrowDown class="h-4.5 w-4.5" />
              </div>
              <div class="min-w-0 flex-1">
                <h2 class="text-base font-semibold text-zinc-900 dark:text-zinc-50">{{ $t('settings.updateDialogTitle') }}</h2>
                <p class="mt-1 text-sm text-zinc-500 dark:text-zinc-300">
                  <template v-if="updater.currentVersion.value">{{ $t('settings.currentVersion') }} v{{ updater.currentVersion.value }} → </template>{{ $t('settings.newVersion') }}
                  <span class="font-medium text-violet-500">v{{ updater.newVersion.value }}</span>
                </p>
              </div>
            </div>

          <!-- 更新说明：优先富文本（GitHub 渲染的 HTML，Rust 侧已净化；样式见 style.css .release-notes），
               旧后端无该字段时回退纯文本 -->
          <div
            v-if="updater.releaseNotesHtml.value"
            class="release-notes mt-4 max-h-44 overflow-y-auto rounded-xl bg-zinc-50 p-3 text-xs dark:bg-zinc-900/60"
            v-html="updater.releaseNotesHtml.value"
            @click="onNotesClick"
          ></div>
          <div v-else-if="updater.releaseNotes.value" class="mt-4 max-h-44 overflow-y-auto rounded-xl bg-zinc-50 p-3 dark:bg-zinc-900/60">
            <p class="whitespace-pre-wrap text-xs leading-relaxed text-zinc-500 dark:text-zinc-300">{{ updater.releaseNotes.value }}</p>
          </div>
          <!-- 下载进度条（总量未知时显示不定进度动画） -->
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
          <p v-else-if="updater.status.value === 'ready'" class="mt-3 text-xs leading-relaxed text-zinc-400">{{ $t('settings.updateDownloadedHint') }}</p>
          <p v-else class="mt-3 text-xs leading-relaxed text-zinc-400">{{ $t('settings.goReleaseHint') }}</p>

          <div class="mt-5 flex justify-end gap-2">
            <BaseButton
              v-if="updater.status.value !== 'downloading'"
              variant="ghost"
              size="sm"
              @click="updater.closeUpdateDialog()"
            >
              {{ $t('settings.later') }}
            </BaseButton>
            <BaseButton
              v-if="updater.status.value === 'downloading'"
              size="sm"
              :icon="LoaderCircle"
              loading
            >
              {{ $t('settings.downloadingUpdate') }}
            </BaseButton>
            <BaseButton
              v-else-if="updater.status.value === 'ready'"
              size="sm"
              :icon="RotateCcw"
              @click="updater.installAndRestart()"
            >
              {{ $t('settings.installAndRestart') }}
            </BaseButton>
            <BaseButton
              v-else-if="updater.status.value === 'available'"
              size="sm"
              :icon="updater.canAutoUpdate() ? ArrowDown : LinkIcon"
              @click="updater.canAutoUpdate() ? updater.downloadUpdate() : updater.openReleasePage()"
            >
              {{ updater.canAutoUpdate() ? $t('settings.downloadUpdate') : $t('settings.goRelease') }}
            </BaseButton>
          </div>
        </div>
        </Transition>
      </div>
    </Transition>
  </Teleport>
</template>
