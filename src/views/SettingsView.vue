<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { getVersion } from '@tauri-apps/api/app'
import { VerifiedCheckIcon as Check } from '@solar-icons/vue/linear/verified-check'
import { FolderOpenIcon as FolderOpen } from '@solar-icons/vue/linear/folder-open'
import { GlobeIcon as Globe } from '@solar-icons/vue/linear/globe'
import { DatabaseIcon as HardDrive } from '@solar-icons/vue/linear/database'
import { MagnifierIcon as Search } from '@solar-icons/vue/linear/magnifier'
import { PaletteIcon as Palette } from '@solar-icons/vue/linear/palette'
import { PlayIcon as Play } from '@solar-icons/vue/linear/play'
import { RefreshIcon as LoaderCircle } from '@solar-icons/vue/linear/refresh'
import { RefreshIcon as RefreshCw } from '@solar-icons/vue/linear/refresh'
import { SettingsIcon as Settings } from '@solar-icons/vue/linear/settings'
import { SubtitlesIcon as Subtitles } from '@solar-icons/vue/linear/subtitles'
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
import { errorText } from '@/i18n/error'

const library = useLibraryStore()
const { mode, setTheme } = useTheme()
const { enabled: dlEnabled, toggle: dlToggle, config: dlConfig } = useDesktopLyrics()
const player = usePlayerStore()
const { t, locale } = useI18n()

// ---- 分类导航（左侧分类 / 右侧只渲染当前类）----
const CATEGORY_IDS = ['library', 'appearance', 'playback', 'search', 'lyrics', 'general'] as const
type CategoryId = (typeof CATEGORY_IDS)[number]
const SETTINGS_TAB_KEY = 'lm.settingsTab'

function readActiveTab(): CategoryId {
  const saved = localStorage.getItem(SETTINGS_TAB_KEY)
  return CATEGORY_IDS.includes(saved as CategoryId) ? (saved as CategoryId) : 'library'
}

const active = ref<CategoryId>(readActiveTab())
watch(active, (v) => localStorage.setItem(SETTINGS_TAB_KEY, v))

const categories = computed(() => [
  { id: 'library' as const, icon: HardDrive, label: t('settings.library'), desc: t('settings.libraryDesc') },
  { id: 'appearance' as const, icon: Palette, label: t('settings.appearance'), desc: t('settings.appearanceDesc') },
  { id: 'playback' as const, icon: Play, label: t('settings.playback'), desc: t('settings.playbackDesc') },
  { id: 'search' as const, icon: Search, label: t('settings.search'), desc: t('settings.searchDesc') },
  { id: 'lyrics' as const, icon: Subtitles, label: t('settings.lyrics'), desc: t('settings.lyricsDesc') },
  { id: 'general' as const, icon: Settings, label: t('settings.general'), desc: t('settings.generalDesc') },
])
const currentCategory = computed(() => categories.value.find((c) => c.id === active.value) ?? categories.value[0])

// ---- 语言 ----
const languageOptions = [
  { value: 'zh' as const, label: '简体中文' },
  { value: 'en' as const, label: 'English' },
]
function onLocaleChange(val: string | number) {
  setLocale(val as 'zh' | 'en')
}

// ---- 外观 ----
const themeItems = computed<ButtonGroupItem[]>(() => [
  { value: 'light', label: t('settings.themeLight') },
  { value: 'dark', label: t('settings.themeDark') },
  { value: 'system', label: t('settings.themeSystem') },
])
function onThemeChange(val: string) {
  setTheme(val as ThemeMode)
}

// ---- 播放设置（淡入淡出 / 阻止系统休眠）----
const fadeOn = ref(player.isFadeOn())
function onFadeToggle(val: boolean) {
  fadeOn.value = val
  player.setFadeEnabled(val)
  toast(val ? t('settings.fadeOn') : t('settings.fadeOff'))
}
const preventSleepOn = ref(getPreventSleep())
function onPreventSleepToggle(val: boolean) {
  preventSleepOn.value = val
  setPreventSleepSetting(val, player.playing)
  toast(val ? t('settings.preventSleepOn') : t('settings.preventSleepOff'))
}

