<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { getVersion } from '@tauri-apps/api/app'
import { VerifiedCheckIcon as Check } from '@solar-icons/vue/linear/verified-check'
import { FolderOpenIcon as FolderOpen } from '@solar-icons/vue/linear/folder-open'
import { GlobeIcon as Globe } from '@solar-icons/vue/linear/globe'
import { DatabaseIcon as HardDrive } from '@solar-icons/vue/linear/database'
import { RefreshIcon as LoaderCircle } from '@solar-icons/vue/linear/refresh'
import { RefreshIcon as RefreshCw } from '@solar-icons/vue/linear/refresh'
import { TrashBin2Icon as Trash2 } from '@solar-icons/vue/linear/trash-bin-2'
import { useLibraryStore } from '@/stores/library'
import { useTheme, type ThemeMode } from '@/composables/useTheme'
import { toast } from '@/composables/useToast'
import { confirmDialog } from '@/composables/useConfirm'
import { useStagger } from '@/composables/useStagger'
import { useDesktopLyrics } from '@/composables/useDesktopLyrics'
import { getAppFont, setAppFont } from '@/composables/useAppFont'
import { getPreventSleep, setPreventSleepSetting } from '@/composables/usePowerGuard'
import { useUpdater } from '@/composables/useUpdater'
import { usePlayerStore } from '@/stores/player'
import { api } from '@/api/commands'
import { getSearchSettings, setSearchSettings, type SearchSettings } from '@/composables/useSearchSettings'
import type { ArtistSplitChange, Source } from '@/types'
import { setLocale } from '@/i18n'
import { useI18n } from 'vue-i18n'
import {
  BaseButton,
  BaseButtonGroup,
  BaseCheckbox,
  BaseColorPicker,
  BaseInput,
  BaseSwitch,
  BaseSelect,
  BaseSlider,
} from '@/components/ui'
import type { ButtonGroupItem, SelectOption } from '@/components/ui'

const library = useLibraryStore()
const { mode, setTheme } = useTheme()
const { enabled: dlEnabled, toggle: dlToggle, config: dlConfig } = useDesktopLyrics()
const player = usePlayerStore()
const { locale } = useI18n()

const languageOptions = [
  { value: 'zh' as const, label: '简体中文' },
  { value: 'en' as const, label: 'English' },
]
function onLocaleChange(val: string | number) {
  setLocale(val as 'zh' | 'en')
}

const themeItems: ButtonGroupItem[] = [
  { value: 'light', label: '浅色' },
  { value: 'dark', label: '深色' },
  { value: 'system', label: '跟随系统' },
]
function onThemeChange(val: string) {
  setTheme(val as ThemeMode)
}

// ---- 播放设置（淡入淡出 / 阻止系统休眠）----
const fadeOn = ref(player.isFadeOn())
function onFadeToggle(val: boolean) {
  fadeOn.value = val
  player.setFadeEnabled(val)
  toast(val ? '已开启歌曲淡入淡出' : '已关闭歌曲淡入淡出')
}
const preventSleepOn = ref(getPreventSleep())
function onPreventSleepToggle(val: boolean) {
  preventSleepOn.value = val
  setPreventSleepSetting(val, player.playing)
  toast(val ? '已开启播放时阻止系统休眠' : '已关闭播放时阻止系统休眠')
}

// ---- 系统托盘设置 ----
type CloseAction = 'tray' | 'quit'
function getCloseAction(): CloseAction {
  const v = localStorage.getItem('lm.closeAction')
  return v === 'quit' ? 'quit' : 'tray'
}
function setCloseAction(action: string | number) {
  const v = action as CloseAction
  localStorage.setItem('lm.closeAction', v)
  // 同步到 SQLite，供 Rust 侧关闭事件使用
  api.setSetting('lm.closeAction', v).catch(() => {})
  toast(v === 'tray' ? '关闭窗口时将最小化到托盘' : '关闭窗口时将退出应用')
}
const closeAction = ref(getCloseAction())
// 从 SQLite 加载设置（如果存在）
onMounted(() => {
  api.getSetting('lm.closeAction')
    .then((v) => {
      if (v === 'tray' || v === 'quit') {
        closeAction.value = v
        localStorage.setItem('lm.closeAction', v)
      }
    })
    .catch(() => {})
})

