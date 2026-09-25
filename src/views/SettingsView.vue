<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { getVersion } from '@tauri-apps/api/app'
import { DatabaseIcon as HardDriveBold } from '@solar-icons/vue/bold/database'
import { MagnifierIcon as SearchBold } from '@solar-icons/vue/bold/magnifier'
import { PaletteIcon as PaletteBold } from '@solar-icons/vue/bold/palette'
import { PlayIcon as PlayBold } from '@solar-icons/vue/bold/play'
import { MusicNote2Icon as MusicNoteBold } from '@solar-icons/vue/bold/music-note-2'
import { SettingsIcon as SettingsBold } from '@solar-icons/vue/bold/settings'
import { SubtitlesIcon as SubtitlesBold } from '@solar-icons/vue/bold/subtitles'
import { VerifiedCheckIcon as Check } from '@solar-icons/vue/linear/verified-check'
import { FolderOpenIcon as FolderOpen } from '@solar-icons/vue/linear/folder-open'
import { GlobeIcon as Globe } from '@solar-icons/vue/linear/globe'
import { InfoCircleIcon as InfoCircle } from '@solar-icons/vue/linear/info-circle'
import { GalleryIcon as ImageIcon } from '@solar-icons/vue/linear/gallery'
import { DatabaseIcon as HardDrive } from '@solar-icons/vue/linear/database'
import { MagnifierIcon as Search } from '@solar-icons/vue/linear/magnifier'
import { PaletteIcon as Palette } from '@solar-icons/vue/linear/palette'
import { PlayIcon as Play } from '@solar-icons/vue/linear/play'
import { MusicNote2Icon as MusicNote } from '@solar-icons/vue/linear/music-note-2'
import { RefreshIcon as LoaderCircle } from '@solar-icons/vue/linear/refresh'
import { RestartIcon as RotateCcw } from '@solar-icons/vue/linear/restart'
import { RefreshIcon as RefreshCw } from '@solar-icons/vue/linear/refresh'
import { SettingsIcon as Settings } from '@solar-icons/vue/linear/settings'
import { SubtitlesIcon as Subtitles } from '@solar-icons/vue/linear/subtitles'
import { TrashBin2Icon as Trash2 } from '@solar-icons/vue/linear/trash-bin-2'
import { useLibraryStore } from '@/stores/library'
import { useTheme, type ThemeMode } from '@/composables/useTheme'
import { useThemeColor } from '@/composables/useThemeColor'
import { toast } from '@/composables/useToast'
import { confirmDialog } from '@/composables/useConfirm'
import { useStagger } from '@/composables/useStagger'
import { useDesktopLyrics } from '@/composables/useDesktopLyrics'
import { getAppFont, setAppFont } from '@/composables/useAppFont'
import { dialogBlur, dialogDraggable, type DialogBlur } from '@/composables/useDialogPrefs'
import { isFocusModeEnabled, setFocusModeEnabled } from '@/composables/useFocusMode'
import { useBackground } from '@/composables/useBackground'
import { getPreventSleep, setPreventSleepSetting } from '@/composables/usePowerGuard'
import { useUpdater } from '@/composables/useUpdater'
import { useStatsEntry } from '@/composables/useStatsEntry'
import { useNav } from '@/composables/useNav'
import AudioEffectsPanel from '@/components/AudioEffectsPanel.vue'
import MergedArtistsDialog from '@/components/MergedArtistsDialog.vue'
import {
  SHORTCUT_DEFS,
  GLOBAL_DEFS,
  useShortcuts,
  useGlobalShortcuts,
  setShortcut,
  resetShortcuts,
  comboFromEvent,
  accelFromEvent,
  formatCombo,
  formatAccel,
  type ShortcutAction,
  type GlobalAction,
} from '@/composables/useShortcuts'
import { usePlayerStore } from '@/stores/player'
import { api } from '@/api/commands'
import { getSearchSettings, setSearchSettings, type SearchSettings } from '@/composables/useSearchSettings'
import type { ArtistAlias, ArtistNormalizeChange, ArtistSplitChange, RemovedTrack, Source } from '@/types'
import { setLocale } from '@/i18n'
import { useI18n } from 'vue-i18n'
import {
  BaseButton,
  BaseButtonGroup,
  BaseCheckbox,
  BaseColorPicker,
  BaseInput,
  BaseModal,
  BaseSwitch,
  BaseSelect,
  BaseSlider,
  BaseTagInput,
} from '@/components/ui'
import type { ButtonGroupItem, SelectOption } from '@/components/ui'
import { errorText } from '@/i18n/error'
import { hexToRgba } from '@/utils/color'

const library = useLibraryStore()
const { mode, setTheme } = useTheme()
// ---- 主题色：Ant Design 色板预设（覆盖 --color-violet-* 变量全局换肤，见 useThemeColor.ts） ----
const { themeColor, presets, setThemeColor } = useThemeColor()
const { enabled: dlEnabled, toggle: dlToggle, config: dlConfig } = useDesktopLyrics()
const player = usePlayerStore()
const { t, locale } = useI18n()

// ---- 锚点目录：六个分类纵向铺开；顶部目录点击滚动跳转，滚动时反向高亮当前分区 ----
const CATEGORY_IDS = ['library', 'appearance', 'playback', 'audio', 'search', 'lyrics', 'general'] as const
type CategoryId = (typeof CATEGORY_IDS)[number]
const SECTION_ID_PREFIX = 'settings-section-'

/** 当前分区：点击目录即时更新，滚动时由 scroll spy 跟随。
 * 刻意**不持久化**：设置页每次进入都从顶部开始，不回到上次看的分区/滚动位置 */
const active = ref<CategoryId>(CATEGORY_IDS[0])

/** 点击目录项：平滑滚动到对应分区（滚动途经的分区会依次高亮，最终停在目标分区） */
function scrollToSection(id: CategoryId) {
  active.value = id
  document.getElementById(SECTION_ID_PREFIX + id)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
}

const categories = computed(() => [
  { id: 'library' as const, icon: HardDrive, iconActive: HardDriveBold, label: t('settings.library'), desc: t('settings.libraryDesc') },
  { id: 'appearance' as const, icon: Palette, iconActive: PaletteBold, label: t('settings.appearance'), desc: t('settings.appearanceDesc') },
  { id: 'playback' as const, icon: Play, iconActive: PlayBold, label: t('settings.playback'), desc: t('settings.playbackDesc') },
  { id: 'audio' as const, icon: MusicNote, iconActive: MusicNoteBold, label: t('settings.audio'), desc: t('settings.audioDesc') },
  { id: 'search' as const, icon: Search, iconActive: SearchBold, label: t('settings.search'), desc: t('settings.searchDesc') },
  { id: 'lyrics' as const, icon: Subtitles, iconActive: SubtitlesBold, label: t('settings.lyrics'), desc: t('settings.lyricsDesc') },
  { id: 'general' as const, icon: Settings, iconActive: SettingsBold, label: t('settings.general'), desc: t('settings.generalDesc') },
])

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
  // key 去重：同一开关反复切换只替换本条提示，不叠加新气泡
  toast(val ? t('settings.fadeOn') : t('settings.fadeOff'), 'info', 'settings.fade')
}
const preventSleepOn = ref(getPreventSleep())
function onPreventSleepToggle(val: boolean) {
  preventSleepOn.value = val
  setPreventSleepSetting(val, player.playing)
  toast(val ? t('settings.preventSleepOn') : t('settings.preventSleepOff'), 'info', 'settings.preventSleep')
}

// ---- 窗口关闭行为 ----
type CloseAction = 'tray' | 'quit'
function getCloseAction(): CloseAction {
  const v = localStorage.getItem('lm.closeAction')
  return v === 'quit' ? 'quit' : 'tray'
}
function setCloseAction(action: string | number) {
  const v = action as CloseAction
  closeActionTouched = true // 用户已手动操作：启动期的 SQLite 回读不得再回滚此值
  localStorage.setItem('lm.closeAction', v)
  // 同步到 SQLite，供 Rust 侧关闭事件使用
  api.setSetting('lm.closeAction', v).catch(() => {})
  toast(v === 'tray' ? t('settings.closeToTray') : t('settings.closeToQuit'), 'info', 'settings.closeAction')
}
const closeAction = ref(getCloseAction())
let closeActionTouched = false
const closeActionItems = computed<ButtonGroupItem[]>(() => [
  { value: 'tray', label: t('settings.closeActionTray') },
  { value: 'quit', label: t('settings.closeActionQuit') },
])
// 从 SQLite 加载设置（如果存在）
onMounted(() => {
  api.getSetting('lm.closeAction')
    .then((v) => {
      if (closeActionTouched) return
      if (v === 'tray' || v === 'quit') {
        closeAction.value = v
        localStorage.setItem('lm.closeAction', v)
      }
    })
    .catch(() => {})
})

// ---- 快捷键（应用内 + 全局，见 useShortcuts.ts）----
const appShortcuts = useShortcuts()
const gs = useGlobalShortcuts()
const globalEnabled = gs.enabled
const globalBindings = gs.bindings
const appRows = SHORTCUT_DEFS
const globalRows = GLOBAL_DEFS

/** 恢复全部应用内默认快捷键 */
function onResetShortcuts() {
  resetShortcuts()
  toast(t('settings.shortcutSaved'), 'info', 'settings.shortcut')
}

async function onGlobalToggle(val: boolean) {
  await gs.setEnabled(val)
}

// 录制器：点击键位按钮进入录制，下一个按键即写入（Esc 取消 / Backspace 清除）。
// 用 window capture 监听：先于 App.vue 的 bubble 快捷键触发，preventDefault + stopPropagation
// 保证录制期间不会误触发播放/切歌等动作。
const recording = ref<string | null>(null) // 'app:{action}' | 'global:{action}'
let recordListener: ((e: KeyboardEvent) => void) | null = null
const RECORD_IGNORED_KEYS = ['Control', 'Meta', 'Alt', 'Shift', 'CapsLock', 'NumLock', 'ScrollLock', 'Tab']

function isRecording(kind: 'app' | 'global', action: string) {
  return recording.value === `${kind}:${action}`
}

function startRecording(kind: 'app' | 'global', action: string) {
  stopRecording()
  recording.value = `${kind}:${action}`
  recordListener = (e: KeyboardEvent) => onRecordKeydown(kind, action, e)
  window.addEventListener('keydown', recordListener, true)
}