// ---- 窗口关闭行为 ----
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
  toast(v === 'tray' ? t('settings.closeToTray') : t('settings.closeToQuit'))
}
const closeAction = ref(getCloseAction())
const closeActionItems = computed<ButtonGroupItem[]>(() => [
  { value: 'tray', label: t('settings.closeActionTray') },
  { value: 'quit', label: t('settings.closeActionQuit') },
])
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
const searchFieldOptions = computed(() => [
  { value: 'title', label: t('settings.fieldTitle') },
  { value: 'artist', label: t('settings.fieldArtist') },
  { value: 'album', label: t('settings.fieldAlbum') },
  { value: 'lyrics', label: t('settings.fieldLyrics') },
  { value: 'filename', label: t('settings.fieldFilename') },
])
const searchSortItems = computed<ButtonGroupItem[]>(() => [
  { value: 'relevance', label: t('settings.sortRelevance') },
  { value: 'added', label: t('settings.sortAdded') },
  { value: 'plays', label: t('settings.sortPlays') },
])
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
      toast(t('settings.separatorResplit', { count: changes.length }))
      // 艺人归属变了，刷新曲库统计与当前列表
      await Promise.all([library.loadStats(), library.loadTracks()])
    } else {
      toast(t('settings.separatorUpdated'))
    }
  } catch (e) {
    toast(errorText(e), 'error')
    // 失败时回读实际生效的设置，保持 UI 与后端一致
    api.getArtistSeparators()
      .then((v) => (artistSeparators.value = new Set(v.split(''))))
      .catch(() => {})
  } finally {
    splitApplying.value = false
  }
}

// ---- 桌面歌词：行数 / 对齐（分段控件，值为字符串，写入时转回）----
const dlLineItems = computed<ButtonGroupItem[]>(() => [
  { value: '1', label: t('settings.dlLinesSingle') },
  { value: '2', label: t('settings.dlLinesDouble') },
])
const dlAlignItems = computed<ButtonGroupItem[]>(() => [
  { value: 'left', label: t('settings.dlAlignLeft') },
  { value: 'center', label: t('settings.dlAlignCenter') },
  { value: 'right', label: t('settings.dlAlignRight') },
  { value: 'split', label: t('settings.dlAlignSplit') },
])
function onDlLinesChange(val: string) {
  dlConfig.value.lines = Number(val) === 1 ? 1 : 2
  if (dlConfig.value.lines === 1 && dlConfig.value.align === 'split') dlConfig.value.align = 'center'
}
function onDlAlignChange(val: string) {
  dlConfig.value.align = val as typeof dlConfig.value.align
}
const filteredDlAlignItems = computed(() => dlAlignItems.value.filter((o) => o.value !== 'split' || dlConfig.value.lines === 2))

// ---- 全局字体（外观）----
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

const fontSelectOptions = computed<SelectOption[]>(() => [
  { value: '', label: t('settings.defaultFont') },
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
    toast(errorText(e), 'error')
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
    toast(errorText(e), 'error')
  } finally {
    webdavBusy.value = false
  }
}

async function remove(s: { id: number; name: string; trackCount: number }) {
  const ok = await confirmDialog({
    title: t('settings.removeSourceTitle'),
    message: t('settings.removeSourceConfirm', { name: s.name, count: s.trackCount }),
    danger: true,
    confirmText: t('common.remove'),
  })
  if (!ok) return
  try {
    await library.removeSource(s.id)
  } catch (e) {
    toast(errorText(e), 'error')
  }
}

function rescan(id: number) {
  library.rescan(id).catch((e) => toast(errorText(e), 'error'))
}

function rescanFull(s: Source) {
  library.rescan(s.id, 'full').catch((e) => toast(errorText(e), 'error'))
}

