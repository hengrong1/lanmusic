<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { getVersion } from '@tauri-apps/api/app'
import {
  Check,
  FolderOpen,
  Globe,
  HardDrive,
  LoaderCircle,
  RefreshCw,
  Trash2,
} from '@lucide/vue'
import { useLibraryStore } from '@/stores/library'
import { useTheme, type ThemeMode } from '@/composables/useTheme'
import { toast } from '@/composables/useToast'
import { confirmDialog } from '@/composables/useConfirm'
import { useStagger } from '@/composables/useStagger'
import type { Source } from '@/types'

const library = useLibraryStore()
const { mode, setTheme } = useTheme()
const root = ref<HTMLElement | null>(null)
useStagger(root, ref(true))

const appVersion = ref('')
onMounted(async () => {
  try {
    appVersion.value = await getVersion()
  } catch {
    appVersion.value = '0.1.0'
  }
})

const adding = ref(false)
async function addFolder() {
  const path = await openDialog({ directory: true, multiple: false })
  if (!path || adding.value) return
  adding.value = true
  try {
    await library.addFolder(path as string)
  } catch (e) {
    toast(String(e), 'error')
  } finally {
    adding.value = false
  }
}

// ---- WebDAV 来源 ----
const showWebdav = ref(false)
const webdav = ref({ url: '', username: '', password: '', name: '' })
const webdavBusy = ref(false)
async function submitWebdav() {
  if (webdavBusy.value) return
  webdavBusy.value = true
  try {
    await library.addWebDav(webdav.value.url.trim(), webdav.value.username, webdav.value.password, webdav.value.name.trim() || undefined)
    showWebdav.value = false
    webdav.value = { url: '', username: '', password: '', name: '' }
  } catch (e) {
    toast(String(e), 'error')
  } finally {
    webdavBusy.value = false
  }
}

async function remove(s: { id: number; name: string; trackCount: number }) {
  const ok = await confirmDialog({
    title: '移除音乐来源',
    message: `确定要移除「${s.name}」吗？\n其 ${s.trackCount} 首曲目将从音乐库中删除。`,
    danger: true,
    confirmText: '移除',
  })
  if (!ok) return
  try {
    await library.removeSource(s.id)
  } catch (e) {
    toast(String(e), 'error')
  }
}

function rescan(id: number) {
  library.rescan(id).catch((e) => toast(String(e), 'error'))
}

function rescanFull(s: Source) {
  library.rescan(s.id, 'full').catch((e) => toast(String(e), 'error'))
}

async function toggleFastImport(s: Source) {
  try {
    await library.setFastImport(s.id, !s.fastImport)
    if (!s.fastImport) {
      toast('已开启快速导入：重新扫描后生效，仅按文件名/目录结构入库，不读文件内容')
    } else {
      toast('已关闭快速导入：下次增量扫描会自动补全解析这些歌曲的标签')
    }
  } catch (e) {
    toast(String(e), 'error')
  }
}

function fmtTime(t: number | null) {
  if (!t) return '从未扫描'
  return new Date(t * 1000).toLocaleString()
}

const themes: { value: ThemeMode; label: string }[] = [
  { value: 'dark', label: '深色' },
  { value: 'light', label: '浅色' },
  { value: 'system', label: '跟随系统' },
]

const scannedSourceIds = computed(() => new Set(Object.keys(library.scanProgress).map(Number)))
</script>

