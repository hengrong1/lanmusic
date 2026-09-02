<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { RefreshIcon as LoaderCircle } from '@solar-icons/vue/linear/refresh'
import { TrashBin2Icon as Trash2 } from '@solar-icons/vue/linear/trash-bin-2'
import { CloseIcon as X } from '@solar-icons/vue/linear/close'
import CoverImg from '@/components/CoverImg.vue'
import { useLibraryStore } from '@/stores/library'
import { confirmDialog } from '@/composables/useConfirm'
import { toast } from '@/composables/useToast'

/**
 * 编辑歌单弹层：集中修改名称、简介；只读展示创建时间 / 歌曲数 / 封面；删除歌单。
 * 保存时只提交有变化的字段；删除成功后 emit('deleted') 由上层负责跳转。
 */
const props = defineProps<{ playlistId: number }>()
const emit = defineEmits<{ close: []; saved: [name?: string]; deleted: [] }>()

const library = useLibraryStore()

const meta = computed(() => library.playlists.find((p) => p.id === props.playlistId) ?? null)
const metaDesc = computed(() => meta.value?.description ?? '')
const createdText = computed(() => {
  const t = meta.value?.createdAt
  return t
    ? new Date(t * 1000).toLocaleDateString('zh-CN', { year: 'numeric', month: 'long', day: 'numeric' })
    : '未知'
})

const nameDraft = ref('')
const descDraft = ref('')
const saving = ref(false)
const nameInput = ref<HTMLInputElement | null>(null)

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
    toast('歌单名不能为空', 'error')
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
    toast('歌单已保存')
    emit('saved', nameChanged ? name : undefined)
    emit('close')
  } catch (e) {
    syncing = false
    toast(String(e), 'error')
  } finally {
    saving.value = false
  }
}

async function remove() {
  const ok = await confirmDialog({
    title: '删除歌单',
    message: `确定删除歌单「${meta.value?.name ?? '该歌单'}」吗？歌曲本身不会被删除。`,
    danger: true,
    confirmText: '删除',
  })
  if (!ok) return
  try {
    await library.deletePlaylist(props.playlistId)
    emit('deleted')
    emit('close')
  } catch (e) {
    toast(String(e), 'error')
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') emit('close')
}
onMounted(() => window.addEventListener('keydown', onKeydown))
onBeforeUnmount(() => window.removeEventListener('keydown', onKeydown))
</script>

<template>
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6" @click.self="emit('close')">
    <div
      class="flex w-full max-w-md flex-col overflow-hidden rounded-xl border border-zinc-200 bg-white shadow-2xl dark:border-zinc-700 dark:bg-zinc-900"
    >
      <!-- 标题栏 -->
      <div class="flex shrink-0 items-center justify-between border-b border-zinc-200 px-5 py-4 dark:border-zinc-800">
        <h2 class="text-base font-semibold text-zinc-900 dark:text-zinc-50">编辑歌单</h2>
        <button
          class="flex h-7 w-7 cursor-pointer items-center justify-center rounded-full text-zinc-400 transition hover:bg-zinc-100 dark:hover:bg-zinc-800"
          title="关闭"
          @click="emit('close')"
        >
          <X class="h-4 w-4" />
        </button>
      </div>

      <!-- 表单 -->
      <div class="min-h-0 flex-1 space-y-4 overflow-y-auto px-5 py-4">
        <!-- 封面 + 元信息 -->
        <div class="flex items-center gap-4">
          <CoverImg :album-id="meta?.coverAlbumId ?? null" rounded="h-16 w-16 shrink-0 rounded-lg" />
          <div class="min-w-0 text-xs text-zinc-500 dark:text-zinc-400">
            <p>创建时间：{{ createdText }}</p>
            <p class="mt-0.5">歌曲：{{ meta?.trackCount ?? 0 }} 首</p>
          </div>
        </div>

        <div>
          <label class="mb-1 block text-xs font-medium text-zinc-500 dark:text-zinc-400">歌单名称</label>
          <input
            ref="nameInput"
            v-model="nameDraft"
            class="h-9 w-full rounded-lg border border-zinc-200 bg-transparent px-3 text-sm text-zinc-800 outline-none focus:border-violet-400 dark:border-zinc-700 dark:text-zinc-100"
            placeholder="歌单名称"
            maxlength="60"
            @keydown.enter="save"
          />
        </div>

        <div>
          <label class="mb-1 block text-xs font-medium text-zinc-500 dark:text-zinc-400">简介</label>
          <textarea
            v-model="descDraft"
            rows="3"
            class="w-full resize-none rounded-lg border border-zinc-200 bg-transparent px-3 py-2 text-sm text-zinc-800 outline-none focus:border-violet-400 dark:border-zinc-700 dark:text-zinc-100"
            placeholder="写点什么，介绍这个歌单…"
            maxlength="300"
          ></textarea>
        </div>

        <div class="border-t border-zinc-100 pt-3 dark:border-zinc-800">
          <button
            class="flex cursor-pointer items-center gap-1.5 text-xs font-medium text-red-500 transition hover:text-red-600"
            @click="remove"
          >
            <Trash2 class="h-3.5 w-3.5" />
            删除歌单
          </button>
        </div>
      </div>

      <!-- 底部操作 -->
      <div class="flex shrink-0 items-center justify-end gap-2 border-t border-zinc-200 px-5 py-3 dark:border-zinc-800">
        <button
          class="cursor-pointer rounded-full px-3 py-1.5 text-sm text-zinc-500 transition hover:bg-zinc-100 dark:hover:bg-zinc-800"
          @click="emit('close')"
        >
          取消
        </button>
        <button
          class="flex cursor-pointer items-center gap-1.5 rounded-full bg-violet-500 px-4 py-1.5 text-sm font-medium text-white transition hover:bg-violet-400 disabled:cursor-not-allowed disabled:opacity-40"
          :disabled="saving || !nameDraft.trim()"
          @click="save"
        >
          <LoaderCircle v-if="saving" class="h-4 w-4 animate-spin" />
          保存
        </button>
      </div>
    </div>
  </div>
</template>