async function toggleFastImport(s: Source, val?: boolean) {
  const next = val ?? !s.fastImport
  if (next === s.fastImport) return
  try {
    await library.setFastImport(s.id, next)
    toast(next ? t('settings.quickImportOn') : t('settings.quickImportOff'))
  } catch (e) {
    toast(errorText(e), 'error')
  }
}

function fmtTime(t2: number | null) {
  if (!t2) return t('settings.neverScanned')
  return new Date(t2 * 1000).toLocaleString()
}

const scannedSourceIds = computed(() => new Set(Object.keys(library.scanProgress).map(Number)))
</script>

<template>
  <div ref="root" class="flex h-full min-h-0">
    <!-- 左侧：分类导航 -->
    <aside class="flex w-56 shrink-0 flex-col border-r border-zinc-100 px-3 pt-5 pb-4 dark:border-zinc-900">
      <div class="px-3 pb-5">
        <p data-stagger class="text-xs font-semibold tracking-wider text-violet-500 uppercase">{{ t('settings.title') }}</p>
        <h1 data-stagger class="mt-0.5 text-2xl font-bold text-zinc-900 dark:text-zinc-50">{{ t('settings.preferences') }}</h1>
      </div>
      <nav class="min-h-0 flex-1 space-y-0.5 overflow-y-auto" :aria-label="t('settings.title')">
        <button
          v-for="c in categories"
          :key="c.id"
          type="button"
          class="flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-sm transition-colors"
          :class="
            active === c.id
              ? 'bg-violet-50 font-medium text-violet-600 dark:bg-violet-500/10 dark:text-violet-300'
              : 'text-zinc-600 hover:bg-zinc-100 dark:text-zinc-400 dark:hover:bg-zinc-900'
          "
          :aria-current="active === c.id ? 'page' : undefined"
          @click="active = c.id"
        >
          <component :is="c.icon" class="h-4 w-4 shrink-0" />
          <span class="truncate">{{ c.label }}</span>
        </button>
      </nav>
    </aside>

    <!-- 右侧：当前分类内容 -->
    <div class="min-w-0 flex-1 overflow-y-auto px-6 pt-5 pb-8">
      <div class="mx-auto max-w-2xl">
        <header class="mb-5">
          <h2 class="text-lg font-semibold text-zinc-900 dark:text-zinc-50">{{ currentCategory.label }}</h2>
          <p class="mt-0.5 text-xs text-zinc-400">{{ currentCategory.desc }}</p>
        </header>

        <Transition
          mode="out-in"
          enter-active-class="transition duration-200 ease-out"
          enter-from-class="translate-y-1 opacity-0"
          enter-to-class="translate-y-0 opacity-100"
          leave-active-class="transition duration-100 ease-in"
          leave-to-class="opacity-0"
        >
          <div :key="active" class="space-y-6">
            <!-- ===== 音乐库 ===== -->
            <template v-if="active === 'library'">
              <!-- 音乐来源 -->
              <section>
                <div class="mb-2.5 flex items-center justify-between gap-3">
                  <h3 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ t('settings.sources') }}</h3>
                  <BaseButton
                    size="sm"
                    :icon="adding ? LoaderCircle : FolderOpen"
                    :loading="adding"
                    :disabled="adding"
                    @click="addFolder"
                  >
                    {{ t('settings.addFolder') }}
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
                      <span class="shrink-0 text-xs text-zinc-400">
                        {{ t('settings.sourceTrackCount', { count: s.trackCount }) }} · {{ fmtTime(s.lastScanAt) }}
                      </span>
                      <div
                        class="flex shrink-0 items-center gap-1.5 text-xs text-zinc-500"
                        :title="t('settings.quickImportTip')"
                      >
                        {{ t('settings.quickImport') }}
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
                          :disabled="scannedSourceIds.has(s.id)"
                          :title="t('settings.fullParseTip')"
                          @click="rescanFull(s)"
                        >
                          {{ t('settings.fullParse') }}
                        </BaseButton>
                        <BaseButton
                          variant="ghost"
                          size="xs"
                          :icon="RefreshCw"
                          :loading="scannedSourceIds.has(s.id)"
                          :disabled="scannedSourceIds.has(s.id)"
                          :title="t('settings.incrementalScan')"
                          :aria-label="t('settings.incrementalScan')"
                          @click="rescan(s.id)"
                        />
                        <BaseButton
                          variant="ghost"
                          tone="danger"
                          size="xs"
                          :icon="Trash2"
                          :title="t('common.remove')"
                          :aria-label="t('common.remove')"
                          @click="remove(s)"
                        />
                      </div>
                    </div>
                    <div v-if="library.scanProgress[s.id]" class="mt-3">
                      <template v-if="library.scanProgress[s.id].phase === 'enumerate'">
                        <div class="mb-1 flex justify-between text-xs text-zinc-500">
                          <span>{{ t('settings.scanningEnumerate') }}</span>
                          <span class="tabular-nums">{{ t('settings.fileCount', { count: library.scanProgress[s.id].done }) }}</span>
                        </div>
                        <div class="h-1.5 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
                          <div class="h-full w-1/3 animate-pulse rounded-full bg-violet-500"></div>
                        </div>
                      </template>
                      <template v-else>
                        <div class="mb-1 flex justify-between text-xs text-zinc-500">
                          <span>{{ t('settings.scanningParse') }}</span>
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
                    {{ t('settings.noSources') }}
                  </p>
                </div>
              </section>

              <!-- WebDAV 音乐源 -->
              <section>
                <div class="mb-2.5 flex items-center justify-between gap-3">
                  <h3 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ t('settings.webdav') }}</h3>
                  <BaseButton size="sm" :icon="Globe" @click="showWebdav = !showWebdav">
                    {{ showWebdav ? t('settings.collapse') : t('settings.addWebdav') }}
                  </BaseButton>
                </div>
                <form
                  v-if="showWebdav"
                  class="space-y-3 rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900"
                  @submit.prevent="submitWebdav"
                >
                  <p class="flex items-center gap-1.5 text-xs text-zinc-400">
                    <Globe class="h-3.5 w-3.5" /> {{ t('settings.webdavHint') }}
                  </p>
                  <div class="grid gap-3" style="grid-template-columns: 2fr 1fr 1fr">
                    <BaseInput v-model="webdav.url" :placeholder="t('settings.webdavUrl')" required />
                    <BaseInput v-model="webdav.username" :placeholder="t('settings.webdavUsername')" autocomplete="off" />
                    <BaseInput v-model="webdav.password" type="password" :placeholder="t('settings.webdavPassword')" autocomplete="off" />
                  </div>
                  <div class="flex items-center gap-3">
                    <div class="w-56">
                      <BaseInput v-model="webdav.name" :placeholder="t('settings.webdavNamePlaceholder')" />
                    </div>
                    <BaseButton type="submit" :loading="webdavBusy" :disabled="webdavBusy" :icon="webdavBusy ? undefined : Check">
                      {{ t('settings.addAndScan') }}
                    </BaseButton>
                  </div>
                </form>
              </section>

              <!-- 多艺人分隔符 -->
              <section>
                <h3 class="mb-2.5 text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ t('settings.artistSeparators') }}</h3>
                <div class="rounded-xl border border-zinc-200 bg-white p-4 text-sm dark:border-zinc-800 dark:bg-zinc-900">
                  <div class="flex flex-wrap items-center justify-between gap-3">
                    <div class="flex w-full flex-wrap justify-end gap-2">
                      <BaseButton
                        v-for="sep in SEPARATOR_CANDIDATES"
                        :key="sep"
                        size="sm"
                        :variant="artistSeparators.has(sep) ? 'primary' : 'secondary'"
                        :title="sep === FIXED_SEPARATOR ? t('settings.separatorFixedTip') : t('settings.separatorToggleTip', { sep })"
                        :disabled="sep === FIXED_SEPARATOR || splitApplying"
                        @click="onSeparatorToggle(sep)"
                      >
                        {{ sep }}
                      </BaseButton>
                    </div>
                  </div>
                  <p class="mt-2 text-xs leading-relaxed text-zinc-400">{{ t('settings.artistSeparatorsDesc') }}</p>

                  <!-- 变更报告：保存后展示受影响歌曲的艺人变化 -->
                  <template v-if="splitChanges !== null">
                    <div class="mt-3 border-t border-zinc-100 pt-3 dark:border-zinc-800">
                      <p class="text-xs font-medium" :class="splitChanges.length > 0 ? 'text-violet-500' : 'text-zinc-400'">
                        {{
                          splitChanges.length > 0
                            ? t('settings.separatorApplied', { count: splitChanges.length })
                            : t('settings.separatorNoAffected')
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
                        {{ t('settings.separatorMore', { count: splitChanges.length - 200 }) }}
                      </p>
                    </div>
                  </template>
                </div>
              </section>
            </template>

            <!-- ===== 外观 ===== -->
            <template v-else-if="active === 'appearance'">
              <section>
                <div class="space-y-4 rounded-xl border border-zinc-200 bg-white p-4 text-sm dark:border-zinc-800 dark:bg-zinc-900">
                  <div class="flex items-center justify-between gap-3">
                    <span class="text-zinc-600 dark:text-zinc-300">{{ t('settings.theme') }}</span>
                    <BaseButtonGroup :model-value="mode" :items="themeItems" size="sm" @update:model-value="onThemeChange" />
                  </div>
                  <!-- 全局字体：应用于整个软件（含桌面歌词），从系统读取 -->
                  <div class="flex items-center justify-between gap-3 border-t border-zinc-100 pt-4 dark:border-zinc-800">
                    <span class="text-zinc-600 dark:text-zinc-300">{{ t('settings.fontFamily') }}</span>
                    <div class="max-w-[280px] flex-1">
                      <BaseSelect :model-value="appFont" :options="fontSelectOptions" size="sm" @update:model-value="onFontChange" />
                    </div>
                  </div>
                </div>
              </section>
            </template>

            <!-- ===== 播放 ===== -->
            <template v-else-if="active === 'playback'">
              <section>
                <div class="space-y-4 rounded-xl border border-zinc-200 bg-white p-4 text-sm dark:border-zinc-800 dark:bg-zinc-900">
                  <!-- 淡入淡出 -->
                  <div>
                    <div class="flex items-center justify-between gap-3">
                      <span class="text-zinc-600 dark:text-zinc-300">{{ t('settings.fadeInOut') }}</span>
                      <BaseSwitch
                        :model-value="fadeOn"
                        size="sm"
                        :title="t('settings.fadeInOutTip')"
                        @update:model-value="onFadeToggle"
                      />
                    </div>
                    <p class="mt-1.5 text-xs leading-relaxed text-zinc-400">{{ t('settings.fadeInOutDesc') }}</p>
                  </div>

                  <!-- 阻止系统休眠 -->
                  <div class="border-t border-zinc-100 pt-4 dark:border-zinc-800">
                    <div class="flex items-center justify-between gap-3">
                      <span class="text-zinc-600 dark:text-zinc-300">{{ t('settings.preventSleep') }}</span>
                      <BaseSwitch
                        :model-value="preventSleepOn"
                        size="sm"
                        :title="t('settings.preventSleepTip')"
                        @update:model-value="onPreventSleepToggle"
                      />
                    </div>
                    <p class="mt-1.5 text-xs leading-relaxed text-zinc-400">{{ t('settings.preventSleepDesc') }}</p>
                  </div>

                  <!-- 关闭窗口行为 -->
                  <div class="border-t border-zinc-100 pt-4 dark:border-zinc-800">
                    <div class="flex flex-wrap items-center justify-between gap-3">
                      <span class="text-zinc-600 dark:text-zinc-300">{{ t('settings.closeAction') }}</span>
                      <BaseButtonGroup
                        :model-value="closeAction"
                        :items="closeActionItems"
                        size="sm"
                        @update:model-value="setCloseAction"
                      />
                    </div>
                    <p class="mt-1.5 text-xs leading-relaxed text-zinc-400">{{ t('settings.closeActionDesc') }}</p>
                  </div>
                </div>
              </section>
            </template>

            <!-- ===== 搜索 ===== -->
            <template v-else-if="active === 'search'">
              <section>
                <div class="space-y-4 rounded-xl border border-zinc-200 bg-white p-4 text-sm dark:border-zinc-800 dark:bg-zinc-900">
                  <!-- 搜索范围 -->
                  <div>
                    <p class="mb-2 text-xs font-medium text-zinc-400">{{ t('settings.searchScope') }}</p>
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
                    <p class="mt-1.5 text-xs leading-relaxed text-zinc-400">{{ t('settings.searchScopeHint') }}</p>
                  </div>

                  <!-- 拼音搜索 -->
                  <div class="border-t border-zinc-100 pt-4 dark:border-zinc-800">
                    <div class="flex items-center justify-between gap-3">
                      <span class="text-zinc-600 dark:text-zinc-300">{{ t('settings.pinyinSearch') }}</span>
                      <BaseSwitch
                        :model-value="searchSettings.pinyin"
                        size="sm"
                        :title="t('settings.pinyinTip')"
                        @update:model-value="togglePinyin"
                      />
                    </div>
                    <p class="mt-1.5 text-xs leading-relaxed text-zinc-400">{{ t('settings.pinyinDesc') }}</p>
                  </div>

                  <!-- 排序偏好 -->
                  <div class="border-t border-zinc-100 pt-4 dark:border-zinc-800">
                    <div class="flex flex-wrap items-center justify-between gap-3">
                      <span class="text-zinc-600 dark:text-zinc-300">{{ t('settings.sortPreference') }}</span>
                      <BaseButtonGroup
                        :model-value="searchSettings.sort"
                        :items="searchSortItems"
                        size="sm"
                        @update:model-value="onSearchSortChange"
                      />
                    </div>
                  </div>

                  <!-- 输入防抖 -->
                  <div class="border-t border-zinc-100 pt-4 dark:border-zinc-800">
                    <div class="flex items-center justify-between gap-3">
                      <span class="text-zinc-600 dark:text-zinc-300">{{ t('settings.inputDebounce') }}</span>
                      <div class="w-28">
                        <BaseSelect
                          :model-value="searchSettings.debounceMs"
                          :options="debounceSelectOptions"
                          size="sm"
                          @update:model-value="onDebounceChange"
                        />
                      </div>
                    </div>
                    <p class="mt-1.5 text-xs leading-relaxed text-zinc-400">{{ t('settings.inputDebounceDesc') }}</p>
                  </div>
                </div>
              </section>
            </template>

            <!-- ===== 歌词 ===== -->
            <template v-else-if="active === 'lyrics'">
              <section>
                <div class="rounded-xl border border-zinc-200 bg-white p-4 text-sm dark:border-zinc-800 dark:bg-zinc-900">
                  <!-- 预览：与浮窗样式一致，随下方设置实时变化 -->
                  <div
                    class="flex min-h-[76px] flex-col justify-center gap-1 rounded-xl px-5 py-3"
                    :style="previewBoxStyle"
                  >
                    <p class="truncate font-bold" :style="previewMainStyle">{{ t('settings.dlPreviewMain') }}</p>
                    <p class="truncate" :style="previewPendingStyle">{{ t('settings.dlPreviewPending') }}</p>
                  </div>

                  <!-- 显示 -->
                  <div class="mt-5 mb-3 flex items-center gap-2">
                    <span class="text-xs font-medium text-zinc-400">{{ t('settings.dlDisplay') }}</span>
                    <span class="h-px flex-1 bg-zinc-100 dark:bg-zinc-800"></span>
                  </div>
                  <div class="grid grid-cols-1 gap-x-8 gap-y-4 sm:grid-cols-2">
                    <!-- 开关 -->
                    <div class="flex items-center justify-between gap-3">
                      <span class="text-zinc-600 dark:text-zinc-300">{{ t('settings.dlShow') }}</span>
                      <BaseSwitch
                        :model-value="dlEnabled"
                        size="sm"
                        :label="dlEnabled ? t('desktopLyrics.enabled') : t('desktopLyrics.disabled')"
                        @update:model-value="() => dlToggle()"
                      />
                    </div>
                    <!-- 显示行数 -->
                    <div class="flex items-center justify-between gap-3">
                      <span class="text-zinc-600 dark:text-zinc-300">{{ t('settings.dlLines') }}</span>
                      <BaseButtonGroup
                        :model-value="String(dlConfig.lines)"
                        :items="dlLineItems"
                        size="sm"
                        @update:model-value="onDlLinesChange"
                      />
                    </div>
                    <!-- 对齐方式（选项较多，独占一行） -->
                    <div class="flex items-center justify-between gap-3 sm:col-span-2">
                      <span class="shrink-0 text-zinc-600 dark:text-zinc-300">{{ t('settings.dlAlign') }}</span>
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
                    <span class="text-xs font-medium text-zinc-400">{{ t('settings.dlStyle') }}</span>
                    <span class="h-px flex-1 bg-zinc-100 dark:bg-zinc-800"></span>
                  </div>
                  <div class="grid grid-cols-1 gap-x-8 gap-y-4 sm:grid-cols-2">
                    <!-- 播放行颜色 -->
                    <div class="flex items-center justify-between gap-3">
                      <span class="text-zinc-600 dark:text-zinc-300">{{ t('settings.dlPlayColor') }}</span>
                      <BaseColorPicker v-model="dlConfig.color" :title="t('settings.dlPlayColor')" />
                    </div>
                    <!-- 未播放行颜色 -->
                    <div class="flex items-center justify-between gap-3">
                      <span class="text-zinc-600 dark:text-zinc-300">{{ t('settings.dlPendingColor') }}</span>
                      <BaseColorPicker v-model="dlConfig.pendingColor" :title="t('settings.dlPendingColor')" />
                    </div>
                    <!-- 描边 -->
                    <div class="flex items-center justify-between gap-3">
                      <span class="text-zinc-600 dark:text-zinc-300">{{ t('settings.dlOutline') }}</span>
                      <div class="flex items-center gap-2">
                        <BaseColorPicker
                          v-model="dlConfig.outlineColor"
                          :disabled="!dlConfig.outline"
                          :title="t('settings.dlOutlineColor')"
                        />
                        <BaseCheckbox v-model="dlConfig.outline" size="sm" />
                      </div>
                    </div>
                    <!-- 字体加粗 -->
                    <div class="flex items-center justify-between gap-3">
                      <span class="text-zinc-600 dark:text-zinc-300">{{ t('settings.dlBold') }}</span>
                      <BaseCheckbox v-model="dlConfig.bold" size="sm" />
                    </div>
                    <!-- 字号（独占一行，滑杆拉满宽度） -->
                    <div class="flex items-center gap-3 sm:col-span-2">
                      <span class="shrink-0 text-zinc-600 dark:text-zinc-300">{{ t('settings.dlFontSize', { size: dlConfig.fontSize }) }}</span>
                      <div class="w-full min-w-0 flex-1">
                        <BaseSlider v-model="dlConfig.fontSize" :min="18" :max="56" :step="2" />
                      </div>
                    </div>
                  </div>

                  <!-- 背景 -->
                  <div class="mt-5 mb-3 flex items-center gap-2">
                    <span class="text-xs font-medium text-zinc-400">{{ t('settings.dlBackground') }}</span>
                    <span class="h-px flex-1 bg-zinc-100 dark:bg-zinc-800"></span>
                  </div>
                  <div class="grid grid-cols-1 gap-x-8 gap-y-4 sm:grid-cols-2">
                    <!-- 背景颜色 -->
                    <div class="flex items-center justify-between gap-3">
                      <span class="text-zinc-600 dark:text-zinc-300">{{ t('settings.dlBgColor') }}</span>
                      <BaseColorPicker v-model="dlConfig.bgColor" :title="t('settings.dlBgColor')" />
                    </div>
                    <!-- 背景不透明度 -->
                    <div class="flex items-center gap-3">
                      <span class="shrink-0 text-zinc-600 dark:text-zinc-300">
                        {{ t('settings.dlBgOpacity', { value: Math.round(dlConfig.bgOpacity * 100) }) }}
                      </span>
                      <div class="w-full min-w-0 flex-1">
                        <BaseSlider v-model="dlConfig.bgOpacity" :min="0" :max="0.85" :step="0.05" />
                      </div>
                    </div>
                  </div>
                  <p class="mt-4 text-xs leading-relaxed text-zinc-400">{{ t('settings.dlHint') }}</p>
                </div>
              </section>
            </template>

            <!-- ===== 通用 ===== -->
            <template v-else>
              <!-- 语言 -->
              <section>
                <h3 class="mb-2.5 text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ t('settings.language') }}</h3>
                <div class="rounded-xl border border-zinc-200 bg-white p-4 text-sm text-zinc-500 dark:border-zinc-800 dark:bg-zinc-900">
                  <div class="flex flex-wrap items-center justify-between gap-3">
                    <span>{{ t('settings.languageSelect') }}</span>
                    <div class="w-40 shrink-0">
                      <BaseSelect :model-value="locale" :options="languageOptions" size="sm" @update:model-value="onLocaleChange" />
                    </div>
                  </div>
                </div>
              </section>

              <!-- 关于 -->
              <section>
                <h3 class="mb-2.5 text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ t('settings.about') }}</h3>
                <div class="rounded-xl border border-zinc-200 bg-white p-4 text-sm text-zinc-500 dark:border-zinc-800 dark:bg-zinc-900">
                  <div class="flex flex-wrap items-center justify-between gap-3">
                    <span>{{ t('settings.versionLine', { version: appVersion }) }}</span>
                    <!-- 更新操作区：按状态切换 -->
                    <div class="flex items-center gap-2">
                      <template v-if="updater.status.value === 'available' || updater.status.value === 'downloading'">
                        <BaseButton
                          size="sm"
                          :loading="updater.status.value === 'downloading'"
                          :icon="updater.status.value === 'downloading' ? undefined : RefreshCw"
                          @click="updater.downloadAndInstall()"
                        >
                          {{
                            updater.status.value === 'downloading'
                              ? t('settings.downloadingUpdate')
                              : t('settings.updateTo', { version: updater.newVersion.value })
                          }}
                        </BaseButton>
                      </template>
                      <BaseButton
                        v-else-if="updater.status.value === 'ready'"
                        size="sm"
                        @click="updater.restartToUpdate()"
                      >
                        {{ t('settings.restartToUpdate') }}
                      </BaseButton>
                      <BaseButton
                        v-else
                        size="sm"
                        variant="outline"
                        :loading="updater.status.value === 'checking'"
                        :disabled="updater.status.value === 'checking'"
                        :icon="updater.status.value === 'checking' ? undefined : RefreshCw"
                        @click="updater.checkForUpdate(false)"
                      >
                        {{ t('settings.checkForUpdates') }}
                      </BaseButton>
                      <span v-if="updater.status.value === 'uptodate'" class="text-xs text-zinc-400">{{ t('settings.upToDateShort') }}</span>
                    </div>
                  </div>
                  <!-- 更新版说明 + 下载进度 -->
                  <div v-if="updater.status.value === 'available' || updater.status.value === 'downloading' || updater.releaseNotes.value" class="mt-3 border-t border-zinc-100 pt-3 dark:border-zinc-800">
                    <p v-if="updater.status.value === 'available' || updater.status.value === 'downloading'" class="text-xs font-medium text-violet-500">
                      {{ t('settings.updateFound', { version: updater.newVersion.value }) }}
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
                    <p v-if="updater.status.value === 'ready'" class="text-xs font-medium text-violet-500">{{ t('settings.updateReadyHint') }}</p>
                  </div>
                </div>
              </section>
            </template>
          </div>
        </Transition>
      </div>
    </div>
  </div>
</template>
