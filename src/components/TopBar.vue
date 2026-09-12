<script setup lang="ts">
import { ref, watch, computed, onBeforeUnmount } from 'vue'
import { ArrowLeftIcon as ArrowLeft } from '@solar-icons/vue/linear/arrow-left'
import { HistoryIcon as History } from '@solar-icons/vue/linear/history'
import { MoonIcon as Moon } from '@solar-icons/vue/linear/moon'
import { SidebarMinimalisticIcon as SidebarCollapse } from '@solar-icons/vue/linear/sidebar-minimalistic'
import { SidebarIcon as SidebarExpand } from '@solar-icons/vue/linear/sidebar'
import { MagnifierIcon as Search } from '@solar-icons/vue/linear/magnifier'
import { SettingsIcon as SettingsBold } from '@solar-icons/vue/bold/settings'
import { SettingsIcon as Settings } from '@solar-icons/vue/linear/settings'
import { SunIcon as Sun } from '@solar-icons/vue/linear/sun'
import { MonitorIcon as SunMoon } from '@solar-icons/vue/linear/monitor'
import { CloseIcon as X } from '@solar-icons/vue/linear/close'
import { useI18n } from 'vue-i18n'
import { useNav } from '@/composables/useNav'
import { useSidebar } from '@/composables/useSidebar'
import { useTheme } from '@/composables/useTheme'
import { getSearchSettings } from '@/composables/useSearchSettings'
import { api } from '@/api/commands'
import type { Track } from '@/types'
import { CUSTOM_WINDOW_CONTROLS } from '@/utils/platform'
import WindowControls from '@/components/WindowControls.vue'
import HighlightText from '@/components/HighlightText.vue'

const { collapsed } = useSidebar()
function toggleSidebar() {
  collapsed.value = !collapsed.value
}

const { current, back, replaceSearch, canBack, go } = useNav()
const { mode, resolved, setTheme } = useTheme()

const input = ref(current.value.search ?? '')
watch(
  () => current.value.search,
  (s) => (input.value = s ?? ''),
)

// ---- 最近搜索（最多 50 条，localStorage）+ 弹出层实时搜索结果 ----
const RECENT_KEY = 'lm.recentSearches'
const MAX_RECENT = 50
const recentSearches = ref<string[]>(loadRecent())
/** 搜索下拉是否展开（聚焦且未失焦） */
const searchFocused = ref(false)
/** 实时搜索结果（最多 20 条预览） */
const results = ref<Track[]>([])
const resultTotal = ref(0)
// 带参翻译在 setup 内计算：模板里的 $t 全局注入没有带参重载
const { t: translate } = useI18n()
const viewAllLabel = computed(() => translate('search.viewAll', { count: resultTotal.value }))
const searching = ref(false)
let searchSeq = 0

function loadRecent(): string[] {
  try {
    const raw = localStorage.getItem(RECENT_KEY)
    if (raw) {
      const arr = JSON.parse(raw)
      if (Array.isArray(arr)) return arr.filter((x): x is string => typeof x === 'string').slice(0, MAX_RECENT)
    }
  } catch {
    /* 损坏回退空 */
  }
  return []
}
function saveRecent(list: string[]) {
  recentSearches.value = list.slice(0, MAX_RECENT)
  localStorage.setItem(RECENT_KEY, JSON.stringify(recentSearches.value))
}
function recordSearch(q: string) {
  const t = q.trim()
  if (!t) return
  saveRecent([t, ...recentSearches.value.filter((x) => x !== t)])
}
function clearRecent() {
  saveRecent([])
}
function closeDropdown() {
  searchFocused.value = false
}

/** 防抖实时查询：不改路由，结果只在输入框下方弹出层展示 */
async function runSearch(q: string) {
  const my = ++searchSeq
  const t = q.trim()
  if (!t) {
    results.value = []
    resultTotal.value = 0
    searching.value = false
    return
  }
  searching.value = true
  try {
    // api.queryTracks 会自动附加搜索设置（范围/拼音/排序偏好）
    const page = await api.queryTracks({ view: 'all', search: t, pageSize: 20 })
    if (my !== searchSeq) return // 旧回包丢弃
    results.value = page.items
    resultTotal.value = page.total
  } catch {
    if (my === searchSeq) results.value = []
  } finally {
    if (my === searchSeq) searching.value = false
  }
}