function stopRecording() {
  recording.value = null
  if (recordListener) {
    window.removeEventListener('keydown', recordListener, true)
    recordListener = null
  }
}
onBeforeUnmount(stopRecording)

/**
 * 录制中失焦（点击其他位置 / Tab 切走 / 点击别处按钮）：结束录制。
 * 录制期间按钮内始终显示当前键位；若已录到新组合则显示新值，否则保持原值。
 */
function onRecordBlur() {
  stopRecording()
}

function onRecordKeydown(kind: 'app' | 'global', action: string, e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    stopRecording()
    return
  }
  // 单独按修饰键（及 Tab）：等待主键，不结束录制
  if (RECORD_IGNORED_KEYS.includes(e.key)) return
  e.preventDefault()
  e.stopPropagation()
  if (e.key === 'Backspace') {
    void saveRecorded(kind, action, null)
    return
  }
  if (kind === 'app') {
    const combo = comboFromEvent(e)
    if (combo) void saveRecorded(kind, action, combo)
  } else {
    const accel = accelFromEvent(e)
    if (!accel) {
      toast(t('settings.shortcutUnsupported'), 'error')
      return
    }
    void saveRecorded(kind, action, accel)
  }
}

async function saveRecorded(kind: 'app' | 'global', action: string, value: string | null) {
  stopRecording()
  if (kind === 'app') {
    const act = action as ShortcutAction
    // 冲突检测：与其他应用内动作重复则拒绝
    if (value) {
      const clash = appRows.find(
        (d) =>
          d.action !== act &&
          appShortcuts.value[d.action] &&
          value.toLowerCase() === (appShortcuts.value[d.action] ?? '').toLowerCase(),
      )
      if (clash) {
        toast(t('settings.shortcutTaken', { name: t(clash.labelKey) }), 'error')
        return
      }
    }
    setShortcut(act, value)
    toast(t('settings.shortcutSaved'), 'info', 'settings.shortcut')
  } else {
    const act = action as GlobalAction
    // 冲突检测：与其他全局动作重复则拒绝
    if (value) {
      const clash = globalRows.find(
        (d) => d.action !== act && globalBindings.value[d.action] && value === globalBindings.value[d.action],
      )
      if (clash) {
        toast(t('settings.shortcutTaken', { name: t(clash.labelKey) }), 'error')
        return
      }
    }
    // 启用状态下即时重新注册（被其他程序占用时 composable 内回滚并提示）
    if (await gs.setBinding(act, value)) {
      toast(t('settings.shortcutGlobalSaved'), 'info', 'settings.globalShortcut')
    }
  }
}

// ---- 歌词来源优先级（设置 → 歌词）----
// 值为逗号分隔的来源顺序，Rust 侧取歌词时按此顺序尝试（见 src-tauri/src/lyrics.rs lyric_priority）
const LYRIC_PRIORITY_KEY = 'lm.lyricPriority'
const LYRIC_PRIORITY_PARTS = ['qrc', 'lrc', 'embedded'] as const
const LYRIC_SRC_LABEL_KEYS = {
  qrc: 'settings.lyricSrcQrc',
  lrc: 'settings.lyricSrcLrc',
  embedded: 'settings.lyricSrcEmbedded',
} as const
type LyricSrcPart = (typeof LYRIC_PRIORITY_PARTS)[number]

/** 解析并修补顺序：非法项忽略、重复去重、缺失层级按默认顺序补齐（三种来源都保留） */
function normalizeLyricPriority(v: string | null | undefined): string {
  const out: LyricSrcPart[] = []
  for (const part of (v ?? '').split(',')) {
    const p = part.trim() as LyricSrcPart
    if ((LYRIC_PRIORITY_PARTS as readonly string[]).includes(p) && !out.includes(p)) {
      out.push(p)
    }
  }
  for (const p of LYRIC_PRIORITY_PARTS) {
    if (!out.includes(p)) out.push(p)
  }
  return out.join(',')
}

