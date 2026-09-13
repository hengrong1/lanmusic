<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { TrashBin2Icon as Trash2 } from '@solar-icons/vue/linear/trash-bin-2'
import { CloseIcon as X } from '@solar-icons/vue/linear/close'
import CoverImg from '@/components/CoverImg.vue'
import { useLibraryStore } from '@/stores/library'
import { confirmDialog } from '@/composables/useConfirm'
import { toast } from '@/composables/useToast'
import { BaseInput, BaseTextarea, BaseButton } from '@/components/ui'
import { useI18n } from 'vue-i18n'
import { errorText } from '@/i18n/error'
import { dialogOverlayClass, dialogPanelTransition, dialogDraggable } from '@/composables/useDialogPrefs'

/**
 * 编辑歌单弹层：集中修改名称、简介；只读展示创建时间 / 歌曲数 / 封面；删除歌单。
 * 保存时只提交有变化的字段；删除成功后 emit('deleted') 由上层负责跳转。
 */
const props = defineProps<{ playlistId: number }>()
const emit = defineEmits<{ close: []; saved: [name?: string]; deleted: [] }>()

const { t, locale } = useI18n()
const library = useLibraryStore()

const meta = computed(() => library.playlists.find((p) => p.id === props.playlistId) ?? null)
const metaDesc = computed(() => meta.value?.description ?? '')
const createdText = computed(() => {
  const ts = meta.value?.createdAt
  return ts
    ? new Date(ts * 1000).toLocaleDateString(locale.value === 'zh' ? 'zh-CN' : 'en-US', {
        year: 'numeric',
        month: 'long',
        day: 'numeric',
      })
    : t('common.unknown')
})

/** 只读信息行（带参翻译在 setup 内生成） */
const createdLabel = computed(() => t('playlist.createdAtFull', { date: createdText.value }))
const trackCountLabel = computed(() => t('playlist.trackCountFull', { count: meta.value?.trackCount ?? 0 }))

const nameDraft = ref('')
const descDraft = ref('')
const saving = ref(false)
const nameInput = ref<InstanceType<typeof BaseInput> | null>(null)

function resetDrafts() {
  nameDraft.value = meta.value?.name ?? ''
  descDraft.value = metaDesc.value
}

onMounted(() => {
  resetDrafts()
  void Promise.resolve().then(() => nameInput.value?.select())
})

// 歌单列表可能异步刷新（如侧栏改名），保持草稿与最新元数据同步；
// 保存成功期间内部通过 syncing 跳过回填，避免覆盖用户输入。
let syncing = false
watch(
  () => library.playlists,
  () => {
    if (!syncing) resetDrafts()
  },
)

async function save() {
  if (saving.value) return
  const name = nameDraft.value.trim()
  if (!name) {
    toast(t('toast.playlistNameRequired'), 'error')
    return
  }
  const nameChanged = name !== (meta.value?.name ?? '')
  const descChanged = descDraft.value.trim() !== metaDesc.value
  if (!nameChanged && !descChanged) {
    emit('close')
    return
  }
  saving.value = true
  try {
    syncing = true
    if (nameChanged) await library.renamePlaylist(props.playlistId, name)
    if (descChanged) await library.setPlaylistDescription(props.playlistId, descDraft.value.trim())
    syncing = false
    toast(t('toast.playlistSaved'))
    emit('saved', nameChanged ? name : undefined)
    emit('close')
  } catch (e) {
    syncing = false
    toast(errorText(e), 'error')
  } finally {
    saving.value = false
  }
}

async function remove() {
  const ok = await confirmDialog({
    title: t('playlist.deleteTitle'),
    message: t('playlist.deleteMessage', { name: meta.value?.name ?? t('playlist.title') }),
    danger: true,
    confirmText: t('common.delete'),
  })
  if (!ok) return
  try {
    await library.deletePlaylist(props.playlistId)
    emit('deleted')
    emit('close')
  } catch (e) {
    toast(errorText(e), 'error')
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') emit('close')
}
onMounted(() => window.addEventListener('keydown', onKeydown))
onBeforeUnmount(() => window.removeEventListener('keydown', onKeydown))
</script>

<template>
  <div :class="[dialogOverlayClass('z-50'), 'p-6']" @click.self="emit('close')">
    <Transition v-bind="dialogPanelTransition" appear>
      <div
        data-dialog-panel
        class="flex w-full max-w-md flex-col overflow-hidden rounded-xl border border-zinc-200 bg-white shadow-2xl dark:border-zinc-700 dark:bg-zinc-900"
      >
        <!-- 标题栏（可拖动） -->
        <div
          v-drag-dialog
          class="flex shrink-0 items-center justify-between border-b border-zinc-200 px-5 py-4 dark:border-zinc-800"
          :class="dialogDraggable() ? 'cursor-move select-none' : ''"
        >
          <h2 class="text-base font-semibold text-zinc-900 dark:text-zinc-50">{{ $t('playlist.edit') }}</h2>
          <BaseButton variant="ghost" size="xs" :icon="X" data-no-drag v-tooltip="$t('common.close')" :aria-label="$t('common.close')" @click="emit('close')" />
        </div>

      <!-- 表单 -->
      <div class="min-h-0 flex-1 space-y-4 overflow-y-auto px-5 py-4">
        <!-- 封面 + 元信息 -->
        <div class="flex items-center gap-4">
          <CoverImg :album-id="meta?.coverAlbumId ?? null" rounded="h-16 w-16 shrink-0 rounded-lg" />
          <div class="min-w-0 text-xs text-zinc-500 dark:text-zinc-400">
            <p>{{ createdLabel }}</p>
            <p class="mt-0.5">{{ trackCountLabel }}</p>
          </div>
        </div>

        <div>
          <label class="mb-1 block text-xs font-medium text-zinc-500 dark:text-zinc-400">{{ $t('playlist.namePlaceholder') }}</label>
          <BaseInput
            ref="nameInput"
            v-model="nameDraft"
            :placeholder="$t('playlist.namePlaceholder')"
            :maxlength="60"
            @keydown.enter="save"
          />
        </div>

        <div>
          <label class="mb-1 block text-xs font-medium text-zinc-500 dark:text-zinc-400">{{ $t('common.description') }}</label>
          <BaseTextarea
            v-model="descDraft"
            :rows="3"
            :placeholder="$t('playlist.descPlaceholderLong')"
            :maxlength="300"
          />
        </div>

        <div class="border-t border-zinc-100 pt-3 dark:border-zinc-800">
          <BaseButton variant="ghost" tone="danger" size="xs" :icon="Trash2" @click="remove">
            {{ $t('playlist.delete') }}
          </BaseButton>
        </div>
      </div>

      <!-- 底部操作 -->
      <div class="flex shrink-0 items-center justify-end gap-2 border-t border-zinc-200 px-5 py-3 dark:border-zinc-800">
        <BaseButton variant="ghost" size="sm" @click="emit('close')">{{ $t('common.cancel') }}</BaseButton>
        <BaseButton variant="primary" size="sm" :loading="saving" :disabled="!nameDraft.trim()" @click="save">
          {{ $t('common.save') }}
        </BaseButton>
      </div>
    </div>
    </Transition>
  </div>
</template>