// ---- 搜索设置 ----
const searchSettings = ref<SearchSettings>(getSearchSettings())
const searchFieldOptions = [
  { value: 'title', label: '标题' },
  { value: 'artist', label: '艺术家' },
  { value: 'album', label: '专辑' },
  { value: 'lyrics', label: '歌词' },
  { value: 'filename', label: '文件名' },
]
const searchSortOptions: { value: SearchSettings['sort']; label: string }[] = [
  { value: 'relevance', label: '相关度优先' },
  { value: 'added', label: '时间添加优先' },
  { value: 'plays', label: '播放次数优先' },
]
const searchSortItems: ButtonGroupItem[] = searchSortOptions.map((o) => ({ value: o.value, label: o.label }))
function onSearchSortChange(val: string) {
  searchSettings.value = { ...searchSettings.value, sort: val as SearchSettings['sort'] }
  saveSearchSettings()
}
/** 保存到 localStorage（每次变更即时生效，无 toast 打扰） */
function saveSearchSettings() {
  setSearchSettings(searchSettings.value)
}
/** 勾选/取消某个搜索字段；至少保留一项，其余变更即时保存 */
function toggleSearchField(v: string, checked: boolean) {
  const fields = searchSettings.value.fields
  if (checked && !fields.includes(v)) {
    searchSettings.value = { ...searchSettings.value, fields: [...fields, v] }
  } else if (!checked && fields.includes(v)) {
    if (fields.length === 1) return
    searchSettings.value = { ...searchSettings.value, fields: fields.filter((x) => x !== v) }
  } else {
    return
  }
  saveSearchSettings()
}
function togglePinyin(val: boolean) {
  searchSettings.value = { ...searchSettings.value, pinyin: val }
  saveSearchSettings()
}
function onDebounceChange(val: string | number) {
  searchSettings.value = { ...searchSettings.value, debounceMs: Number(val) }
  saveSearchSettings()
}

// ---- 曲库：多艺人分隔符 ----
/** 可选分隔符候选集（与 Rust 侧 SEPARATOR_CANDIDATES 对应；顺序即展示顺序） */
const SEPARATOR_CANDIDATES = [';', '；', '、', '&', '，', ',', '/'] as const
/** 恒定开启的分隔符：feat. / ft. / featuring 归一为 ';' 后按它拆分 */
const FIXED_SEPARATOR = ';'
const artistSeparators = ref<Set<string>>(new Set(SEPARATOR_CANDIDATES))
/** 最近一次调整分隔符后的艺人变更列表（null = 尚未调整过） */
const splitChanges = ref<ArtistSplitChange[] | null>(null)
const splitApplying = ref(false)

onMounted(() => {
  api.getArtistSeparators()
    .then((v) => (artistSeparators.value = new Set(v.split(''))))
    .catch(() => {})
})

async function onSeparatorToggle(sep: string) {
  if (sep === FIXED_SEPARATOR || splitApplying.value) return
  const next = new Set(artistSeparators.value)
  if (next.has(sep)) next.delete(sep)
  else next.add(sep)
  artistSeparators.value = next
  splitApplying.value = true
  try {
    const value = SEPARATOR_CANDIDATES.filter((c) => next.has(c)).join('')
    const changes = await api.setArtistSeparators(value)
    splitChanges.value = changes
    if (changes.length > 0) {
      toast(`已按新分隔符重新拆分 ${changes.length} 首歌曲的艺人`)
      // 艺人归属变了，刷新曲库统计与当前列表
      await Promise.all([library.loadStats(), library.loadTracks()])
    } else {
      toast('分隔符已更新，曲库中没有歌曲的艺人受影响')
    }
  } catch (e) {
    toast(String(e), 'error')
    // 失败时回读实际生效的设置，保持 UI 与后端一致
    api.getArtistSeparators()
      .then((v) => (artistSeparators.value = new Set(v.split(''))))
      .catch(() => {})
  } finally {
    splitApplying.value = false
  }
}