<template>
  <div ref="root" class="h-full overflow-y-auto px-6 pt-5 pb-8">
    <div class="mb-6">
      <p data-stagger class="text-xs font-semibold tracking-wider text-violet-500 uppercase">设置</p>
      <h1 data-stagger class="mt-0.5 text-2xl font-bold text-zinc-900 dark:text-zinc-50">偏好</h1>
    </div>

    <div class="max-w-2xl space-y-8">
      <!-- 音乐来源 -->
      <section data-stagger>
        <div class="mb-3 flex items-center justify-between">
          <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">音乐来源</h2>
          <button
            class="flex items-center gap-1.5 rounded-full bg-violet-500 px-3.5 py-1.5 text-xs font-medium text-white transition hover:bg-violet-400"
            :disabled="adding"
            @click="addFolder"
          >
            <LoaderCircle v-if="adding" class="h-3.5 w-3.5 animate-spin" />
            <FolderOpen v-else class="h-3.5 w-3.5" />
            添加文件夹
          </button>
        </div>

        <div class="space-y-2">
          <div
            v-for="s in library.sources"
            :key="s.id"
            class="rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900"
          >
            <div class="flex items-center gap-3">
              <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-zinc-100 text-zinc-500 dark:bg-zinc-800 dark:text-zinc-400">
                <component :is="s.kind === 'webdav' ? Globe : HardDrive" class="h-4.5 w-4.5" />
              </div>
              <div class="min-w-0 flex-1">
                <p class="truncate text-sm font-medium text-zinc-800 dark:text-zinc-100">{{ s.name }}</p>
                <p class="truncate text-xs text-zinc-500" :title="s.basePath ?? s.baseUrl ?? ''">{{ s.basePath ?? s.baseUrl }}</p>
              </div>
              <span class="shrink-0 text-xs text-zinc-400">{{ s.trackCount }} 首 · {{ fmtTime(s.lastScanAt) }}</span>
              <label
                class="flex shrink-0 cursor-pointer items-center gap-1.5 text-xs text-zinc-500"
                title="快速导入：不读取文件内容，按文件名/目录结构入库，适合慢速网络目录（NAS/SMB 挂载）"
              >
                快速导入
                <button
                  class="relative h-5 w-9 rounded-full transition"
                  :class="s.fastImport ? 'bg-violet-500' : 'bg-zinc-200 dark:bg-zinc-700'"
                  @click.prevent="toggleFastImport(s)"
                >
                  <span
                    class="absolute top-0.5 h-4 w-4 rounded-full bg-white shadow transition-all"
                    :class="s.fastImport ? 'left-[18px]' : 'left-0.5'"
                  ></span>
                </button>
              </label>
              <div class="flex shrink-0 items-center gap-1">
                <button
                  class="flex h-8 items-center justify-center rounded-full px-2 text-xs text-zinc-500 hover:bg-zinc-100 hover:text-violet-600 disabled:opacity-40 dark:hover:bg-zinc-800"
                  title="全部重新解析标签（含快速导入与解析失败的歌曲）"
                  :disabled="scannedSourceIds.has(s.id)"
                  @click="rescanFull(s)"
                >
                  完整解析
                </button>
                <button
                  class="flex h-8 w-8 items-center justify-center rounded-full text-zinc-500 hover:bg-zinc-100 hover:text-violet-600 disabled:opacity-40 dark:hover:bg-zinc-800"
                  title="增量扫描"
                  :disabled="scannedSourceIds.has(s.id)"
                  @click="rescan(s.id)"
                >
                  <RefreshCw class="h-4 w-4" :class="scannedSourceIds.has(s.id) ? 'animate-spin' : ''" />
                </button>
                <button
                  class="flex h-8 w-8 items-center justify-center rounded-full text-zinc-500 hover:bg-zinc-100 hover:text-red-500 dark:hover:bg-zinc-800"
                  title="移除"
                  @click="remove(s)"
                >
                  <Trash2 class="h-4 w-4" />
                </button>
              </div>
            </div>
            <div v-if="library.scanProgress[s.id]" class="mt-3">
              <template v-if="library.scanProgress[s.id].phase === 'enumerate'">
                <div class="mb-1 flex justify-between text-xs text-zinc-500">
                  <span>正在枚举目录…</span>
                  <span class="tabular-nums">{{ library.scanProgress[s.id].done }} 个文件</span>
                </div>
                <div class="h-1.5 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
                  <div class="h-full w-1/3 animate-pulse rounded-full bg-violet-500"></div>
                </div>
              </template>
              <template v-else>
                <div class="mb-1 flex justify-between text-xs text-zinc-500">
                  <span>正在解析入库…</span>
                  <span class="tabular-nums">{{ library.scanProgress[s.id].done }} / {{ library.scanProgress[s.id].total }}</span>
                </div>
                <div class="h-1.5 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
                  <div
                    class="h-full rounded-full bg-violet-500 transition-all"
                    :style="{
                      width:
                        library.scanProgress[s.id].total > 0
                          ? `${(library.scanProgress[s.id].done / library.scanProgress[s.id].total) * 100}%`
                          : '0%',
                    }"
                  ></div>
                </div>
              </template>
            </div>
          </div>

          <p v-if="!library.sources.length" class="rounded-xl border border-dashed border-zinc-300 p-4 text-center text-sm text-zinc-400 dark:border-zinc-700">
            还没有音乐来源，点击上方按钮添加文件夹
          </p>
        </div>
      </section>

      <!-- WebDAV 音乐源 -->
      <section data-stagger>
        <div class="mb-3 flex items-center justify-between">
          <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">WebDAV 音乐源</h2>
          <button
            class="flex items-center gap-1.5 rounded-full bg-violet-500 px-3.5 py-1.5 text-xs font-medium text-white transition hover:bg-violet-400"
            @click="showWebdav = !showWebdav"
          >
            <Globe class="h-3.5 w-3.5" />
            {{ showWebdav ? '收起' : '添加 WebDAV' }}
          </button>
        </div>
        <form
          v-if="showWebdav"
          class="mb-4 space-y-3 rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900"
          @submit.prevent="submitWebdav"
        >
          <p class="flex items-center gap-1.5 text-xs text-zinc-400">
            <Globe class="h-3.5 w-3.5" /> 支持 https://nas.local:5006 或 http://192.168.1.2:5005
          </p>
          <div class="grid gap-3" style="grid-template-columns: 2fr 1fr 1fr">
            <input
              v-model="webdav.url"
              class="h-9 rounded-lg border border-zinc-200 bg-transparent px-3 text-sm outline-none focus:border-violet-400 dark:border-zinc-700"
              placeholder="WebDAV 地址"
              required
            />
            <input
              v-model="webdav.username"
              class="h-9 rounded-lg border border-zinc-200 bg-transparent px-3 text-sm outline-none focus:border-violet-400 dark:border-zinc-700"
              placeholder="账号"
              autocomplete="off"
            />
            <input
              v-model="webdav.password"
              type="password"
              class="h-9 rounded-lg border border-zinc-200 bg-transparent px-3 text-sm outline-none focus:border-violet-400 dark:border-zinc-700"
              placeholder="密码"
              autocomplete="off"
            />
          </div>
          <div class="flex items-center gap-3">
            <input
              v-model="webdav.name"
              class="h-9 w-56 rounded-lg border border-zinc-200 bg-transparent px-3 text-sm outline-none focus:border-violet-400 dark:border-zinc-700"
              placeholder="备注名（可选）"
            />
            <button
              class="flex items-center gap-2 rounded-full bg-violet-500 px-4 py-1.5 text-sm font-medium text-white hover:bg-violet-400"
              :disabled="webdavBusy"
            >
              <LoaderCircle v-if="webdavBusy" class="h-4 w-4 animate-spin" />
              <Check v-else class="h-4 w-4" />
              添加并扫描
            </button>
          </div>
        </form>
      </section>

      <!-- 外观 -->
      <section data-stagger>
        <h2 class="mb-3 text-sm font-semibold text-zinc-800 dark:text-zinc-100">外观</h2>
        <div class="flex gap-2">
          <button
            v-for="t in themes"
            :key="t.value"
            class="rounded-full px-4 py-1.5 text-sm transition"
            :class="
              mode === t.value
                ? 'bg-violet-500 font-medium text-white'
                : 'bg-zinc-100 text-zinc-600 hover:bg-zinc-200 dark:bg-zinc-800 dark:text-zinc-300 dark:hover:bg-zinc-700'
            "
            @click="setTheme(t.value)"
          >
            {{ t.label }}
          </button>
        </div>
      </section>

      <!-- 关于 -->
      <section data-stagger>
        <h2 class="mb-3 text-sm font-semibold text-zinc-800 dark:text-zinc-100">关于</h2>
        <div class="rounded-xl border border-zinc-200 bg-white p-4 text-sm text-zinc-500 dark:border-zinc-800 dark:bg-zinc-900">
          LanMusic <span class="tabular-nums">{{ appVersion }}</span> · Tauri 2 + Vue 3 · 纯本地，不上传任何数据
        </div>
      </section>
    </div>
  </div>
</template>