/** 是否展示最近搜索（聚焦且输入为空） */
const showRecent = computed(() => searchFocused.value && !input.value.trim())
/** 是否展示搜索结果（聚焦且有输入内容） */
const showResults = computed(() => searchFocused.value && input.value.trim())

function applyRecent(s: string) {
  input.value = s
  replaceSearch(s)
  recordSearch(s)
  closeDropdown()
}
function onFocus() {
  searchFocused.value = true
}
/** 失焦延迟收起（给 mousedown.prevent 选中项留时间）；组件卸载时一并清理 */
let blurTimer: ReturnType<typeof setTimeout> | undefined
function onBlur() {
  clearTimeout(blurTimer)
  blurTimer = setTimeout(() => (searchFocused.value = false), 150)
}
/** Enter：进入完整搜索结果页 */
function onEnter() {
  const v = input.value.trim()
  if (v) {
    replaceSearch(v)
    recordSearch(v)
  }
  closeDropdown()
}
/** 点击弹出层中的一条搜索结果：进入完整搜索结果页（与 Enter 行为一致） */
function playResult(_t: Track) {
  const v = input.value.trim()
  if (v) {
    replaceSearch(v)
    recordSearch(v)
  }
  closeDropdown()
}
/** 查看全部结果 */
function viewAll() {
  const v = input.value.trim()
  if (!v) return
  replaceSearch(v)
  recordSearch(v)
  closeDropdown()
}

let timer: ReturnType<typeof setTimeout> | undefined
function onInput() {
  clearTimeout(timer)
  // 防抖时长可在设置中调整，避免打字过快导致频繁查询卡顿
  timer = setTimeout(() => runSearch(input.value), getSearchSettings().debounceMs)
}
onBeforeUnmount(() => {
  clearTimeout(timer)
  clearTimeout(blurTimer)
})

function clearSearch() {
  input.value = ''
  replaceSearch('')
}

function cycleTheme() {
  const order = ['dark', 'light', 'system'] as const
  const i = order.indexOf(mode.value as (typeof order)[number])
  setTheme(order[(i + 1) % order.length])
}

function focusSearch() {
  document.getElementById('search-input')?.focus()
}
defineExpose({ focusSearch })
</script>

