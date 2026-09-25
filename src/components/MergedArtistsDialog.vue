<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { CloseIcon as X } from '@solar-icons/vue/linear/close'
import { RefreshIcon as Refresh } from '@solar-icons/vue/linear/refresh'
import { BaseButton } from '@/components/ui'
import { useI18n } from 'vue-i18n'
import { errorText } from '@/i18n/error'
import { confirmDialog } from '@/composables/useConfirm'
import { toast } from '@/composables/useToast'
import { dialogOverlayClass, dialogPanelTransition, dialogDraggable } from '@/composables/useDialogPrefs'
import { api } from '@/api/commands'
import type { ArtistAlias } from '@/types'

/**
 * 已合并艺人弹窗：列出全部合并记录（旧名 → 主艺人），可逐条取消合并。
 * 取消合并按合并历史把曲目 / 专辑归属拆回；无历史的旧记录只恢复艺人本身
 * （曲目在重新扫描后按标签归位）。
 */
const props = defineProps<{ aliases: ArtistAlias[] }>()
const emit = defineEmits<{ close: []; unmerged: [] }>()

const { t } = useI18n()
const unmerging = ref<string | null>(null)

async function unmerge(a: ArtistAlias) {
  const ok = await confirmDialog({
    title: t('settings.artistUnmergeTitle'),
    message: t('settings.artistUnmergeConfirm', {
      alias: a.alias,
      artist: a.artistName,
      tracks: a.restorableTracks,
    }),
    confirmText: t('settings.artistUnmergeAction'),
  })
  if (!ok) return
  unmerging.value = a.alias
  try {
    const r = await api.unmergeArtist(a.alias)
    toast(
      t('settings.artistUnmergeDone', {
        name: r.artistName,
        tracks: r.restoredTracks,
        albums: r.restoredAlbums,
      }),
    )
    emit('unmerged')
  } catch (e) {
    toast(errorText(e), 'error')
  } finally {
    unmerging.value = null
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') emit('close')
}
onMounted(() => window.addEventListener('keydown', onKeydown))
onBeforeUnmount(() => window.removeEventListener('keydown', onKeydown))
</script>

<template>
  <!-- Teleport 到 body：脱离 app-surface-blur 主卡片 DOM 子树，避免 has-bg 映射误伤弹窗内部 -->
  <Teleport to="body">
    <div :class="[dialogOverlayClass(), 'p-6']" @click.self="emit('close')">
      <Transition v-bind="dialogPanelTransition" appear>
        <div
          v-focus-trap
          data-dialog-panel
          class="app-surface-blur flex w-full max-w-md flex-col overflow-hidden rounded-xl border border-white/15 bg-(--app-surface) shadow-2xl"
        >
          <!-- 标题栏（可拖动） -->
          <div
            v-drag-dialog
            class="flex shrink-0 items-center justify-between border-b border-zinc-200 px-5 py-4 dark:border-zinc-800"
            :class="dialogDraggable() ? 'cursor-move select-none' : ''"
          >
            <h2 class="text-base font-semibold text-zinc-900 dark:text-zinc-50">
              {{ t('settings.artistMergedDialogTitle', { count: props.aliases.length }) }}
            </h2>
            <BaseButton variant="ghost" size="xs" :icon="X" data-no-drag v-tooltip="$t('common.close')" :aria-label="$t('common.close')" @click="emit('close')" />
          </div>

          <!-- 合并记录列表 -->
          <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
            <p class="text-xs leading-relaxed text-zinc-400">{{ $t('settings.artistMergedDialogDesc') }}</p>
            <p v-if="!props.aliases.length" class="mt-3 text-xs text-zinc-400">
              {{ $t('settings.artistMergedEmpty') }}
            </p>
            <ul v-else class="mt-3 space-y-1.5">
              <li
                v-for="a in props.aliases"
                :key="`${a.alias}-${a.artistId}`"
                class="flex min-w-0 items-center gap-2 rounded-lg px-2 py-1.5 text-xs hover:bg-zinc-100 dark:hover:bg-zinc-800/60"
              >
                <div class="flex min-w-0 flex-1 flex-wrap items-baseline gap-x-1.5">
                  <span class="truncate text-zinc-400 line-through">{{ a.alias }}</span>
                  <span class="text-violet-500">→</span>
                  <span class="truncate font-medium text-zinc-700 dark:text-zinc-200">{{ a.artistName }}</span>
                  <span
                    v-if="a.restorableTracks > 0"
                    class="shrink-0 rounded-full bg-violet-50 px-1.5 py-0.5 text-[10px] text-violet-500 dark:bg-violet-500/10 dark:text-violet-300"
                  >
                    {{ t('settings.artistMergedRestorable', { count: a.restorableTracks }) }}
                  </span>
                </div>
                <BaseButton
                  variant="ghost"
                  size="xs"
                  :icon="Refresh"
                  :loading="unmerging === a.alias"
                  :disabled="unmerging !== null"
                  v-tooltip="$t('settings.artistUnmergeAction')"
                  :aria-label="$t('settings.artistUnmergeAction')"
                  @click="unmerge(a)"
                />
              </li>
            </ul>
          </div>

          <!-- 底部操作 -->
          <div class="flex shrink-0 items-center justify-between gap-2 border-t border-zinc-200 px-5 py-3 dark:border-zinc-800">
            <p class="min-w-0 truncate text-[10px] text-zinc-400">{{ $t('settings.artistUnmergeNoHistoryHint') }}</p>
            <BaseButton variant="ghost" size="sm" @click="emit('close')">{{ $t('common.close') }}</BaseButton>
          </div>
        </div>
      </Transition>
    </div>
  </Teleport>
</template>