/** 桌面歌词：行数 / 对齐（分段控件，值为字符串，写入时转回） */
const dlLineItems: ButtonGroupItem[] = [
  { value: '1', label: '单行' },
  { value: '2', label: '双行' },
]
const dlAlignItems: ButtonGroupItem[] = [
  { value: 'left', label: '左对齐' },
  { value: 'center', label: '居中' },
  { value: 'right', label: '右对齐' },
  { value: 'split', label: '左右分离' },
]
function onDlLinesChange(val: string) {
  dlConfig.value.lines = Number(val) === 1 ? 1 : 2
  if (dlConfig.value.lines === 1 && dlConfig.value.align === 'split') dlConfig.value.align = 'center'
}
function onDlAlignChange(val: string) {
  dlConfig.value.align = val as typeof dlConfig.value.align
}
const filteredDlAlignItems = computed(() => dlAlignItems.filter((o) => o.value !== 'split' || dlConfig.value.lines === 2))

// ---- 全局字体（设置 → 外观）----
const appFont = ref(getAppFont())
/** 系统已安装字体（Rust 侧读注册表；非 Windows / 读取失败为空，只显示默认项） */
const systemFonts = ref<string[]>([])
onMounted(() => {
  api
    .listSystemFonts()
    .then((f) => (systemFonts.value = f))
    .catch(() => (systemFonts.value = []))
})
function onFontChange(val: string | number) {
  appFont.value = val as string
  setAppFont(appFont.value)
}

// 组件选项数据
const closeActionItems: ButtonGroupItem[] = [
  { value: 'tray', label: '最小化到托盘' },
  { value: 'quit', label: '退出应用' },
]

const fontSelectOptions = computed<SelectOption[]>(() => [
  { value: '', label: '系统默认' },
  ...systemFonts.value.map((f) => ({ value: `'${f}'`, label: f })),
])

const debounceSelectOptions: SelectOption[] = [
  { value: 150, label: '150ms' },
  { value: 300, label: '300ms' },
  { value: 500, label: '500ms' },
  { value: 800, label: '800ms' },
]