<template>
  <!--
    自定义标题栏：
    - 全平台无边框（macOS Overlay 保留红绿灯并浮于内容上，Windows/Linux 完全自绘）
    - data-tauri-drag-region 只作用于直接命中的元素，空白处可拖拽，交互子元素不受影响
    - macOS 红绿灯位于窗口左上角（侧栏 Logo 行上方），顶栏无需为其预留偏移；Windows/Linux 右侧自绘控制按钮
  -->
  <header
    data-tauri-drag-region
    class="flex h-14 shrink-0 items-center gap-3 rounded-2xl bg-white pl-4 dark:bg-zinc-900"
    :class="CUSTOM_WINDOW_CONTROLS ? 'pr-0' : 'pr-4'"
  >
    <button
      class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-lg text-zinc-500 transition hover:bg-zinc-200/70 hover:text-violet-500 dark:hover:bg-zinc-800 dark:text-zinc-400 dark:hover:text-violet-400"
      v-tooltip="collapsed ? $t('settings.sidebarExpanded') : $t('settings.sidebarCollapsed')"
      @click="toggleSidebar"
    >
      <!-- 侧栏展开时显示「折叠」（sidebar-minimalistic），收起时显示「展开」（sidebar） -->
      <SidebarCollapse v-if="!collapsed" class="h-4 w-4" />
      <SidebarExpand v-else class="h-4 w-4" />
    </button>

    <button
      class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-lg text-zinc-500 transition hover:bg-zinc-200/70 hover:text-violet-500 disabled:cursor-not-allowed disabled:opacity-30 dark:text-zinc-400 dark:hover:bg-zinc-800 dark:hover:text-violet-400"
      :disabled="!canBack"
      v-tooltip="$t('common.back')"
      @click="back()"
    >
      <ArrowLeft class="h-4 w-4" />
    </button>

    <div class="relative mx-auto w-full max-w-md">
      <Search class="pointer-events-none absolute top-1/2 left-3 h-4 w-4 -translate-y-1/2 text-zinc-400" />
      <input
        id="search-input"
        v-model="input"
        autocomplete="off"
        class="h-9 w-full rounded-full border border-zinc-200/70 bg-zinc-100 pr-8 pl-9 text-sm text-zinc-800 outline-none transition placeholder:text-zinc-400 hover:border-zinc-300 hover:bg-zinc-200/60 focus:border-violet-400 focus:bg-white dark:border-transparent dark:bg-zinc-800/70 dark:text-zinc-100 dark:hover:bg-zinc-800 dark:focus:bg-zinc-800"
        :placeholder="$t('library.searchPlaceholder') + ' (Ctrl+F)'"
        @focus="onFocus"
        @blur="onBlur"
        @input="onInput"
        @keydown.enter="onEnter"
        @keydown.esc="clearSearch"
      />
      <button
        v-if="input"
        class="transition-colors duration-150 absolute top-1/2 right-2 flex h-5 w-5 cursor-pointer -translate-y-1/2 items-center justify-center rounded-lg text-zinc-400 hover:bg-zinc-200 hover:text-zinc-600 dark:hover:bg-zinc-700"
        @click="clearSearch"
      >
        <X class="h-3 w-3" />
      </button>

      <!-- 最近搜索下拉（聚焦空输入框时展示，最多 50 条，横向标签排列） -->
      <Transition
        enter-active-class="transition duration-150 ease-out"
        enter-from-class="opacity-0 -translate-y-1"
        leave-active-class="transition duration-100 ease-in"
        leave-to-class="opacity-0 -translate-y-1"
      >
        <div
          v-if="showRecent"
          class="absolute top-full z-50 mt-2 w-full overflow-hidden rounded-xl border border-zinc-200 bg-white shadow-xl dark:border-zinc-700 dark:bg-zinc-800"
        >
          <div class="flex items-center justify-between px-3 py-2">
            <span class="flex items-center gap-1.5 text-xs font-medium text-zinc-400">
              <History class="h-3.5 w-3.5" /> {{ $t('search.recent') }}
            </span>
            <button
              v-if="recentSearches.length"
              class="cursor-pointer rounded-lg px-1.5 py-0.5 text-xs text-zinc-400 transition hover:bg-zinc-100 hover:text-zinc-600 dark:hover:bg-zinc-700 dark:hover:text-zinc-200"
              @mousedown.prevent="clearRecent"
            >{{ $t('common.clear') }}</button>
          </div>

          <!-- 有记录：横向标签排列 -->
          <div v-if="recentSearches.length" class="flex flex-wrap gap-1.5 px-3 pb-3">
            <button
              v-for="s in recentSearches"
              :key="s"
              class="cursor-pointer truncate rounded-full border border-zinc-200 bg-zinc-50 px-3 py-1 text-xs text-zinc-600 transition hover:border-violet-300 hover:bg-violet-50 hover:text-violet-600 dark:border-zinc-600 dark:bg-zinc-700 dark:text-zinc-300 dark:hover:border-violet-500 dark:hover:bg-zinc-600 dark:hover:text-violet-300"
              v-tooltip="s"
              @mousedown.prevent="applyRecent(s)"
            >
              {{ s }}
            </button>
          </div>

          <!-- 无记录：提示文字 -->
          <div v-else class="px-3 pb-4 pt-2 text-center text-xs text-zinc-400 dark:text-zinc-500">
            {{ $t('search.empty') }}
          </div>
        </div>
      </Transition>

      <!-- 搜索结果弹出层（聚焦且有输入时展示，在搜索框下方弹出） -->
      <Transition
        enter-active-class="transition duration-150 ease-out"
        enter-from-class="opacity-0 -translate-y-1"
        leave-active-class="transition duration-100 ease-in"
        leave-to-class="opacity-0 -translate-y-1"
      >
        <div
          v-if="showResults && (results.length || searching)"
          class="absolute top-full z-50 mt-2 w-full overflow-hidden rounded-xl border border-zinc-200 bg-white shadow-xl dark:border-zinc-700 dark:bg-zinc-800"
        >
          <!-- 搜索中提示 -->
          <div v-if="searching && !results.length" class="flex items-center justify-center px-3 py-6 text-sm text-zinc-400">
            <span>{{ $t('search.searching') }}</span>
          </div>

          <!-- 结果列表 -->
          <ul v-else class="max-h-80 overflow-y-auto py-1">
            <li v-for="t in results" :key="t.id">
              <button
                class="flex w-full cursor-pointer items-center gap-2 px-3 py-2 text-left transition hover:bg-violet-50 dark:hover:bg-zinc-700"
                @mousedown.prevent="playResult(t)"
              >
                <!-- 音符占位 -->
                <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded bg-zinc-200 dark:bg-zinc-700">
                  <span class="text-sm text-zinc-400">♪</span>
                </div>
                <!-- 信息 -->
                <div class="min-w-0 flex-1">
                  <div class="flex items-center gap-1.5">
                    <HighlightText
                      :text="t.title || $t('search.unknownTitle')"
                      :keyword="input.trim()"
                      class="min-w-0 truncate text-sm font-medium text-zinc-800 dark:text-zinc-100"
                    />
                    <!-- 匹配字段徽标 -->
                    <span
                      v-if="t.matchedFields?.includes('lyrics')"
                      class="shrink-0 rounded-full bg-green-100 px-1.5 py-0.5 text-[10px] font-medium text-green-700 dark:bg-green-900/40 dark:text-green-300"
                    >{{ $t('player.lyrics') }}</span>
                    <span
                      v-else-if="t.matchedFields?.includes('filename')"
                      class="shrink-0 rounded-full bg-purple-100 px-1.5 py-0.5 text-[10px] font-medium text-purple-700 dark:bg-purple-900/40 dark:text-purple-300"
                      v-tooltip="t.path"
                    >{{ $t('settings.fieldFilename') }}</span>
                  </div>
                  <div class="mt-0.5 flex items-center gap-1 truncate text-xs text-zinc-500 dark:text-zinc-400">
                    <HighlightText v-if="t.artist" :text="t.artist" :keyword="input.trim()" class="min-w-0 truncate" />
                    <span v-if="t.artist && t.album" class="shrink-0">·</span>
                    <HighlightText v-if="t.album" :text="t.album" :keyword="input.trim()" class="min-w-0 truncate" />
                  </div>
                </div>
              </button>
            </li>
          </ul>

          <!-- 底部：查看全部结果 -->
          <div v-if="resultTotal > 0" class="border-t border-zinc-100 px-3 py-2 dark:border-zinc-700">
            <button
              class="w-full cursor-pointer text-left text-xs text-violet-500 transition hover:text-violet-600 dark:text-violet-400 dark:hover:text-violet-300"
              @mousedown.prevent="viewAll"
            >
              {{ viewAllLabel }}
            </button>
          </div>
        </div>
      </Transition>
    </div>

    <button
      class="flex h-8 w-8 shrink-0 cursor-pointer items-center justify-center rounded-lg text-zinc-500 transition hover:bg-zinc-200/70 hover:text-violet-500 dark:text-zinc-400 dark:hover:bg-zinc-800 dark:hover:text-violet-400"
      v-tooltip="$t('settings.theme') + ' · ' + (mode === 'dark' ? $t('settings.themeDark') : mode === 'light' ? $t('settings.themeLight') : $t('settings.themeSystem'))"
      @click="cycleTheme"
    >
      <!-- 跟随系统显示日月，固定深色/浅色时分别显示月亮/太阳 -->
      <SunMoon v-if="mode === 'system'" class="h-4 w-4" />
      <Moon v-else-if="resolved === 'dark'" class="h-4 w-4" />
      <Sun v-else class="h-4 w-4" />
    </button>

    <button
      class="flex h-8 w-8 shrink-0 cursor-pointer items-center justify-center rounded-lg transition"
      :class="
        current.view === 'settings'
          ? 'bg-violet-100 text-violet-700 dark:bg-violet-500/15 dark:text-violet-300'
          : 'text-zinc-500 hover:bg-zinc-200/70 hover:text-violet-500 dark:text-zinc-400 dark:hover:bg-zinc-800 dark:hover:text-violet-400'
      "
      v-tooltip="$t('nav.settings')"
      @click="go({ view: 'settings' })"
    >
      <!-- 设置入口：当前在设置页时图标用 bold 变体 + 选中背景（与侧栏选中态一致） -->
      <SettingsBold v-if="current.view === 'settings'" class="h-4 w-4" />
      <Settings v-else class="h-4 w-4" />
    </button>

    <WindowControls v-if="CUSTOM_WINDOW_CONTROLS" />
  </header>
</template>