/** N 个来源的全排列 → 6 个预设顺序选项（标签形如「外挂 QRC → 外挂 LRC → 内嵌歌词」） */
function permutations<T>(items: T[]): T[][] {
  if (items.length <= 1) return [items]
  return items.flatMap((item, i) =>
    permutations(items.filter((_, j) => j !== i)).map((rest) => [item, ...rest]),
  )
}
const lyricPriority = ref(normalizeLyricPriority(localStorage.getItem(LYRIC_PRIORITY_KEY)))
let lyricPriorityTouched = false
const lyricPriorityOptions = computed<SelectOption[]>(() =>
  permutations([...LYRIC_PRIORITY_PARTS]).map((order) => ({
    value: order.join(','),
    label: order.map((s) => t(LYRIC_SRC_LABEL_KEYS[s])).join(' → '),
  })),
)
function onLyricPriorityChange(val: string | number) {
  const v = normalizeLyricPriority(String(val))
  lyricPriorityTouched = true // 用户已手动操作：启动期的 SQLite 回读不得再回滚此值
  lyricPriority.value = v
  localStorage.setItem(LYRIC_PRIORITY_KEY, v)
  // 同步到 SQLite，供 Rust 侧取歌词时读取；当前曲目歌词按新优先级立即重载
  api.setSetting(LYRIC_PRIORITY_KEY, v).catch(() => {})
  player.reloadLyrics()
  toast(t('settings.lyricPrioritySaved'))
}
onMounted(() => {
  api
    .getSetting(LYRIC_PRIORITY_KEY)
    .then((v) => {
      if (lyricPriorityTouched || !v) return
      const n = normalizeLyricPriority(v)
      lyricPriority.value = n
      localStorage.setItem(LYRIC_PRIORITY_KEY, n)
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

// ---- 曲库：艺人名规整（合并同义艺人，如「陈奕迅（Eason Chan）」→「陈奕迅」）----
/** 最近一次规整的变更列表（null = 尚未执行过） */
const normalizeChanges = ref<ArtistNormalizeChange[] | null>(null)
const normalizeApplying = ref(false)

async function onNormalizeArtists() {
  normalizeApplying.value = true
  try {
    const changes = await api.normalizeArtistNames()
    normalizeChanges.value = changes
    if (changes.length > 0) {
      toast(t('settings.artistNormalizeApplied', { count: changes.length }))
      // 艺人归属变了，刷新曲库统计与当前列表
      await Promise.all([library.loadStats(), library.loadTracks()])
    } else {
      toast(t('settings.artistNormalizeNone'))
    }
    // 规整会产生新的合并记录，已合并名单与艺人下拉同步刷新
    await Promise.all([loadArtistAliases(), loadArtistOptions()])
  } catch (e) {
    toast(errorText(e), 'error')
  } finally {
    normalizeApplying.value = false
  }
}

// ---- 已合并名单 + 自定义合并（a 与 b 实为同一人时手动归并）----
const artistAliases = ref<ArtistAlias[]>([])
const mergedDialogOpen = ref(false)

async function loadArtistAliases() {
  try {
    artistAliases.value = await api.listArtistAliases()
  } catch (e) {
    toast(errorText(e), 'error')
  }
}
void loadArtistAliases()

/** 取消合并后：合并记录、艺人下拉、曲库统计都变了，统一刷新 */
async function onUnmerged() {
  await Promise.all([loadArtistAliases(), loadArtistOptions(), library.loadStats()])
}

/** 艺人下拉选项（按名称排序，最多前 1000 位——与艺人页同一上限口径） */
const artistOptions = ref<SelectOption[]>([])
const artistOptionsLoading = ref(false)

async function loadArtistOptions() {
  artistOptionsLoading.value = true
  try {
    const page = await api.queryArtists(undefined, 0, 1000)
    artistOptions.value = page.items.map((a) => ({ value: a.id, label: a.name }))
  } catch (e) {
    toast(errorText(e), 'error')
  } finally {
    artistOptionsLoading.value = false
  }
}
void loadArtistOptions()

const mergeKeep = ref<string | number>('')
const mergeAbsorb = ref<string | number>('')
const merging = ref(false)

async function onMergeArtist() {
  if (mergeKeep.value === '' || mergeAbsorb.value === '' || mergeKeep.value === mergeAbsorb.value) return
  merging.value = true
  try {
    const change = await api.mergeArtist(Number(mergeAbsorb.value), Number(mergeKeep.value))
    toast(
      t('settings.artistMergeDone', {
        old: change.oldName,
        new: change.newName,
        count: change.trackCount,
      }),
    )
    mergeAbsorb.value = ''
    // 合并后旧名成为别名、下拉里少一位艺人：两份列表都要刷新
    await Promise.all([loadArtistAliases(), loadArtistOptions(), library.loadStats()])
  } catch (e) {
    toast(errorText(e), 'error')
  } finally {
    merging.value = false
  }
}
const DL_PLAY_PRESETS = ['#a78bfa', '#ffffff', '#22d3ee', '#4ade80', '#f472b6', '#fbbf24']
const DL_PENDING_PRESETS = ['#22d3ee', '#f472b6', '#38bdf8', '#fbbf24', '#a78bfa', '#71717a', '#a1a1aa']

/** 桌面歌词配色方案：每项一对明显不同的色值（播放行 / 未播放行），下拉点选即同时应用 */
const DL_COLOR_SCHEMES: Record<string, { color: string; pendingColor: string }> = {
  violet: { color: '#a78bfa', pendingColor: '#22d3ee' },
  green: { color: '#4ade80', pendingColor: '#f472b6' },
  pink: { color: '#f472b6', pendingColor: '#38bdf8' },
  blue: { color: '#38bdf8', pendingColor: '#fbbf24' },
  gray: { color: '#ffffff', pendingColor: '#71717a' },
  yellow: { color: '#fbbf24', pendingColor: '#a78bfa' },
}
/** 当前配色命中的方案名；两色与所有方案都不匹配时为 custom（手动微调后自动回落） */
const dlPreset = computed(() => {
  const hit = Object.entries(DL_COLOR_SCHEMES).find(
    ([, s]) =>
      s.color.toLowerCase() === dlConfig.value.color.toLowerCase() &&
      s.pendingColor.toLowerCase() === dlConfig.value.pendingColor.toLowerCase(),
  )
  return hit?.[0] ?? 'custom'
})
const dlPresetOptions = computed<SelectOption[]>(() => [
  { value: 'custom', label: t('settings.dlPresetCustom') },
  { value: 'violet', label: t('settings.dlPresetViolet') },
  { value: 'green', label: t('settings.dlPresetGreen') },
  { value: 'pink', label: t('settings.dlPresetPink') },
  { value: 'blue', label: t('settings.dlPresetBlue') },
  { value: 'gray', label: t('settings.dlPresetGray') },
  { value: 'yellow', label: t('settings.dlPresetYellow') },
])
function onDlPresetChange(val: string | number) {
  const scheme = DL_COLOR_SCHEMES[val as string]
  if (!scheme) return
  dlConfig.value.color = scheme.color
  dlConfig.value.pendingColor = scheme.pendingColor
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

/** 桌面歌词预览：与浮窗完全一致的样式计算（hexToRgba 实现在 utils/color.ts，与歌词浮窗共用） */
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
/** 预览译文行：与浮窗一致——独立第二行，字号与主行一致、不加粗、降不透明度 */
const previewTransStyle = computed(() => ({
  color: dlConfig.value.color,
  fontSize: `${Math.min(dlConfig.value.fontSize, 28)}px`,
  fontWeight: 500,
  opacity: 0.72,
  textShadow: previewShadow.value,
  textAlign: dlConfig.value.align === 'split' ? 'right' : dlConfig.value.align,
}))

const root = ref<HTMLElement | null>(null)
useStagger(root, ref(true))

// ---- 滚动高亮（scroll spy，rAF 节流）：高亮「顶部已越过容器顶 80px」的最后一个分区 ----
const contentEl = ref<HTMLElement | null>(null)
let spyTicking = false
let spyRaf = 0
function onContentScroll() {
  if (spyTicking) return
  spyTicking = true
  spyRaf = requestAnimationFrame(updateActiveFromScroll)
}
onBeforeUnmount(() => {
  if (spyRaf) cancelAnimationFrame(spyRaf)
})
function sectionEls(): HTMLElement[] {
  return root.value ? Array.from(root.value.querySelectorAll<HTMLElement>('[data-settings-section]')) : []
}
function updateActiveFromScroll() {
  spyTicking = false
  const container = contentEl.value
  if (!container) return
  const containerTop = container.getBoundingClientRect().top
  let current: CategoryId = CATEGORY_IDS[0]
  for (const el of sectionEls()) {
    if (el.getBoundingClientRect().top - containerTop <= 80) {
      current = (el.dataset.settingsSection ?? CATEGORY_IDS[0]) as CategoryId
    } else {
      break
    }
  }
  // 滚到底时高亮最后一节（末节可能不够高、滚不到容器顶）
  if (container.scrollTop + container.clientHeight >= container.scrollHeight - 4) {
    current = CATEGORY_IDS[CATEGORY_IDS.length - 1]
  }
  if (active.value !== current) active.value = current
}

// 刻意不做位置恢复：组件每次都是新建（App.vue 的视图带 :key），容器 scrollTop 天然为 0，
// 挂载后只需同步一次高亮（首帧即 library）
onMounted(() => {
  spyRaf = requestAnimationFrame(updateActiveFromScroll)
})

// ---- 弹窗偏好（外观）：可拖动 + 背景模糊度；弹窗打开时读取，立即生效于下一个弹窗 ----
const dialogDragOn = ref(dialogDraggable())
function setDialogDrag(v: boolean) {
  dialogDragOn.value = v
  localStorage.setItem('lm.dialogDrag', v ? '1' : '0')
}
const dialogBlurLevel = ref<DialogBlur>(dialogBlur())
const dialogBlurOptions = computed<SelectOption[]>(() => [
  { value: 'none', label: t('settings.blurNone') },
  { value: 'sm', label: t('settings.blurLight') },
  { value: 'md', label: t('settings.blurMedium') },
  { value: 'lg', label: t('settings.blurHeavy') },
])
function onDialogBlur(v: string | number) {
  dialogBlurLevel.value = v as DialogBlur
  localStorage.setItem('lm.dialogBlur', String(v))
}

// ---- 自定义背景（外观）：本机图片复制到应用数据目录，经 bg:// 协议铺满底色区（useBackground 模块级单例） ----
const { file: bgFile, blur: bgBlur, set: setBg } = useBackground()
const bgBlurOptions = computed<SelectOption[]>(() => [
  { value: '0', label: t('settings.blurNone') },
  { value: '6', label: t('settings.blurLight') },
  { value: '12', label: t('settings.blurMedium') },
  { value: '20', label: t('settings.blurHeavy') },
])
function onBgBlur(v: string | number) {
  setBg(bgFile.value, Number(v))
}
const bgPicking = ref(false)
async function pickBgImage() {
  const p = await openDialog({
    multiple: false,
    directory: false,
    filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'webp', 'bmp', 'gif'] }],
  })
  if (!p || bgPicking.value) return
  bgPicking.value = true
  try {
    const name = await api.setBackgroundImage(p as string)
    setBg(name, bgBlur.value)
    toast(t('settings.bgImageSaved'))
  } catch (e) {
    toast(errorText(e), 'error')
  } finally {
    bgPicking.value = false
  }
}
function clearBgImage() {
  setBg(null)
}

// ---- 播放页专注模式（默认开启、5 秒；App.vue 播放时按此计时隐藏控制条）----
// 开关读写走 useFocusMode 单点（与 App.vue 同一口径）
const focusModeOn = ref(isFocusModeEnabled())
function setFocusMode(v: boolean) {
  focusModeOn.value = v
  setFocusModeEnabled(v)
}
const FOCUS_DELAY_KEY = 'lm.focusDelay'
const focusDelay = ref(Number(localStorage.getItem(FOCUS_DELAY_KEY)) || 5)
const focusDelayOptions = computed<SelectOption[]>(() =>
  [3, 5, 8, 10, 15, 30].map((s) => ({ value: s, label: t('settings.focusDelaySecs', { value: s }) })),
)
function onFocusDelay(v: string | number) {
  focusDelay.value = Number(v)
  localStorage.setItem(FOCUS_DELAY_KEY, String(v))
}

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

// ---- 听歌统计入口开关（与侧栏共享状态，见 useStatsEntry）----
const nav = useNav()
const { statsEnabled, setStatsEnabled: _setStatsEnabled } = useStatsEntry()
function setStatsEntry(v: boolean) {
  _setStatsEnabled(v)
  // 关闭入口时若正停在统计页，跳回全部歌曲（侧栏入口已隐藏，避免停留在不可达页面）
  if (!v && nav.current.value.view === 'stats') nav.go({ view: 'tracks' })
}

/** 重新运行首次启动引导（清 DB 标记后重载，便于回看或演示） */
async function rerunOnboarding() {
  try {
    await api.setSetting('app.onboarded', '0')
  } catch {
    // 写失败时重载后仍进主界面，不影响使用
  }
  location.reload()
}

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

async function toggleScanSubdirs(s: Source, val?: boolean) {
  const next = val ?? !s.scanSubdirs
  if (next === s.scanSubdirs) return
  try {
    await library.setScanSubdirs(s.id, next)
    toast(next ? t('settings.scanSubdirsOn') : t('settings.scanSubdirsOff'))
  } catch (e) {
    toast(errorText(e), 'error')
  }
}

// ---- 跳过目录：扫描时忽略的目录名（与内置 NAS 回收站/系统目录合并生效）----
// 存档沿用逗号分隔字符串（后端 load_skip_dirs 已按逗号/换行拆分），前端以标签数组编辑
const SKIP_DIRS_KEY = 'scan.skipDirs'
/**
 * 内置跳过目录名（来自 Rust `scanner::BUILTIN_SKIP_DIRS`）：这批「关键字」由内置规则始终跳过，
 * 设置页把它们单独标出来，与用户自己追加的标签区分——否则用户容易以为需要手动添加
 */
const builtinSkipDirs = ref<string[]>([])
const skipDirs = ref<string[]>([])
/** 最近一次成功保存的值（null = 还没从 SQLite 读到），用于判断标签变化是否需要落库 */
let skipDirsSaved: string | null = null

/** 把存档字符串拆成标签（去空白、大小写不敏感去重），与 BaseTagInput 的口径一致 */
function parseSkipDirs(raw: string): string[] {
  const seen = new Set<string>()
  const out: string[] = []
  for (const part of raw.split(/[，,；;\n\r]/).map((s) => s.trim()).filter(Boolean)) {
    const key = part.toLowerCase()
    if (seen.has(key)) continue
    seen.add(key)
    out.push(part)
  }
  return out
}

watch(skipDirs, (tags) => {
  const joined = tags.join(',')
  // 存档还没读到 / 与已保存值一致：不落库（读到存档后的首次赋值也会走到这里，属于空操作）
  if (skipDirsSaved === null || joined === skipDirsSaved) return
  void (async () => {
    try {
      await api.setSetting(SKIP_DIRS_KEY, joined)
      skipDirsSaved = joined
      toast(t('settings.skipDirsSaved'))
    } catch (e) {
      toast(errorText(e), 'error')
    }
  })()
})

api
  .getSetting(SKIP_DIRS_KEY)
  .then((v) => {
    skipDirsSaved = v ?? ''
    skipDirs.value = parseSkipDirs(v ?? '')
  })
  .catch(() => {
    skipDirsSaved = ''
  })

api
  .getBuiltinSkipDirs()
  .then((v) => {
    builtinSkipDirs.value = v ?? []
  })
  .catch(() => {
    /* 取不到就不显示内置标记，不影响其它功能 */
  })

// ---- 已移除歌曲记录（从曲库移除 / 扫描消失，留底便于找回；列表在弹出窗中查看）----
const removedTracks = ref<RemovedTrack[]>([])
const removedTracksLoaded = ref(false)
const removedTracksOpen = ref(false)

function openRemovedTracks() {
  removedTracksOpen.value = true
  if (!removedTracksLoaded.value) void loadRemovedTracks()
}
async function loadRemovedTracks() {
  try {
    removedTracks.value = await api.listRemovedTracks()
    removedTracksLoaded.value = true
  } catch (e) {
    toast(errorText(e), 'error')
  }
}
function removedReasonLabel(reason: string) {
  return reason === 'scan' ? t('settings.removedTracksScan') : t('settings.removedTracksManual')
}
/** 还原：确认文件仍在后触发来源增量扫描，重新入库并删除记录 */
const restoringRemoved = ref(false)
async function restoreRemoved(ids: number[]) {
  if (!ids.length || restoringRemoved.value) return
  restoringRemoved.value = true
  try {
    const r = await api.restoreRemovedTracks(ids)
    if (r.restored > 0) {
      removedTracks.value = removedTracks.value.filter((x) => !ids.includes(x.id))
      toast(t('settings.removedRestoreStarted', { count: r.restored }))
    }
    if (r.missing > 0) {
      toast(t('settings.removedRestoreMissing', { count: r.missing }), r.restored > 0 ? 'info' : 'error')
    }
  } catch (e) {
    toast(errorText(e), 'error')
  } finally {
    restoringRemoved.value = false
  }
}
async function clearRemovedTracks() {
  const ok = await confirmDialog({
    title: t('settings.removedTracksClearTitle'),
    message: t('settings.removedTracksClearMessage'),
    danger: true,
    confirmText: t('common.delete'),
  })
  if (!ok) return
  try {
    await api.clearRemovedTracks()
    removedTracks.value = []
    toast(t('settings.removedTracksCleared'))
  } catch (e) {
    toast(errorText(e), 'error')
  }
}

function fmtTime(t2: number | null) {
  if (!t2) return t('settings.neverScanned')
  return new Date(t2 * 1000).toLocaleString()
}

const scannedSourceIds = computed(() => new Set(Object.keys(library.scanProgress).map(Number)))

/** 已添加过 WebDAV 来源时始终展示云端限制说明；展开添加表单时也展示（便于添加前先了解） */
const showWebdavLimits = computed(() => showWebdav.value || library.sources.some((s) => s.kind === 'webdav'))
</script>

<template>
  <div ref="root" class="flex h-full min-h-0 select-none flex-col">
    <!-- 顶部：标题 + 横向分类目录（原左侧竖排目录，改顶部后内容区通栏更宽敞） -->
    <header
      class="flex shrink-0 items-center justify-between gap-4 border-b border-zinc-100 py-3 pl-6 pr-4 dark:border-zinc-800"
    >
      <h1 data-stagger class="shrink-0 text-lg font-bold text-zinc-900 dark:text-zinc-50">
        {{ t('settings.preferences') }}
      </h1>
      <nav
        class="flex min-w-0 items-center gap-1 overflow-x-auto rounded-xl bg-zinc-100/70 p-1 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden dark:bg-zinc-800/60"
        :aria-label="t('settings.title')"
      >
        <button
          v-for="c in categories"
          :key="c.id"
          type="button"
          class="flex shrink-0 cursor-pointer items-center gap-2 rounded-lg px-3 py-1.5 text-left text-sm transition-colors"
          :class="
            active === c.id
              ? 'bg-white font-medium text-violet-600 shadow-sm dark:bg-zinc-700 dark:text-violet-300'
              : 'text-zinc-500 hover:bg-white/60 hover:text-zinc-800 dark:text-zinc-400 dark:hover:bg-zinc-700/60 dark:hover:text-zinc-100'
          "
          :aria-current="active === c.id ? 'page' : undefined"
          @click="scrollToSection(c.id)"
        >
          <!-- 选中分类的图标用 bold 变体，与侧栏选中态一致 -->
          <component :is="active === c.id ? c.iconActive : c.icon" class="h-4 w-4 shrink-0" />
          <span class="whitespace-nowrap">{{ c.label }}</span>
        </button>
      </nav>
    </header>

    <!-- 全部分区纵向铺开，随滚动浏览；顶部目录点击跳转 -->
    <div ref="contentEl" class="min-h-0 flex-1 overflow-y-auto px-6 pt-5 pb-8" @scroll.passive="onContentScroll">
      <div class="mx-auto max-w-3xl">
      <!-- ===== 音乐库 ===== -->
        <section :id="`settings-section-library`" data-settings-section="library" class="scroll-mt-4">
          <header data-stagger class="mb-5 flex items-center gap-3.5">
            <div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-violet-50 text-violet-500 dark:bg-violet-500/10 dark:text-violet-300">
              <HardDrive class="h-5 w-5" />
            </div>
            <div class="min-w-0">
              <h2 class="text-lg font-semibold text-zinc-900 dark:text-zinc-50">{{ t('settings.library') }}</h2>
              <p class="mt-0.5 text-xs text-zinc-400">{{ t('settings.libraryDesc') }}</p>
            </div>
          </header>
          <div class="space-y-6">
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
                  class="rounded-2xl shadow-sm hover-accent-border border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900"
                >
                  <!-- 图标跨「名称+地址」两行垂直居中；内容列：行1 名称+开关+按钮，行2 地址+曲目/时间 -->
                  <div class="flex items-center gap-3">
                    <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-zinc-100 text-zinc-500 dark:bg-zinc-800 dark:text-zinc-400">
                      <component :is="s.kind === 'webdav' ? Globe : HardDrive" class="h-4.5 w-4.5" />
                    </div>
                    <div class="min-w-0 flex-1">
                      <div class="flex items-center gap-3">
                        <p class="min-w-0 flex-1 truncate text-sm font-medium text-zinc-800 dark:text-zinc-100">{{ s.name }}</p>
                        <div
                          class="flex shrink-0 items-center gap-1.5 text-xs text-zinc-500"
                          v-tooltip="t('settings.quickImportTip')"
                        >
                          {{ t('settings.quickImport') }}
                          <BaseSwitch
                            :model-value="s.fastImport"
                            size="sm"
                            @update:model-value="(v) => toggleFastImport(s, v)"
                          />
                        </div>
                        <div
                          class="flex shrink-0 items-center gap-1.5 text-xs text-zinc-500"
                          v-tooltip="t('settings.scanSubdirsTip')"
                        >
                          {{ t('settings.scanSubdirs') }}
                          <BaseSwitch
                            :model-value="s.scanSubdirs"
                            size="sm"
                            @update:model-value="(v) => toggleScanSubdirs(s, v)"
                          />
                        </div>
                        <div class="flex shrink-0 items-center gap-1">
                          <BaseButton
                            variant="ghost"
                            size="xs"
                            :disabled="scannedSourceIds.has(s.id)"
                            v-tooltip="t('settings.fullParseTip')"
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
                            v-tooltip="t('settings.incrementalScan')"
                            :aria-label="t('settings.incrementalScan')"
                            @click="rescan(s.id)"
                          />
                          <BaseButton
                            variant="ghost"
                            tone="danger"
                            size="xs"
                            :icon="Trash2"
                            v-tooltip="t('common.remove')"
                            :aria-label="t('common.remove')"
                            @click="remove(s)"
                          />
                        </div>
                      </div>
                      <div class="mt-1.5 flex items-center gap-3">
                        <p class="min-w-0 flex-1 truncate text-xs text-zinc-500" v-tooltip="s.basePath ?? s.baseUrl ?? ''">
                          {{ s.basePath ?? s.baseUrl }}
                        </p>
                        <span class="shrink-0 text-xs text-zinc-400">
                          {{ t('settings.sourceTrackCount', { count: s.trackCount }) }} · {{ fmtTime(s.lastScanAt) }}
                        </span>
                      </div>
                    </div>
                  </div>
                  <!-- WebDAV 来源的固有限制，直接写在卡片里（不藏在 tooltip） -->
                  <p
                    v-if="s.kind === 'webdav'"
                    class="mt-2.5 flex items-start gap-1.5 border-t border-zinc-100 pt-2.5 text-xs leading-relaxed text-zinc-400 dark:border-zinc-800"
                  >
                    <InfoCircle class="mt-px h-3.5 w-3.5 shrink-0" />
                    <span>{{ t('settings.webdavCardHint') }}</span>
                  </p>
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

                <p v-if="!library.sources.length" class="rounded-2xl border border-dashed border-zinc-300 p-4 text-center text-sm text-zinc-400 dark:border-zinc-700">
                  {{ t('settings.noSources') }}
                </p>
              </div>

              <!-- 跳过目录：NAS 回收站 / 系统目录内置跳过，可按目录名追加；修改后重新扫描生效 -->
              <!-- mt-6：与各区块间 space-y-6 同距，避免卡片贴着来源列表与相邻卡片 -->
              <div class="mt-6 rounded-2xl hover-accent-border border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900">
                <p class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ t('settings.skipDirs') }}</p>
                <p class="mt-1 text-xs leading-relaxed text-zinc-400">{{ t('settings.skipDirsHint') }}</p>
                <!-- 内置关键字标记：等宽字体 + 浅底做成「标记」样式，并加「内置」徽标与说明气泡，
                     让用户一眼看出这批目录始终跳过、不必自己添加 -->
                <div v-if="builtinSkipDirs.length" class="mt-2.5 flex flex-wrap items-center gap-1.5">
                  <span
                    class="rounded-md bg-violet-50 px-1.5 py-0.5 text-[11px] font-medium text-violet-600 dark:bg-violet-500/15 dark:text-violet-300"
                    v-tooltip="t('settings.skipDirsBuiltinHint')"
                  >
                    {{ t('settings.skipDirsBuiltinTag') }}
                  </span>
                  <code
                    v-for="d in builtinSkipDirs"
                    :key="d"
                    class="rounded-md border border-zinc-200 bg-zinc-50 px-1.5 py-0.5 font-mono text-[11px] text-zinc-500 dark:border-zinc-700 dark:bg-zinc-800/60 dark:text-zinc-400"
                  >{{ d }}</code>
                </div>
                <BaseTagInput
                  v-model="skipDirs"
                  class="mt-3"
                  :placeholder="t('settings.skipDirsPlaceholder')"
                />

                <!-- 已移除歌曲：手动移除 / 扫描消失的记录（仅曲目信息，便于找回文件位置）；数量多，列表收在弹出窗里。
                     与跳过目录同卡：两节都是「库维护」性质，合并减少零碎小卡 -->
                <div class="mt-4 flex flex-wrap items-center justify-between gap-2 border-t border-zinc-100 pt-4 dark:border-zinc-800">
                  <p class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">
                    {{ t('settings.removedTracks') }}
                    <span v-if="removedTracksLoaded && removedTracks.length" class="ml-1 text-xs font-normal text-zinc-400">
                      {{ removedTracks.length }}
                    </span>
                  </p>
                  <BaseButton variant="secondary" size="xs" @click="openRemovedTracks">
                    {{ t('settings.removedTracksView') }}
                  </BaseButton>
                </div>
                <p class="mt-1 text-xs leading-relaxed text-zinc-400">{{ t('settings.removedTracksHint') }}</p>

                <BaseModal
                  :open="removedTracksOpen"
                  size="lg"
                  :title="`${t('settings.removedTracks')}${removedTracks.length ? ` (${removedTracks.length})` : ''}`"
                  @close="removedTracksOpen = false"
                >
                <ul v-if="removedTracks.length" class="max-h-[60vh] space-y-0.5 overflow-y-auto pr-1">
                  <li
                    v-for="r in removedTracks"
                    :key="r.id"
                    class="group flex min-w-0 items-center gap-2 rounded-lg px-2 py-1.5 text-xs hover:bg-zinc-100/70 dark:hover:bg-zinc-800/50"
                    v-tooltip="r.path"
                  >
                    <span class="min-w-0 flex-1 truncate text-zinc-700 dark:text-zinc-200">{{ r.title }}</span>
                    <span class="hidden w-36 shrink-0 truncate text-zinc-400 sm:block">{{ r.artist ?? $t('artist.unknownArtist') }}</span>
                    <span
                      class="shrink-0 rounded-full px-1.5 py-px text-[10px] font-medium"
                      :class="
                        r.reason === 'scan'
                          ? 'bg-amber-100 text-amber-600 dark:bg-amber-500/15 dark:text-amber-300'
                          : 'bg-zinc-200 text-zinc-500 dark:bg-zinc-700 dark:text-zinc-300'
                      "
                    >
                      {{ removedReasonLabel(r.reason) }}
                    </span>
                    <span class="shrink-0 tabular-nums text-zinc-400">
                      {{ new Date(r.removedAt * 1000).toLocaleString() }}
                    </span>
                    <button
                      class="shrink-0 cursor-pointer rounded-md p-1 text-zinc-400 opacity-0 transition hover:bg-zinc-200/70 hover:text-violet-500 group-hover:opacity-100 disabled:cursor-default disabled:opacity-30 dark:hover:bg-zinc-700"
                      :disabled="restoringRemoved"
                      v-tooltip="$t('settings.removedRestore')"
                      :aria-label="$t('settings.removedRestore')"
                      @click="restoreRemoved([r.id])"
                    >
                      <RotateCcw class="h-3.5 w-3.5" />
                    </button>
                  </li>
                </ul>
                <p v-else class="py-2 text-xs text-zinc-400">{{ t('settings.removedTracksEmpty') }}</p>
                <template #footer>
                  <BaseButton
                    variant="secondary"
                    size="sm"
                    :disabled="!removedTracks.length || restoringRemoved"
                    @click="restoreRemoved(removedTracks.map((r) => r.id))"
                  >
                    {{ t('settings.removedRestoreAll') }}
                  </BaseButton>
                  <BaseButton
                    variant="ghost"
                    tone="danger"
                    size="sm"
                    :disabled="!removedTracks.length"
                    @click="clearRemovedTracks"
                  >
                    {{ t('settings.removedTracksClear') }}
                  </BaseButton>
                    <BaseButton variant="secondary" size="sm" @click="removedTracksOpen = false">
                      {{ t('common.close') }}
                    </BaseButton>
                  </template>
                </BaseModal>
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
                class="space-y-3 rounded-2xl hover-accent-border border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900"
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

              <!-- 云端（WebDAV）来源的已知限制：常驻展示，避免「为什么云端新歌不出现 / 没有 MV / 缺时长」被当成 bug -->
              <div
                v-if="showWebdavLimits"
                class="mt-2.5 rounded-2xl border border-zinc-200 bg-zinc-50/70 p-4 dark:border-zinc-800 dark:bg-zinc-900/50"
              >
                <p class="mb-2 flex items-center gap-1.5 text-xs font-medium text-zinc-600 dark:text-zinc-300">
                  <InfoCircle class="h-3.5 w-3.5 shrink-0" />
                  {{ t('settings.webdavLimitsTitle') }}
                </p>
                <ul class="space-y-1.5 text-xs leading-relaxed text-zinc-500">
                  <li>{{ t('settings.webdavLimitNoWatch') }}</li>
                  <li>{{ t('settings.webdavLimitNoMv') }}</li>
                  <li>{{ t('settings.webdavLimitHeadOnly') }}</li>
                  <li>{{ t('settings.webdavLimitPartial') }}</li>
                  <li>{{ t('settings.webdavLimitRateLimit') }}</li>
                </ul>
              </div>
            </section>

            <!-- 多艺人分隔符 -->
            <section>
              <h3 class="mb-2.5 text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ t('settings.artistSeparators') }}</h3>
              <div class="rounded-2xl shadow-sm hover-accent-border border border-zinc-200 bg-white p-4 text-sm dark:border-zinc-800 dark:bg-zinc-900">
                <div class="flex flex-wrap items-center justify-between gap-3">
                  <div class="flex w-full flex-wrap justify-end gap-2">
                    <BaseButton
                      v-for="sep in SEPARATOR_CANDIDATES"
                      :key="sep"
                      size="sm"
                      :variant="artistSeparators.has(sep) ? 'primary' : 'secondary'"
                      v-tooltip="sep === FIXED_SEPARATOR ? t('settings.separatorFixedTip') : t('settings.separatorToggleTip', { sep })"
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
                        <span class="text-zinc-700 dark:text-zinc-200">{{ c.newArtists.join(' / ') }}</span>
                      </li>
                    </ul>
                    <p v-if="splitChanges.length > 200" class="mt-1 text-xs text-zinc-400">
                      {{ t('settings.separatorMore', { count: splitChanges.length - 200 }) }}
                    </p>
                  </div>
                </template>
              </div>
            </section>

            <!-- 艺人名规整：合并同义艺人（如「陈奕迅（Eason Chan）」→「陈奕迅」）-->
            <section>
              <h3 class="mb-2.5 text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ t('settings.artistNormalize') }}</h3>
              <div class="rounded-2xl shadow-sm hover-accent-border border border-zinc-200 bg-white p-4 text-sm dark:border-zinc-800 dark:bg-zinc-900">
                <div class="flex flex-wrap items-center justify-between gap-3">
                  <p class="max-w-md text-xs leading-relaxed text-zinc-400">{{ t('settings.artistNormalizeDesc') }}</p>
                  <BaseButton
                    size="sm"
                    variant="secondary"
                    :loading="normalizeApplying"
                    :disabled="normalizeApplying"
                    :icon="normalizeApplying ? undefined : Check"
                    @click="onNormalizeArtists"
                  >
                    {{ normalizeApplying ? t('settings.artistNormalizeApplying') : t('settings.artistNormalizeBtn') }}
                  </BaseButton>
                </div>

                <!-- 变更报告：执行后展示被合并的艺人变化 -->
                <template v-if="normalizeChanges !== null">
                  <div class="mt-3 border-t border-zinc-100 pt-3 dark:border-zinc-800">
                    <p class="text-xs font-medium" :class="normalizeChanges.length > 0 ? 'text-violet-500' : 'text-zinc-400'">
                      {{
                        normalizeChanges.length > 0
                          ? t('settings.artistNormalizeApplied', { count: normalizeChanges.length })
                          : t('settings.artistNormalizeNone')
                      }}
                    </p>
                    <ul v-if="normalizeChanges.length > 0" class="mt-2 max-h-56 space-y-1.5 overflow-y-auto pr-1">
                      <li v-for="c in normalizeChanges.slice(0, 200)" :key="c.oldName" class="flex min-w-0 flex-wrap items-baseline gap-x-1 text-xs">
                        <span class="text-zinc-400 line-through">{{ c.oldName }}</span>
                        <span class="text-violet-500">→</span>
                        <span class="font-medium text-zinc-700 dark:text-zinc-200">{{ c.newName }}</span>
                        <span class="text-zinc-400">({{ c.trackCount }})</span>
                      </li>
                    </ul>
                    <p v-if="normalizeChanges.length > 200" class="mt-1 text-xs text-zinc-400">
                      {{ t('settings.artistNormalizeMore', { count: normalizeChanges.length - 200 }) }}
                    </p>
                  </div>
                </template>

                <!-- 已合并名单：历次规整与自定义合并的记录（扫描遇到旧名仍归到主艺人名下）。
                     点「管理」打开弹窗，可逐条取消合并（按合并历史拆回曲目/专辑归属）。 -->
                <div class="mt-3 border-t border-zinc-100 pt-3 dark:border-zinc-800">
                  <div class="flex items-center justify-between gap-2">
                    <p class="text-xs font-medium text-zinc-500">
                      {{ t('settings.artistMergedTitle', { count: artistAliases.length }) }}
                    </p>
                    <BaseButton
                      v-if="artistAliases.length"
                      variant="ghost"
                      size="xs"
                      @click="mergedDialogOpen = true"
                    >
                      {{ t('settings.artistMergedManage') }}
                    </BaseButton>
                  </div>
                  <p v-if="!artistAliases.length" class="mt-1.5 text-xs text-zinc-400">
                    {{ t('settings.artistMergedEmpty') }}
                  </p>
                  <ul v-else class="mt-2 max-h-56 space-y-1.5 overflow-y-auto pr-1">
                    <li
                      v-for="a in artistAliases.slice(0, 200)"
                      :key="`${a.alias}-${a.artistId}`"
                      class="flex min-w-0 flex-wrap items-baseline gap-x-1 text-xs"
                    >
                      <span class="text-zinc-400 line-through">{{ a.alias }}</span>
                      <span class="text-violet-500">→</span>
                      <span class="font-medium text-zinc-700 dark:text-zinc-200">{{ a.artistName }}</span>
                    </li>
                  </ul>
                  <p v-if="artistAliases.length > 200" class="mt-1 text-xs text-zinc-400">
                    {{ t('settings.artistNormalizeMore', { count: artistAliases.length - 200 }) }}
                  </p>
                </div>

                <!-- 自定义合并：a 与 b 实为同一人（改名 / 写法不同）时手动归并 -->
                <div class="mt-3 border-t border-zinc-100 pt-3 dark:border-zinc-800">
                  <p class="text-xs font-medium text-zinc-500">{{ t('settings.artistMergeCustom') }}</p>
                  <p class="mt-1 text-xs leading-relaxed text-zinc-400">{{ t('settings.artistMergeHint') }}</p>
                  <div class="mt-2.5 grid items-center gap-2 sm:grid-cols-[1fr_1fr_auto]">
                    <BaseSelect
                      v-model="mergeKeep"
                      :options="artistOptions"
                      :placeholder="t('settings.artistMergeKeep')"
                      size="sm"
                      searchable
                    />
                    <BaseSelect
                      v-model="mergeAbsorb"
                      :options="artistOptions"
                      :placeholder="t('settings.artistMergeAbsorb')"
                      size="sm"
                      searchable
                    />
                    <BaseButton
                      size="sm"
                      variant="secondary"
                      :loading="merging"
                      :disabled="merging || artistOptionsLoading || mergeKeep === '' || mergeAbsorb === '' || mergeKeep === mergeAbsorb"
                      @click="onMergeArtist"
                    >
                      {{ t('settings.artistMergeAction') }}
                    </BaseButton>
                  </div>
                </div>
              </div>
            </section>
          </div>
        </section>

      <!-- ===== 外观 ===== -->

        <section :id="`settings-section-appearance`" data-settings-section="appearance" class="mt-14 scroll-mt-4">
          <header data-stagger class="mb-5 flex items-center gap-3.5">
            <div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-violet-50 text-violet-500 dark:bg-violet-500/10 dark:text-violet-300">
              <Palette class="h-5 w-5" />
            </div>
            <div class="min-w-0">
              <h2 class="text-lg font-semibold text-zinc-900 dark:text-zinc-50">{{ t('settings.appearance') }}</h2>
              <p class="mt-0.5 text-xs text-zinc-400">{{ t('settings.appearanceDesc') }}</p>
            </div>
          </header>
          <div class="space-y-6">
            <section>
              <div class="space-y-4 rounded-2xl hover-accent-border border border-zinc-200 bg-white p-4 text-sm dark:border-zinc-800 dark:bg-zinc-900">
                <div class="flex items-center justify-between gap-3">
                  <span class="text-zinc-700 dark:text-zinc-200">{{ t('settings.theme') }}</span>
                  <BaseButtonGroup :model-value="mode" :items="themeItems" size="sm" @update:model-value="onThemeChange" />
                </div>
                <!-- 主题色：Ant Design 色板预设，覆盖 --color-violet-* 变量全局换肤（见 useThemeColor.ts） -->
                <div class="flex items-center justify-between gap-3">
                  <span class="shrink-0 text-zinc-700 dark:text-zinc-200">{{ t('settings.themeColor') }}</span>
                  <div class="flex flex-wrap items-center justify-end gap-1.5">
                    <button
                      v-for="p in presets"
                      :key="p.key"
                      class="h-5 w-5 cursor-pointer rounded-full transition-transform hover:scale-110"
                      :class="themeColor === p.key ? 'ring-2 ring-zinc-500 ring-offset-2 ring-offset-white dark:ring-zinc-300 dark:ring-offset-zinc-900' : ''"
                      :style="{ background: p.scale[5] }"
                      v-tooltip="t(p.nameKey)"
                      :aria-label="t(p.nameKey)"
                      @click="setThemeColor(p.key)"
                    ></button>
                    <button
                      class="flex h-5 w-5 cursor-pointer items-center justify-center rounded-full bg-gradient-to-br from-violet-400 to-violet-600 transition-transform hover:scale-110"
                      :class="themeColor === null ? 'ring-2 ring-zinc-500 ring-offset-2 ring-offset-white dark:ring-zinc-300 dark:ring-offset-zinc-900' : ''"
                      v-tooltip="t('settings.themeColorDefault')"
                      :aria-label="t('settings.themeColorDefault')"
                      @click="setThemeColor(null)"
                    >
                      <Check v-if="themeColor === null" class="h-3 w-3 text-white" />
                    </button>
                  </div>
                </div>
                <!-- 全局字体：应用于整个软件（含桌面歌词），从系统读取；系统字体多，开启搜索过滤 -->
                <div class="flex items-center justify-between gap-3 border-t border-zinc-100 pt-4 dark:border-zinc-800">
                  <span class="text-zinc-700 dark:text-zinc-200">{{ t('settings.fontFamily') }}</span>
                  <div class="max-w-[280px] flex-1">
                    <BaseSelect
                      :model-value="appFont"
                      :options="fontSelectOptions"
                      size="sm"
                      searchable
                      @update:model-value="onFontChange"
                    />
                  </div>
                </div>
                <!-- 弹窗可拖动：按住标题栏拖动位置 -->
                <div class="flex items-center justify-between gap-3 border-t border-zinc-100 pt-4 dark:border-zinc-800">
                  <div class="min-w-0">
                    <p class="text-zinc-700 dark:text-zinc-200">{{ t('settings.dialogDrag') }}</p>
                    <p class="mt-0.5 text-xs text-zinc-400">{{ t('settings.dialogDragHint') }}</p>
                  </div>
                  <BaseSwitch :model-value="dialogDragOn" size="sm" @update:model-value="setDialogDrag" />
                </div>
                <!-- 弹窗背景模糊度 -->
                <div class="flex items-center justify-between gap-3 border-t border-zinc-100 pt-4 dark:border-zinc-800">
                  <span class="text-zinc-700 dark:text-zinc-200">{{ t('settings.dialogBlur') }}</span>
                  <div class="w-28">
                    <BaseSelect
                      :model-value="dialogBlurLevel"
                      :options="dialogBlurOptions"
                      size="sm"
                      @update:model-value="onDialogBlur"
                    />
                  </div>
                </div>
                <!-- 自定义背景：本机图片复制到应用数据目录（bg:// 协议），铺满底色区，卡片浮于其上 -->
                <div class="flex items-center justify-between gap-3 border-t border-zinc-100 pt-4 dark:border-zinc-800">
                  <div class="min-w-0">
                    <p class="text-zinc-700 dark:text-zinc-200">{{ t('settings.bgImage') }}</p>
                    <p class="mt-0.5 text-xs text-zinc-400">{{ t('settings.bgImageHint') }}</p>
                  </div>
                  <div class="flex shrink-0 items-center gap-2">
                    <BaseButton
                      size="sm"
                      :icon="bgPicking ? LoaderCircle : ImageIcon"
                      :loading="bgPicking"
                      :disabled="bgPicking"
                      @click="pickBgImage"
                    >
                      {{ t('settings.bgImagePick') }}
                    </BaseButton>
                    <BaseButton v-if="bgFile" variant="ghost" size="sm" @click="clearBgImage">
                      {{ t('settings.bgImageClear') }}
                    </BaseButton>
                  </div>
                </div>
                <div v-if="bgFile" class="flex items-center justify-between gap-3 border-t border-zinc-100 pt-4 dark:border-zinc-800">
                  <span class="text-zinc-700 dark:text-zinc-200">{{ t('settings.bgBlur') }}</span>
                  <div class="w-28">
                    <BaseSelect
                      :model-value="String(bgBlur)"
                      :options="bgBlurOptions"
                      size="sm"
                      @update:model-value="onBgBlur"
                    />
                  </div>
                </div>
              </div>
            </section>
          </div>
        </section>

      <!-- ===== 播放 ===== -->

        <section :id="`settings-section-playback`" data-settings-section="playback" class="mt-14 scroll-mt-4">
          <header data-stagger class="mb-5 flex items-center gap-3.5">
            <div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-violet-50 text-violet-500 dark:bg-violet-500/10 dark:text-violet-300">
              <Play class="h-5 w-5" />
            </div>
            <div class="min-w-0">
              <h2 class="text-lg font-semibold text-zinc-900 dark:text-zinc-50">{{ t('settings.playback') }}</h2>
              <p class="mt-0.5 text-xs text-zinc-400">{{ t('settings.playbackDesc') }}</p>
            </div>
          </header>
          <div class="space-y-6">
            <section>
              <div class="space-y-4 rounded-2xl hover-accent-border border border-zinc-200 bg-white p-4 text-sm dark:border-zinc-800 dark:bg-zinc-900">
                <!-- 播放页专注模式：播放中鼠标停顿自动隐藏顶栏与播放条 -->
                <div>
                  <div class="flex items-center justify-between gap-3">
                    <div class="min-w-0">
                      <span class="text-zinc-700 dark:text-zinc-200">{{ t('settings.focusMode') }}</span>
                      <p class="mt-0.5 text-xs leading-relaxed text-zinc-400">{{ t('settings.focusModeHint') }}</p>
                    </div>
                    <BaseSwitch :model-value="focusModeOn" size="sm" @update:model-value="setFocusMode" />
                  </div>
                  <div v-if="focusModeOn" class="mt-3 flex items-center justify-between gap-3 border-t border-zinc-100 pt-3 dark:border-zinc-800">
                    <span class="text-zinc-700 dark:text-zinc-200">{{ t('settings.focusDelay') }}</span>
                    <div class="w-28">
                      <BaseSelect
                        :model-value="focusDelay"
                        :options="focusDelayOptions"
                        size="sm"
                        @update:model-value="onFocusDelay"
                      />
                    </div>
                  </div>
                </div>
                <!-- 淡入淡出 -->
                <div class="border-t border-zinc-100 pt-4 dark:border-zinc-800">
                  <div class="flex items-center justify-between gap-3">
                    <span class="text-zinc-700 dark:text-zinc-200">{{ t('settings.fadeInOut') }}</span>
                    <BaseSwitch
                      :model-value="fadeOn"
                      size="sm"
                      v-tooltip="t('settings.fadeInOutTip')"
                      @update:model-value="onFadeToggle"
                    />
                  </div>
                  <p class="mt-1.5 text-xs leading-relaxed text-zinc-400">{{ t('settings.fadeInOutDesc') }}</p>
                </div>

                <!-- 阻止系统休眠 -->
                <div class="border-t border-zinc-100 pt-4 dark:border-zinc-800">
                  <div class="flex items-center justify-between gap-3">
                    <span class="text-zinc-700 dark:text-zinc-200">{{ t('settings.preventSleep') }}</span>
                    <BaseSwitch
                      :model-value="preventSleepOn"
                      size="sm"
                      v-tooltip="t('settings.preventSleepTip')"
                      @update:model-value="onPreventSleepToggle"
                    />
                  </div>
                  <p class="mt-1.5 text-xs leading-relaxed text-zinc-400">{{ t('settings.preventSleepDesc') }}</p>
                </div>
              </div>
            </section>
          </div>
        </section>

        <!-- 音频：均衡器 / 音量归一化 / 睡眠定时器 / 系统媒体键 -->
        <section
          :id="`settings-section-audio`"
          data-settings-section="audio"
          class="mt-14 scroll-mt-4"
        >
          <header data-stagger class="mb-5 flex items-center gap-3.5">
            <div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-violet-50 text-violet-500 dark:bg-violet-500/10 dark:text-violet-300">
              <MusicNote class="h-5 w-5" />
            </div>
            <div class="min-w-0">
              <h2 class="text-lg font-semibold text-zinc-900 dark:text-zinc-50">{{ t('settings.audio') }}</h2>
              <p class="mt-0.5 text-xs text-zinc-400">{{ t('settings.audioDesc') }}</p>
            </div>
          </header>
          <AudioEffectsPanel />
        </section>

      <!-- ===== 搜索 ===== -->

        <section :id="`settings-section-search`" data-settings-section="search" class="mt-14 scroll-mt-4">
          <header data-stagger class="mb-5 flex items-center gap-3.5">
            <div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-violet-50 text-violet-500 dark:bg-violet-500/10 dark:text-violet-300">
              <Search class="h-5 w-5" />
            </div>
            <div class="min-w-0">
              <h2 class="text-lg font-semibold text-zinc-900 dark:text-zinc-50">{{ t('settings.search') }}</h2>
              <p class="mt-0.5 text-xs text-zinc-400">{{ t('settings.searchDesc') }}</p>
            </div>
          </header>
          <div class="space-y-6">
            <section>
              <div class="space-y-4 rounded-2xl hover-accent-border border border-zinc-200 bg-white p-4 text-sm dark:border-zinc-800 dark:bg-zinc-900">
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
                    <span class="text-zinc-700 dark:text-zinc-200">{{ t('settings.pinyinSearch') }}</span>
                    <BaseSwitch
                      :model-value="searchSettings.pinyin"
                      size="sm"
                      v-tooltip="t('settings.pinyinTip')"
                      @update:model-value="togglePinyin"
                    />
                  </div>
                  <p class="mt-1.5 text-xs leading-relaxed text-zinc-400">{{ t('settings.pinyinDesc') }}</p>
                </div>

                <!-- 排序偏好 -->
                <div class="border-t border-zinc-100 pt-4 dark:border-zinc-800">
                  <div class="flex flex-wrap items-center justify-between gap-3">
                    <span class="text-zinc-700 dark:text-zinc-200">{{ t('settings.sortPreference') }}</span>
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
                    <span class="text-zinc-700 dark:text-zinc-200">{{ t('settings.inputDebounce') }}</span>
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
          </div>
        </section>

      <!-- ===== 歌词 ===== -->

        <section :id="`settings-section-lyrics`" data-settings-section="lyrics" class="mt-14 scroll-mt-4">
          <header data-stagger class="mb-5 flex items-center gap-3.5">
            <div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-violet-50 text-violet-500 dark:bg-violet-500/10 dark:text-violet-300">
              <Subtitles class="h-5 w-5" />
            </div>
            <div class="min-w-0">
              <h2 class="text-lg font-semibold text-zinc-900 dark:text-zinc-50">{{ t('settings.lyrics') }}</h2>
              <p class="mt-0.5 text-xs text-zinc-400">{{ t('settings.lyricsDesc') }}</p>
            </div>
          </header>
          <div class="space-y-6">
            <section>
              <div class="rounded-2xl shadow-sm hover-accent-border border border-zinc-200 bg-white p-4 text-sm dark:border-zinc-800 dark:bg-zinc-900">
                <!-- 歌词来源优先级：并入桌面歌词卡顶部（同属「歌词」，减少零碎小卡） -->
                <div class="flex items-center justify-between gap-3">
                  <div class="min-w-0">
                    <p class="text-zinc-700 dark:text-zinc-200">{{ t('settings.lyricPriority') }}</p>
                    <p class="mt-0.5 text-xs text-zinc-400">{{ t('settings.lyricPriorityDesc') }}</p>
                  </div>
                  <div class="w-64 shrink-0">
                    <BaseSelect
                      :model-value="lyricPriority"
                      :options="lyricPriorityOptions"
                      size="sm"
                      @update:model-value="onLyricPriorityChange"
                    />
                  </div>
                </div>

                <!-- 预览：与浮窗样式一致，随下方设置实时变化 -->
                <div
                  class="mt-4 flex min-h-[76px] flex-col justify-center gap-1 rounded-xl px-5 py-3"
                  :style="previewBoxStyle"
                >
                  <p class="truncate font-bold" :style="previewMainStyle">{{ t('settings.dlPreviewMain') }}</p>
                  <p v-if="dlConfig.showTranslation" class="truncate" :style="previewTransStyle">
                    {{ t('settings.dlPreviewTrans') }}
                  </p>
                  <!-- 显示翻译时浮窗只显示当前句（歌词+翻译），不再预告下一句 -->
                  <p v-if="!dlConfig.showTranslation" class="truncate" :style="previewPendingStyle">{{ t('settings.dlPreviewPending') }}</p>
                </div>

                <!-- 显示 -->
                <div class="mt-5 mb-3 flex items-center gap-2">
                  <span class="text-xs font-medium text-zinc-400">{{ t('settings.dlDisplay') }}</span>
                  <span class="h-px flex-1 bg-zinc-100 dark:bg-zinc-800"></span>
                </div>
                <div class="grid grid-cols-1 gap-x-8 gap-y-4 sm:grid-cols-2">
                  <!-- 开关 -->
                  <div class="flex items-center justify-between gap-3">
                    <span class="text-zinc-700 dark:text-zinc-200">{{ t('settings.dlShow') }}</span>
                    <BaseSwitch
                      :model-value="dlEnabled"
                      size="sm"
                      :label="dlEnabled ? t('desktopLyrics.enabled') : t('desktopLyrics.disabled')"
                      @update:model-value="() => dlToggle()"
                    />
                  </div>
                  <!-- 显示行数 -->
                  <div class="flex items-center justify-between gap-3">
                    <span class="text-zinc-700 dark:text-zinc-200">{{ t('settings.dlLines') }}</span>
                    <BaseButtonGroup
                      :model-value="String(dlConfig.lines)"
                      :items="dlLineItems"
                      size="sm"
                      @update:model-value="onDlLinesChange"
                    />
                  </div>
                  <!-- 显示翻译：有翻译的行在歌词下方显示译文副行（一行歌词、一行翻译） -->
                  <div class="flex items-center justify-between gap-3">
                    <span class="text-zinc-700 dark:text-zinc-200">{{ t('settings.dlTranslation') }}</span>
                    <BaseSwitch
                      v-model="dlConfig.showTranslation"
                      size="sm"
                      :label="t('settings.dlTranslation')"
                    />
                  </div>
                  <!-- 对齐方式（选项较多，独占一行） -->
                  <div class="flex items-center justify-between gap-3 sm:col-span-2">
                    <span class="shrink-0 text-zinc-700 dark:text-zinc-200">{{ t('settings.dlAlign') }}</span>
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
                  <!-- 预设配色（独占一行）：点选后同时更新播放行 / 未播放行颜色 -->
                  <div class="flex items-center justify-between gap-3 sm:col-span-2">
                    <span class="shrink-0 text-zinc-700 dark:text-zinc-200">{{ t('settings.dlPreset') }}</span>
                    <div class="w-28 shrink-0">
                      <BaseSelect :model-value="dlPreset" :options="dlPresetOptions" size="sm" @update:model-value="onDlPresetChange" />
                    </div>
                  </div>
                  <!-- 播放行颜色 -->
                  <div class="flex items-center justify-between gap-3">
                    <span class="text-zinc-700 dark:text-zinc-200">{{ t('settings.dlPlayColor') }}</span>
                    <BaseColorPicker
                      v-model="dlConfig.color"
                      :presets="DL_PLAY_PRESETS"
                      v-tooltip="t('settings.dlPlayColor')"
                    />
                  </div>
                  <!-- 未播放行颜色 -->
                  <div class="flex items-center justify-between gap-3">
                    <span class="text-zinc-700 dark:text-zinc-200">{{ t('settings.dlPendingColor') }}</span>
                    <BaseColorPicker
                      v-model="dlConfig.pendingColor"
                      :presets="DL_PENDING_PRESETS"
                      v-tooltip="t('settings.dlPendingColor')"
                    />
                  </div>
                  <!-- 描边 -->
                  <div class="flex items-center justify-between gap-3">
                    <span class="text-zinc-700 dark:text-zinc-200">{{ t('settings.dlOutline') }}</span>
                    <div class="flex items-center gap-2">
                      <BaseColorPicker
                        v-model="dlConfig.outlineColor"
                        :disabled="!dlConfig.outline"
                        v-tooltip="t('settings.dlOutlineColor')"
                      />
                      <BaseCheckbox v-model="dlConfig.outline" size="sm" />
                    </div>
                  </div>
                  <!-- 字体加粗 -->
                  <div class="flex items-center justify-between gap-3">
                    <span class="text-zinc-700 dark:text-zinc-200">{{ t('settings.dlBold') }}</span>
                    <BaseCheckbox v-model="dlConfig.bold" size="sm" />
                  </div>
                  <!-- 字号（独占一行，滑杆拉满宽度） -->
                  <div class="flex items-center gap-3 sm:col-span-2">
                    <span class="shrink-0 text-zinc-700 dark:text-zinc-200">{{ t('settings.dlFontSize', { size: dlConfig.fontSize }) }}</span>
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
                    <span class="text-zinc-700 dark:text-zinc-200">{{ t('settings.dlBgColor') }}</span>
                    <BaseColorPicker v-model="dlConfig.bgColor" :title="t('settings.dlBgColor')" />
                  </div>
                  <!-- 背景不透明度 -->
                  <div class="flex items-center gap-3">
                    <span class="shrink-0 text-zinc-700 dark:text-zinc-200">
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
          </div>
        </section>

      <!-- ===== 通用 ===== -->

        <section :id="`settings-section-general`" data-settings-section="general" class="mt-14 scroll-mt-4">
          <header data-stagger class="mb-5 flex items-center gap-3.5">
            <div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-violet-50 text-violet-500 dark:bg-violet-500/10 dark:text-violet-300">
              <Settings class="h-5 w-5" />
            </div>
            <div class="min-w-0">
              <h2 class="text-lg font-semibold text-zinc-900 dark:text-zinc-50">{{ t('settings.general') }}</h2>
              <p class="mt-0.5 text-xs text-zinc-400">{{ t('settings.generalDesc') }}</p>
            </div>
          </header>
          <div class="space-y-6">
            <!-- 基础偏好：语言 / 关闭行为 / 听歌统计入口（单行设置合并一卡，列表组风格） -->
            <section>
              <div class="rounded-xl shadow-sm hover-accent-border border border-zinc-200 bg-white p-1.5 text-sm dark:border-zinc-800 dark:bg-zinc-900">
                <div class="flex flex-wrap items-center justify-between gap-3 rounded-lg px-3 py-2.5 transition-colors hover:bg-zinc-50 dark:hover:bg-zinc-800/60">
                  <span class="text-zinc-700 dark:text-zinc-200">{{ t('settings.languageSelect') }}</span>
                  <div class="w-40 shrink-0">
                    <BaseSelect :model-value="locale" :options="languageOptions" size="sm" @update:model-value="onLocaleChange" />
                  </div>
                </div>
                <div class="mt-0.5 flex flex-wrap items-center justify-between gap-3 rounded-lg px-3 py-2.5 transition-colors hover:bg-zinc-50 dark:hover:bg-zinc-800/60">
                  <span class="text-zinc-700 dark:text-zinc-200">{{ t('settings.closeActionDesc') }}</span>
                  <BaseButtonGroup
                    :model-value="closeAction"
                    :items="closeActionItems"
                    size="sm"
                    @update:model-value="setCloseAction"
                  />
                </div>
                <div class="mt-0.5 flex items-center justify-between gap-3 rounded-lg px-3 py-2.5 transition-colors hover:bg-zinc-50 dark:hover:bg-zinc-800/60">
                  <div class="min-w-0">
                    <p class="text-zinc-700 dark:text-zinc-200">{{ t('settings.statsEntry') }}</p>
                    <p class="mt-0.5 text-xs text-zinc-400">{{ t('settings.statsEntryHint') }}</p>
                  </div>
                  <BaseSwitch :model-value="statsEnabled" size="sm" @update:model-value="setStatsEntry" />
                </div>
              </div>
            </section>

            <!-- 快捷键：应用内快捷键（可录制）+ 全局快捷键（默认关闭，见 useShortcuts.ts） -->
            <section>
              <h3 class="mb-2.5 text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ t('settings.shortcuts') }}</h3>
              <div class="rounded-2xl shadow-sm hover-accent-border border border-zinc-200 bg-white p-4 text-sm dark:border-zinc-800 dark:bg-zinc-900">
                <!-- 应用内快捷键 -->
                <div class="flex items-center justify-between gap-3">
                  <div class="min-w-0">
                    <p class="text-zinc-700 dark:text-zinc-200">{{ t('settings.shortcutsInApp') }}</p>
                    <p class="mt-0.5 text-xs text-zinc-400">{{ t('settings.shortcutsInAppDesc') }}</p>
                  </div>
                  <BaseButton variant="ghost" size="sm" :icon="RotateCcw" @click="onResetShortcuts">
                    {{ t('settings.shortcutReset') }}
                  </BaseButton>
                </div>
                <div class="mt-2 space-y-0.5">
                  <div
                    v-for="def in appRows"
                    :key="def.action"
                    class="flex items-center justify-between gap-3 rounded-lg px-2 py-1.5 transition-colors hover:bg-zinc-50 dark:hover:bg-zinc-800/60"
                  >
                    <span class="min-w-0 truncate text-zinc-500 dark:text-zinc-400">{{ t(def.labelKey) }}</span>
                    <button
                      type="button"
                      class="shrink-0 rounded-md border px-2.5 py-1 font-mono text-xs transition-colors"
                      :class="
                        isRecording('app', def.action)
                          ? 'animate-pulse border-violet-300 bg-violet-50 text-violet-600 dark:border-violet-500/40 dark:bg-violet-500/15 dark:text-violet-300'
                          : 'hover-accent-border border-zinc-200 bg-zinc-50 text-zinc-600 hover:text-violet-600 dark:border-zinc-700 dark:bg-zinc-800/60 dark:text-zinc-300 dark:hover:text-violet-300'
                      "
                      v-tooltip="isRecording('app', def.action) ? t('settings.shortcutRecordHint') : ''"
                      @click="startRecording('app', def.action)"
                      @blur="onRecordBlur"
                    >
                      <template v-if="appShortcuts[def.action]">{{ formatCombo(appShortcuts[def.action]!) }}</template>
                      <template v-else>{{ t('settings.shortcutNone') }}</template>
                    </button>
                  </div>
                </div>

                <!-- 全局快捷键：系统级热键，默认关闭 -->
                <div class="mt-5 border-t border-zinc-100 pt-4 dark:border-zinc-800">
                  <div class="flex items-center justify-between gap-3">
                    <div class="min-w-0">
                      <p class="text-zinc-700 dark:text-zinc-200">{{ t('settings.shortcutsGlobal') }}</p>
                      <p class="mt-0.5 text-xs text-zinc-400">{{ t('settings.shortcutsGlobalDesc') }}</p>
                    </div>
                    <BaseSwitch :model-value="globalEnabled" size="sm" @update:model-value="onGlobalToggle" />
                  </div>
                  <div class="mt-2 space-y-0.5">
                    <div
                      v-for="def in globalRows"
                      :key="def.action"
                      class="flex items-center justify-between gap-3 rounded-lg px-2 py-1.5 transition-colors hover:bg-zinc-50 dark:hover:bg-zinc-800/60"
                    >
                      <span class="min-w-0 truncate text-zinc-500 dark:text-zinc-400">{{ t(def.labelKey) }}</span>
                      <button
                        type="button"
                        class="shrink-0 rounded-md border px-2.5 py-1 font-mono text-xs transition-colors"
                        :class="
                          isRecording('global', def.action)
                            ? 'animate-pulse border-violet-300 bg-violet-50 text-violet-600 dark:border-violet-500/40 dark:bg-violet-500/15 dark:text-violet-300'
                            : 'hover-accent-border border-zinc-200 bg-zinc-50 text-zinc-600 hover:text-violet-600 dark:border-zinc-700 dark:bg-zinc-800/60 dark:text-zinc-300 dark:hover:text-violet-300'
                        "
                        v-tooltip="isRecording('global', def.action) ? t('settings.shortcutRecordHint') : ''"
                        @click="startRecording('global', def.action)"
                        @blur="onRecordBlur"
                      >
                        <template v-if="globalBindings[def.action]">{{ formatAccel(globalBindings[def.action]!) }}</template>
                        <template v-else>{{ t('settings.shortcutNone') }}</template>
                      </button>
                    </div>
                  </div>
                  <p class="mt-3 text-xs leading-relaxed text-zinc-400">{{ t('settings.shortcutsGlobalHint') }}</p>
                </div>
              </div>
            </section>

            <!-- 关于 -->
            <section>
              <h3 class="mb-2.5 text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ t('settings.about') }}</h3>
              <div class="rounded-xl shadow-sm hover-accent-border border border-zinc-200 bg-white p-4 text-sm text-zinc-500 dark:border-zinc-800 dark:bg-zinc-900">
                <div class="flex flex-wrap items-center justify-between gap-3">
                  <span>{{ t('settings.versionLine', { version: appVersion }) }}</span>
                  <!-- 更新操作区：按状态切换 -->
                  <div class="flex items-center gap-2">
                    <BaseButton
                      v-if="updater.status.value === 'available'"
                      size="sm"
                      :icon="updater.canAutoUpdate() ? RefreshCw : undefined"
                      @click="updater.canAutoUpdate() ? updater.downloadUpdate() : updater.openReleasePage()"
                    >
                      {{
                        updater.canAutoUpdate()
                          ? t('settings.downloadUpdate')
                          : t('settings.updateTo', { version: updater.newVersion.value })
                      }}
                    </BaseButton>
                    <BaseButton
                      v-else-if="updater.status.value === 'downloading'"
                      size="sm"
                      :icon="RefreshCw"
                      loading
                    >
                      {{ t('settings.downloadingUpdate') }}
                    </BaseButton>
                    <BaseButton
                      v-else-if="updater.status.value === 'ready'"
                      size="sm"
                      @click="updater.installAndRestart()"
                    >
                      {{ t('settings.installAndRestart') }}
                    </BaseButton>
                    <span v-if="updater.status.value === 'uptodate'" class="text-xs text-zinc-400">{{ t('settings.upToDateShort') }}</span>
                    <BaseButton
                      size="sm"
                      variant="outline"
                      :loading="updater.status.value === 'checking'"
                      :disabled="updater.status.value === 'checking' || updater.status.value === 'downloading'"
                      :icon="updater.status.value === 'checking' ? undefined : RefreshCw"
                      @click="updater.checkForUpdate(false)"
                    >
                      {{ t('settings.checkForUpdates') }}
                    </BaseButton>
                  </div>
                  <!-- 重新运行首次引导（便于回看 / 演示） -->
                  <div class="mt-3 flex flex-wrap items-center justify-between gap-3 border-t border-zinc-100 pt-3 dark:border-zinc-800">
                    <span class="text-xs text-zinc-400">{{ t('settings.rerunOnboardingDesc') }}</span>
                    <BaseButton size="sm" variant="outline" @click="rerunOnboarding">
                      {{ t('settings.rerunOnboarding') }}
                    </BaseButton>
                  </div>
                </div>
                <!-- 更新版说明 + 下载进度 -->
                <div v-if="updater.status.value === 'available' || updater.status.value === 'downloading' || updater.releaseNotes.value" class="mt-3 border-t border-zinc-100 pt-3 dark:border-zinc-800">
                  <p v-if="updater.status.value === 'available' || updater.status.value === 'downloading'" class="text-xs font-medium text-violet-500">
                    {{ t('settings.updateFound', { version: updater.newVersion.value }) }}
                  </p>
                  <p v-if="updater.releaseNotes.value" class="mt-1 line-clamp-4 whitespace-pre-wrap text-xs leading-relaxed">{{ updater.releaseNotes.value }}</p>
                  <!-- 下载进度条（总量未知时显示不定进度动画） -->
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
                  <p v-if="updater.status.value === 'ready'" class="text-xs font-medium text-violet-500">{{ t('settings.updateDownloadedHint') }}</p>
                  <p v-else-if="updater.status.value === 'available' && !updater.canAutoUpdate()" class="mt-1 text-xs text-zinc-400">{{ t('settings.goReleaseHint') }}</p>
                </div>
              </div>
            </section>
          </div>
        </section>
      </div>
    </div>

    <!-- 已合并艺人弹窗：查看全部合并记录，逐条取消合并 -->
    <MergedArtistsDialog
      v-if="mergedDialogOpen"
      :aliases="artistAliases"
      @close="mergedDialogOpen = false"
      @unmerged="onUnmerged"
    />
  </div>
</template>