/** 桌面歌词预览：与浮窗完全一致的样式计算 */
function hexToRgba(hex: string, alpha: number): string {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim())
  if (!m) return `rgba(0, 0, 0, ${alpha})`
  const n = parseInt(m[1], 16)
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${alpha})`
}
const previewBoxStyle = computed(() => ({
  background:
    dlConfig.value.bgOpacity > 0 ? hexToRgba(dlConfig.value.bgColor, dlConfig.value.bgOpacity) : 'transparent',
  boxShadow: dlConfig.value.bgOpacity > 0 ? 'none' : 'inset 0 0 0 1px rgba(0,0,0,0.08)',
}))
const previewShadow = computed(() => {
  if (!dlConfig.value.outline) return 'none'
  return `0 0 2px ${dlConfig.value.outlineColor}, 0 1px 2px ${dlConfig.value.outlineColor}`
})
const previewMainStyle = computed(() => ({
  color: dlConfig.value.color,
  fontSize: `${Math.min(dlConfig.value.fontSize, 28)}px`,
  fontWeight: dlConfig.value.bold ? 700 : 500,
  textShadow: previewShadow.value,
  textAlign: dlConfig.value.align === 'split' ? 'left' : dlConfig.value.align,
}))
const previewPendingStyle = computed(() => ({
  color: dlConfig.value.pendingColor,
  fontSize: `${Math.min(dlConfig.value.fontSize, 28)}px`,
  fontWeight: dlConfig.value.bold ? 700 : 500,
  textShadow: previewShadow.value,
  textAlign: dlConfig.value.align === 'split' ? 'right' : dlConfig.value.align,
}))
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

// ---- 应用内更新（GitHub Releases）----
const updater = useUpdater()
const progressPct = computed(() => (updater.progress.value >= 0 ? Math.round(updater.progress.value * 100) : -1))

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

async function toggleFastImport(s: Source, val?: boolean) {
  const next = val ?? !s.fastImport
  if (next === s.fastImport) return
  try {
    await library.setFastImport(s.id, next)
    if (next) {
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
          <BaseButton
            size="sm"
            rounded
            :icon="adding ? LoaderCircle : FolderOpen"
            :loading="adding"
            :disabled="adding"
            @click="addFolder"
          >
            添加文件夹
          </BaseButton>
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
              <div
                class="flex shrink-0 items-center gap-1.5 text-xs text-zinc-500"
                title="快速导入：不读取文件内容，按文件名/目录结构入库，适合慢速网络目录（NAS/SMB 挂载）"
              >
                快速导入
                <BaseSwitch
                  :model-value="s.fastImport"
                  size="sm"
                  @update:model-value="(v) => toggleFastImport(s, v)"
                />
              </div>
              <div class="flex shrink-0 items-center gap-1">
                <BaseButton
                  variant="ghost"
                  size="xs"
                  rounded
                  :disabled="scannedSourceIds.has(s.id)"
                  title="全部重新解析标签（含快速导入与解析失败的歌曲）"
                  @click="rescanFull(s)"
                >
                  完整解析
                </BaseButton>
                <BaseButton
                  variant="ghost"
                  size="xs"
                  rounded
                  :icon="RefreshCw"
                  :loading="scannedSourceIds.has(s.id)"
                  :disabled="scannedSourceIds.has(s.id)"
                  title="增量扫描"
                  aria-label="增量扫描"
                  @click="rescan(s.id)"
                />
                <BaseButton
                  variant="ghost"
                  tone="danger"
                  size="xs"
                  rounded
                  :icon="Trash2"
                  title="移除"
                  aria-label="移除"
                  @click="remove(s)"
                />
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
          <BaseButton size="sm" rounded :icon="Globe" @click="showWebdav = !showWebdav">
            {{ showWebdav ? '收起' : '添加 WebDAV' }}
          </BaseButton>
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
            <BaseInput v-model="webdav.url" placeholder="WebDAV 地址" required />
            <BaseInput v-model="webdav.username" placeholder="账号" autocomplete="off" />
            <BaseInput v-model="webdav.password" type="password" placeholder="密码" autocomplete="off" />
          </div>
          <div class="flex items-center gap-3">
            <div class="w-56">
              <BaseInput v-model="webdav.name" placeholder="备注名（可选）" />
            </div>
            <BaseButton type="submit" rounded :loading="webdavBusy" :disabled="webdavBusy" :icon="webdavBusy ? undefined : Check">
              添加并扫描
            </BaseButton>
          </div>
        </form>
      </section>

      <!-- 外观 -->
      <section data-stagger>
        <h2 class="mb-3 text-sm font-semibold text-zinc-800 dark:text-zinc-100">外观</h2>
        <div class="space-y-3">
          <BaseButtonGroup :model-value="mode" :items="themeItems" size="sm" @update:model-value="onThemeChange" />
          <!-- 全局字体：应用于整个软件（含桌面歌词），从系统读取 -->
          <div class="flex items-center justify-between gap-3">
            <span class="text-sm text-zinc-600 dark:text-zinc-300">字体</span>
            <div class="max-w-[280px] flex-1">
              <BaseSelect :model-value="appFont" :options="fontSelectOptions" size="sm" @update:model-value="onFontChange" />
            </div>
          </div>
        </div>
      </section>

      <!-- 播放 -->
      <section data-stagger>
        <h2 class="mb-3 text-sm font-semibold text-zinc-800 dark:text-zinc-100">播放</h2>
        <div class="space-y-3 rounded-xl border border-zinc-200 bg-white p-4 text-sm dark:border-zinc-800 dark:bg-zinc-900">
          <!-- 淡入淡出 -->
          <div class="flex items-center justify-between">
            <span class="text-zinc-600 dark:text-zinc-300">歌曲淡入淡出</span>
            <BaseSwitch
              :model-value="fadeOn"
              size="sm"
              title="开启后播放/暂停与切歌时音量平滑过渡（淡入 0.8s，淡出 0.6s）"
              @update:model-value="onFadeToggle"
            />
          </div>
          <p class="text-xs text-zinc-400">播放 / 暂停与切歌 / 队列末尾时音量平滑过渡，避免突兀截断。</p>

          <!-- 阻止系统休眠 -->
          <div class="flex items-center justify-between pt-2">
            <span class="text-zinc-600 dark:text-zinc-300">播放时阻止系统休眠 / 锁屏</span>
            <BaseSwitch
              :model-value="preventSleepOn"
              size="sm"
              title="播放歌曲期间保持系统与屏幕常亮，防止自动休眠/锁屏"
              @update:model-value="onPreventSleepToggle"
            />
          </div>
          <p class="text-xs text-zinc-400">播放期间保持系统与屏幕常亮；暂停 / 停止后自动恢复（默认开启）。Windows 通过系统电源 API 实现，其他平台尝试 Web Wake Lock。</p>
        </div>
      </section>

      <!-- 曲库 -->
      <section data-stagger>
        <h2 class="mb-3 text-sm font-semibold text-zinc-800 dark:text-zinc-100">曲库</h2>
        <div class="rounded-xl border border-zinc-200 bg-white p-4 text-sm dark:border-zinc-800 dark:bg-zinc-900">
          <!-- 多艺人分隔符 -->
          <div class="flex flex-wrap items-center justify-between gap-3">
            <span class="text-zinc-600 dark:text-zinc-300">多艺人分隔符</span>
            <div class="flex flex-wrap gap-2">
              <BaseButton
                v-for="sep in SEPARATOR_CANDIDATES"
                :key="sep"
                size="sm"
                rounded
                :variant="artistSeparators.has(sep) ? 'primary' : 'secondary'"
                :title="sep === FIXED_SEPARATOR ? '固定分隔符：feat. / ft. / featuring 等合作标注按它拆分' : `启用后按「${sep}」拆分多艺人`"
                :disabled="sep === FIXED_SEPARATOR || splitApplying"
                @click="onSeparatorToggle(sep)"
              >
                {{ sep }}
              </BaseButton>
            </div>
          </div>
          <p class="mt-2 text-xs text-zinc-400">
            歌曲标签里的艺人按所选分隔符拆分为独立艺人；分号「;」为固定分隔符（feat. / ft. / featuring 也会按它拆分）。
            关闭某分隔符后，含该符号的艺人名将保持完整（如 AC/DC），保存后立即对整个曲库生效。
          </p>

          <!-- 变更报告：保存后展示受影响歌曲的艺人变化 -->
          <template v-if="splitChanges !== null">
            <div class="mt-3 border-t border-zinc-100 pt-3 dark:border-zinc-800">
              <p class="text-xs font-medium" :class="splitChanges.length > 0 ? 'text-violet-500' : 'text-zinc-400'">
                {{
                  splitChanges.length > 0
                    ? `本次更新了 ${splitChanges.length} 首歌曲的艺人拆分`
                    : '曲库中没有歌曲的艺人受此次分隔符变更影响'
                }}
              </p>
              <ul v-if="splitChanges.length > 0" class="mt-2 max-h-56 space-y-1.5 overflow-y-auto pr-1">
                <li v-for="c in splitChanges.slice(0, 200)" :key="c.trackId" class="flex min-w-0 flex-wrap items-baseline gap-x-1 text-xs">
                  <span class="font-medium text-zinc-700 dark:text-zinc-200">{{ c.title }}</span>
                  <span class="text-zinc-400">：</span>
                  <span class="text-zinc-400 line-through">{{ c.oldArtists.join(' / ') }}</span>
                  <span class="text-violet-500">→</span>
                  <span class="text-zinc-600 dark:text-zinc-300">{{ c.newArtists.join(' / ') }}</span>
                </li>
              </ul>
              <p v-if="splitChanges.length > 200" class="mt-1 text-xs text-zinc-400">
                其余 {{ splitChanges.length - 200 }} 首歌曲的艺人也已同步更新。
              </p>
            </div>
          </template>
        </div>
      </section>

      <!-- 搜索 -->
      <section data-stagger>
        <h2 class="mb-3 text-sm font-semibold text-zinc-800 dark:text-zinc-100">搜索</h2>
        <div class="space-y-4 rounded-xl border border-zinc-200 bg-white p-4 text-sm dark:border-zinc-800 dark:bg-zinc-900">
          <!-- 搜索范围 -->
          <div>
            <p class="mb-2 text-xs font-medium text-zinc-400">搜索范围</p>
            <div class="flex flex-wrap gap-x-5 gap-y-2">
              <BaseCheckbox
                v-for="opt in searchFieldOptions"
                :key="opt.value"
                :model-value="searchSettings.fields.includes(opt.value)"
                :label="opt.label"
                size="sm"
                :disabled="searchSettings.fields.length === 1 && searchSettings.fields.includes(opt.value)"
                @update:model-value="(v) => toggleSearchField(opt.value, v)"
              />
            </div>
          </div>
          <p class="-mt-2 text-xs text-zinc-400">勾选后搜索将在对应字段内匹配；至少保留一项。</p>

          <!-- 拼音搜索 -->
          <div class="flex items-center justify-between">
            <span class="text-zinc-600 dark:text-zinc-300">拼音搜索</span>
            <BaseSwitch
              :model-value="searchSettings.pinyin"
              size="sm"
              title="开启后可用拼音或首字母搜索中文歌曲"
              @update:model-value="togglePinyin"
            />
          </div>
          <p class="-mt-2 text-xs text-zinc-400">开启后可用拼音或首字母搜索中文歌曲（如输入 zhou 匹配 周杰伦）。</p>

          <!-- 排序偏好 -->
          <div class="flex items-center justify-between">
            <span class="text-zinc-600 dark:text-zinc-300">排序偏好</span>
            <BaseButtonGroup
              :model-value="searchSettings.sort"
              :items="searchSortItems"
              size="sm"
              @update:model-value="onSearchSortChange"
            />
          </div>

          <!-- 输入防抖 -->
          <div class="flex items-center justify-between">
            <span class="text-zinc-600 dark:text-zinc-300">输入防抖</span>
            <div class="w-28">
              <BaseSelect
                :model-value="searchSettings.debounceMs"
                :options="debounceSelectOptions"
                size="sm"
                @update:model-value="onDebounceChange"
              />
            </div>
          </div>
          <p class="-mt-2 text-xs text-zinc-400">打字停顿超过该时长才发起搜索，避免输入过快导致频繁查询卡顿。</p>
        </div>
      </section>

      <!-- 桌面歌词 -->
      <section data-stagger>
        <h2 class="mb-3 text-sm font-semibold text-zinc-800 dark:text-zinc-100">桌面歌词</h2>
        <div class="rounded-xl border border-zinc-200 bg-white p-4 text-sm dark:border-zinc-800 dark:bg-zinc-900">
          <!-- 预览：与浮窗样式一致，随下方设置实时变化 -->
          <div
            class="flex min-h-[76px] flex-col justify-center gap-1 rounded-xl px-5 py-3"
            :style="previewBoxStyle"
          >
            <p class="truncate font-bold" :style="previewMainStyle">这是正在播放的一句歌词预览</p>
            <p class="truncate" :style="previewPendingStyle">这是下一句歌词的预告预览</p>
          </div>

          <!-- 显示 -->
          <div class="mt-5 mb-3 flex items-center gap-2">
            <span class="text-xs font-medium text-zinc-400">显示</span>
            <span class="h-px flex-1 bg-zinc-100 dark:bg-zinc-800"></span>
          </div>
          <div class="grid grid-cols-1 gap-x-8 gap-y-4 sm:grid-cols-2">
            <!-- 开关 -->
            <div class="flex items-center justify-between">
              <span class="text-zinc-600 dark:text-zinc-300">显示桌面歌词浮窗</span>
              <BaseSwitch
                :model-value="dlEnabled"
                size="sm"
                :label="dlEnabled ? '已开启' : '已关闭'"
                @update:model-value="() => dlToggle()"
              />
            </div>
            <!-- 显示行数 -->
            <div class="flex items-center justify-between">
              <span class="text-zinc-600 dark:text-zinc-300">显示行数</span>
              <BaseButtonGroup
                :model-value="String(dlConfig.lines)"
                :items="dlLineItems"
                size="sm"
                @update:model-value="onDlLinesChange"
              />
            </div>
            <!-- 对齐方式（选项较多，独占一行） -->
            <div class="flex items-center justify-between gap-3 sm:col-span-2">
              <span class="shrink-0 text-zinc-600 dark:text-zinc-300">对齐方式</span>
              <BaseButtonGroup
                :model-value="dlConfig.align"
                :items="filteredDlAlignItems"
                size="sm"
                @update:model-value="onDlAlignChange"
              />
            </div>
          </div>
          <!-- 样式 -->
          <div class="mt-5 mb-3 flex items-center gap-2">
            <span class="text-xs font-medium text-zinc-400">样式</span>
            <span class="h-px flex-1 bg-zinc-100 dark:bg-zinc-800"></span>
          </div>
          <div class="grid grid-cols-1 gap-x-8 gap-y-4 sm:grid-cols-2">
            <!-- 播放行颜色 -->
            <div class="flex items-center justify-between">
              <span class="text-zinc-600 dark:text-zinc-300">播放行颜色</span>
              <BaseColorPicker v-model="dlConfig.color" title="播放行颜色" />
            </div>
            <!-- 未播放行颜色 -->
            <div class="flex items-center justify-between">
              <span class="text-zinc-600 dark:text-zinc-300">未播放行颜色</span>
              <BaseColorPicker v-model="dlConfig.pendingColor" title="未播放行颜色" />
            </div>
            <!-- 描边 -->
            <div class="flex items-center justify-between">
              <span class="text-zinc-600 dark:text-zinc-300">显示文字描边</span>
              <div class="flex items-center gap-2">
                <BaseColorPicker
                  v-model="dlConfig.outlineColor"
                  :disabled="!dlConfig.outline"
                  title="描边颜色"
                />
                <BaseCheckbox v-model="dlConfig.outline" size="sm" />
              </div>
            </div>
            <!-- 字体加粗 -->
            <div class="flex items-center justify-between">
              <span class="text-zinc-600 dark:text-zinc-300">字体加粗</span>
              <BaseCheckbox v-model="dlConfig.bold" size="sm" />
            </div>
            <!-- 字号（独占一行，滑杆拉满宽度） -->
            <div class="flex items-center gap-3 sm:col-span-2">
              <span class="shrink-0 text-zinc-600 dark:text-zinc-300">字号（{{ dlConfig.fontSize }}px）</span>
              <div class="w-full min-w-0 flex-1">
                <BaseSlider v-model="dlConfig.fontSize" :min="18" :max="56" :step="2" />
              </div>
            </div>
          </div>

          <!-- 背景 -->
          <div class="mt-5 mb-3 flex items-center gap-2">
            <span class="text-xs font-medium text-zinc-400">背景</span>
            <span class="h-px flex-1 bg-zinc-100 dark:bg-zinc-800"></span>
          </div>
          <div class="grid grid-cols-1 gap-x-8 gap-y-4 sm:grid-cols-2">
            <!-- 背景颜色 -->
            <div class="flex items-center justify-between">
              <span class="text-zinc-600 dark:text-zinc-300">背景颜色</span>
              <BaseColorPicker v-model="dlConfig.bgColor" title="背景颜色" />
            </div>
            <!-- 背景不透明度 -->
            <div class="flex items-center gap-3">
              <span class="shrink-0 text-zinc-600 dark:text-zinc-300">不透明度（{{ Math.round(dlConfig.bgOpacity * 100) }}%）</span>
              <div class="w-full min-w-0 flex-1">
                <BaseSlider v-model="dlConfig.bgOpacity" :min="0" :max="0.85" :step="0.05" />
              </div>
            </div>
          </div>
          <p class="mt-4 text-xs text-zinc-400">
            桌面歌词背景默认隐藏，鼠标悬停到浮窗上才会显示；按住歌词文字区域可拖动位置，悬停时还可使用控制条（切歌 / 歌词校准 / 关闭）。
          </p>
        </div>
      </section>

      <!-- 系统托盘 -->
      <section data-stagger>
        <h2 class="mb-3 text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ $t('settings.tray') }}</h2>
        <div class="rounded-xl border border-zinc-200 bg-white p-4 text-sm dark:border-zinc-800 dark:bg-zinc-900">
          <div class="flex flex-wrap items-center justify-between gap-3">
            <span class="text-zinc-600 dark:text-zinc-300">{{ $t('settings.closeAction') }}</span>
            <BaseButtonGroup
              v-model="closeAction"
              :items="closeActionItems"
              size="sm"
              @update:model-value="setCloseAction"
            />
          </div>
          <p class="mt-2 text-xs text-zinc-400">{{ $t('settings.closeActionDesc') }}</p>
        </div>
      </section>

      <!-- 语言 -->
      <section data-stagger>
        <h2 class="mb-3 text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ $t('settings.language') }}</h2>
        <div class="rounded-xl border border-zinc-200 bg-white p-4 text-sm text-zinc-500 dark:border-zinc-800 dark:bg-zinc-900">
          <div class="flex flex-wrap items-center justify-between gap-3">
            <span>{{ $t('settings.languageSelect') }}</span>
            <BaseSelect :model-value="locale" :options="languageOptions" size="sm" @update:model-value="onLocaleChange" />
          </div>
        </div>
      </section>

      <!-- 关于 -->
      <section data-stagger>
        <h2 class="mb-3 text-sm font-semibold text-zinc-800 dark:text-zinc-100">关于</h2>
        <div class="rounded-xl border border-zinc-200 bg-white p-4 text-sm text-zinc-500 dark:border-zinc-800 dark:bg-zinc-900">
          <div class="flex flex-wrap items-center justify-between gap-3">
            <span>LanMusic <span class="tabular-nums">{{ appVersion }}</span> · Tauri 2 + Vue 3 · 纯本地，不上传任何数据</span>
            <!-- 更新操作区：按状态切换 -->
            <div class="flex items-center gap-2">
              <template v-if="updater.status.value === 'available' || updater.status.value === 'downloading'">
                <BaseButton
                  size="sm"
                  rounded
                  :loading="updater.status.value === 'downloading'"
                  :icon="updater.status.value === 'downloading' ? undefined : RefreshCw"
                  @click="updater.downloadAndInstall()"
                >
                  {{ updater.status.value === 'downloading' ? '正在下载…' : `更新到 v${updater.newVersion.value}` }}
                </BaseButton>
              </template>
              <BaseButton
                v-else-if="updater.status.value === 'ready'"
                size="sm"
                rounded
                @click="updater.restartToUpdate()"
              >
                重启完成更新
              </BaseButton>
              <BaseButton
                v-else
                size="sm"
                rounded
                variant="outline"
                :loading="updater.status.value === 'checking'"
                :disabled="updater.status.value === 'checking'"
                :icon="updater.status.value === 'checking' ? undefined : RefreshCw"
                @click="updater.checkForUpdate(false)"
              >
                检查更新
              </BaseButton>
              <span v-if="updater.status.value === 'uptodate'" class="text-xs text-zinc-400">已是最新</span>
            </div>
          </div>
          <!-- 更新版说明 + 下载进度 -->
          <div v-if="updater.status.value === 'available' || updater.status.value === 'downloading' || updater.releaseNotes.value" class="mt-3 border-t border-zinc-100 pt-3 dark:border-zinc-800">
            <p v-if="updater.status.value === 'available' || updater.status.value === 'downloading'" class="text-xs font-medium text-violet-500">
              发现新版本 v{{ updater.newVersion.value }}
            </p>
            <p v-if="updater.releaseNotes.value" class="mt-1 line-clamp-4 whitespace-pre-wrap text-xs leading-relaxed">{{ updater.releaseNotes.value }}</p>
            <!-- 下载进度条（total 未知时显示不定进度动画） -->
            <div v-if="updater.status.value === 'downloading'" class="mt-2 flex items-center gap-2">
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
            <p v-if="updater.status.value === 'ready'" class="text-xs font-medium text-violet-500">更新已就绪，点击「重启完成更新」生效。</p>
          </div>
        </div>
      </section>
    </div>
  </div>
</template>
