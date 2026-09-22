import { computed, ref, watch } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { api } from '@/api/commands'
import { usePlayerStore } from '@/stores/player'
import { toast } from '@/composables/useToast'
import { t } from '@/i18n/translate'
import { errorText } from '@/i18n/error'
import { IS_MAC } from '@/utils/platform'

/**
 * 快捷键体系（设置 → 通用 → 快捷键）：
 * - 应用内快捷键：窗口聚焦时生效，动作表 SHORTCUT_DEFS，App.vue 全局 keydown 按配置匹配；
 * - 全局快捷键：系统级热键（默认关闭），Rust 侧注册（src-tauri/src/global_shortcuts.rs），
 *   触发后经 `global-shortcut` 事件广播动作名，这里分发给播放器。
 */

/** 应用内快捷键动作 */
export type ShortcutAction = 'toggle' | 'search' | 'next' | 'prev' | 'lyricForward' | 'lyricBack'
/** 可注册为全局快捷键的动作（SHORTCUT_DEFS 中标记 global 的子集） */
export type GlobalAction = 'toggle' | 'next' | 'prev'

export interface ShortcutDef {
  action: ShortcutAction
  /** 展示名（i18n key） */
  labelKey: string
  /** 默认组合 */
  defaultCombo: string
  /** 焦点在输入框时是否仍触发（如聚焦搜索 Ctrl+F） */
  ignoresTyping?: boolean
  /** 是否支持注册为全局快捷键 */
  global?: boolean
}

export const SHORTCUT_DEFS: ShortcutDef[] = [
  { action: 'toggle', labelKey: 'settings.shortcutPlayPause', defaultCombo: 'Space', global: true },
  { action: 'search', labelKey: 'settings.shortcutFocusSearch', defaultCombo: 'Ctrl+F', ignoresTyping: true },
  { action: 'next', labelKey: 'settings.shortcutNext', defaultCombo: 'N', global: true },
  { action: 'prev', labelKey: 'settings.shortcutPrev', defaultCombo: 'P', global: true },
  // [ ] 歌词校准：[ = 提前 0.5s（歌词前进），] = 延后 0.5s（歌词后退），与播放页口径一致
  { action: 'lyricForward', labelKey: 'settings.shortcutLrcForward', defaultCombo: '[' },
  { action: 'lyricBack', labelKey: 'settings.shortcutLrcBackward', defaultCombo: ']' },
]

/** 可注册为全局快捷键的动作定义 */
export const GLOBAL_DEFS: Array<ShortcutDef & { action: GlobalAction }> = SHORTCUT_DEFS.filter(
  (d): d is ShortcutDef & { action: GlobalAction } => !!d.global,
)

export const DEFAULT_SHORTCUTS: Record<ShortcutAction, string> = Object.fromEntries(
  SHORTCUT_DEFS.map((d) => [d.action, d.defaultCombo]),
) as Record<ShortcutAction, string>

/** 全局快捷键默认组合（用户未录制时，开启开关即套用；CommandOrControl = mac Cmd / 其他平台 Ctrl） */
export const DEFAULT_GLOBAL_ACCELS: Record<GlobalAction, string> = {
  toggle: 'CommandOrControl+Alt+P',
  next: 'CommandOrControl+Alt+Right',
  prev: 'CommandOrControl+Alt+Left',
}

/** 单独按下的修饰键 / 非内容键（不构成快捷键，匹配与录制时均忽略） */
const IGNORED_KEYS = ['Control', 'Meta', 'Alt', 'Shift', 'CapsLock', 'NumLock', 'ScrollLock', 'Dead', 'Unidentified']

/**
 * 从键盘事件构造应用内组合串（与设置页录制器同一口径）：
 * Ctrl 与 Meta（mac Cmd）视为同一修饰键（应用内快捷键无需区分平台），如 "Ctrl+F" / "Space" / "N" / "["。
 * 单独按修饰键返回 null。
 */
export function comboFromEvent(e: KeyboardEvent): string | null {
  const key = normalizeComboKey(e.key)
  if (!key) return null
  const mods: string[] = []
  if (e.ctrlKey || e.metaKey) mods.push('Ctrl')
  if (e.altKey) mods.push('Alt')
  if (e.shiftKey) mods.push('Shift')
  return [...mods, key].join('+')
}

function normalizeComboKey(key: string): string | null {
  if (key === ' ' || key === 'Spacebar') return 'Space'
  if (key.length === 1) return key.toUpperCase()
  if (IGNORED_KEYS.includes(key)) return null
  return key // ArrowUp / F2 / Enter / Home …
}

/** 事件是否命中某个应用内组合（大小写不敏感） */
export function eventMatches(e: KeyboardEvent, combo: string | null | undefined): boolean {
  if (!combo) return false
  const c = comboFromEvent(e)
  return !!c && c.toLowerCase() === combo.toLowerCase()
}

// ---------- 全局快捷键加速键（global-hotkey 解析器口径，见 src-tauri/src/global_shortcuts.rs） ----------

/** 全局快捷键允许的主键（global-hotkey 解析器支持的范围；括号等符号不在支持列表内） */
const GLOBAL_KEY_TOKENS: Record<string, string> = {
  ' ': 'Space',
  ArrowUp: 'ArrowUp',
  ArrowDown: 'ArrowDown',
  ArrowLeft: 'ArrowLeft',
  ArrowRight: 'ArrowRight',
  Minus: 'Minus',
  Equal: 'Equal',
  MediaPlayPause: 'MediaPlayPause',
  MediaTrackNext: 'MediaTrackNext',
  MediaTrackPrevious: 'MediaTrackPrev',
  MediaStop: 'MediaStop',
}

/**
 * 从键盘事件构造全局快捷键加速键串（global-hotkey 口径）：
 * 修饰键 Ctrl / Super（mac Cmd、Windows Win）/ Alt / Shift + 主键（KeyP / Digit1 / F5 / Space …）。
 * 字母与数字必须带修饰键（避免劫持全系统输入）；F 功能键、方向键、媒体键允许单独使用。
 * 不支持的按键返回 null（由调用方提示）。
 */
export function accelFromEvent(e: KeyboardEvent): string | null {
  const k = e.key
  let key: string | null = null
  if (/^[a-z]$/i.test(k)) key = `Key${k.toUpperCase()}`
  else if (/^[0-9]$/.test(k)) key = `Digit${k}`
  else if (/^F([1-9]|1[0-9]|2[0-4])$/.test(k)) key = k
  else if (k in GLOBAL_KEY_TOKENS) key = GLOBAL_KEY_TOKENS[k]
  if (!key) return null
  const mods: string[] = []
  if (e.ctrlKey) mods.push('Ctrl')
  if (e.metaKey) mods.push('Super')
  if (e.altKey) mods.push('Alt')
  if (e.shiftKey) mods.push('Shift')
  // 字母 / 数字必须搭配修饰键，否则会全系统劫持普通打字
  if (mods.length === 0 && (/^[a-z]$/i.test(k) || /^[0-9]$/.test(k))) return null
  return [...mods, key].join('+')
}

/** 应用内组合的展示格式（"Ctrl+F" → "Ctrl + F"） */
export function formatCombo(combo: string): string {
  return combo.split('+').join(' + ')
}

/** 全局加速键串 → 人类可读展示（CommandOrControl/Super 按平台显示，方向键 / 媒体键转符号） */
export function formatAccel(accel: string): string {
  return accel
    .split('+')
    .map((tok) => {
      const t = tok.toLowerCase()
      if (t === 'commandorcontrol' || t === 'cmdorctrl') return IS_MAC ? '⌘' : 'Ctrl'
      if (t === 'super') return IS_MAC ? '⌘' : 'Win'
      if (t === 'alt') return IS_MAC ? '⌥' : 'Alt'
      if (t === 'shift') return IS_MAC ? '⇧' : 'Shift'
      if (t === 'ctrl') return 'Ctrl'
      if (t.startsWith('key')) return tok.slice(3).toUpperCase()
      if (t.startsWith('digit')) return tok.slice(5)
      if (t === 'arrowup') return '↑'
      if (t === 'arrowdown') return '↓'
      if (t === 'arrowleft') return '←'
      if (t === 'arrowright') return '→'
      if (t === 'minus') return '-'
      if (t === 'equal') return '='
      if (t === 'mediaplaypause') return '⏯'
      if (t === 'mediatracknext') return '⏭'
      if (t === 'mediatrackprev') return '⏮'
      if (t === 'mediastop') return '⏹'
      return tok
    })
    .join(' + ')
}

// ---------- 应用内快捷键状态（localStorage 存档，App.vue 与设置页共享同一单例） ----------

const LS_SHORTCUTS = 'lm.shortcuts'

function loadShortcuts(): Record<ShortcutAction, string | null> {
  const base: Record<ShortcutAction, string | null> = { ...DEFAULT_SHORTCUTS }
  try {
    const raw = localStorage.getItem(LS_SHORTCUTS)
    if (raw) {
      const saved = JSON.parse(raw) as Partial<Record<ShortcutAction, string | null>>
      for (const def of SHORTCUT_DEFS) {
        if (def.action in saved) {
          const v = saved[def.action]
          base[def.action] = typeof v === 'string' && v.trim() ? v : null
        }
      }
    }
  } catch {
    /* 存档损坏则用默认值 */
  }
  return base
}

/** 应用内快捷键表（模块级单例）；null = 未设置（该动作禁用） */
const shortcuts = ref<Record<ShortcutAction, string | null>>(loadShortcuts())
watch(shortcuts, persistShortcuts, { deep: true })

function persistShortcuts() {
  try {
    localStorage.setItem(LS_SHORTCUTS, JSON.stringify(shortcuts.value))
  } catch {
    /* ignore */
  }
}

/** 应用内快捷键表（App.vue 匹配按键 / 设置页编辑共享同一状态） */
export function useShortcuts() {
  return shortcuts
}

/** 设置应用内快捷键（null = 禁用该动作；冲突检测由调用方完成） */
export function setShortcut(action: ShortcutAction, combo: string | null) {
  shortcuts.value[action] = combo
}

/** 恢复全部应用内默认快捷键 */
export function resetShortcuts() {
  shortcuts.value = { ...DEFAULT_SHORTCUTS }
}

// ---------- 全局快捷键（默认关闭；localStorage 存档，与桌面歌词同口径） ----------

interface GlobalConfig {
  enabled: boolean
  bindings: Record<GlobalAction, string | null>
}

const LS_GLOBAL = 'lm.globalShortcuts'
const DEFAULT_GLOBAL_CONFIG: GlobalConfig = {
  enabled: false,
  bindings: { ...DEFAULT_GLOBAL_ACCELS },
}

function loadGlobalConfig(): GlobalConfig {
  const base: GlobalConfig = JSON.parse(JSON.stringify(DEFAULT_GLOBAL_CONFIG)) as GlobalConfig
  try {
    const raw = localStorage.getItem(LS_GLOBAL)
    if (raw) {
      const s = JSON.parse(raw) as Partial<GlobalConfig>
      if (typeof s.enabled === 'boolean') base.enabled = s.enabled
      if (s.bindings) {
        for (const def of GLOBAL_DEFS) {
          const v = s.bindings?.[def.action]
          base.bindings[def.action] = typeof v === 'string' && v.trim() ? v : null
        }
      }
    }
  } catch {
    /* 存档损坏则用默认值（关闭 + 默认键位） */
  }
  return base
}

const globalConfig = ref<GlobalConfig>(loadGlobalConfig())
watch(globalConfig, persistGlobal, { deep: true })

function persistGlobal() {
  try {
    localStorage.setItem(LS_GLOBAL, JSON.stringify(globalConfig.value))
  } catch {
    /* ignore */
  }
}

/** 组装当前非空的全局绑定列表（整体替换注册用） */
function globalBindingList(): { action: string; shortcut: string }[] {
  const list: { action: string; shortcut: string }[] = []
  for (const d of GLOBAL_DEFS) {
    const accel = globalConfig.value.bindings[d.action]
    if (accel) list.push({ action: d.action, shortcut: accel })
  }
  return list
}

/** 整体替换注册：成功返回 true；失败 toast 提示（通常为组合已被其他程序占用）并保持未注册状态 */
async function applyGlobalBindings(): Promise<boolean> {
  try {
    await api.globalShortcutsApply(globalBindingList())
    return true
  } catch (e) {
    toast(t('settings.shortcutsGlobalEnableFailed', { error: errorText(e) }), 'error')
    return false
  }
}

/** Rust 侧热键触发 → 分发给播放器 */
function dispatchGlobalAction(action: string) {
  const player = usePlayerStore()
  switch (action) {
    case 'toggle':
      player.toggle()
      break
    case 'next':
      player.next()
      break
    case 'prev':
      player.prev()
      break
  }
}

let globalStarted = false

/** 全局快捷键单例（模块级，仅主窗口初始化；返回 computed 只读状态 + 修改方法） */
export function useGlobalShortcuts() {
  if (!globalStarted) {
    globalStarted = true
    // 热键触发事件（Rust 侧广播，payload 为动作名）
    void listen<string>('global-shortcut', (e) => dispatchGlobalAction(e.payload)).catch(() => {})
    // 恢复上次开启状态：注册失败（如组合被占用）则回落为关闭并保留键位存档
    if (globalConfig.value.enabled) {
      void applyGlobalBindings().then((ok) => {
        if (!ok) globalConfig.value.enabled = false
      })
    }
  }
  return {
    enabled: computed(() => globalConfig.value.enabled),
    bindings: computed(() => globalConfig.value.bindings),
    /** 开 / 关全局快捷键；注册失败返回 false（开关自动回落为关闭） */
    setEnabled: async (next: boolean): Promise<boolean> => {
      if (next) {
        if (!(await applyGlobalBindings())) return false
      } else {
        try {
          await api.globalShortcutsClear()
        } catch {
          /* 注销失败不阻塞状态切换 */
        }
      }
      globalConfig.value.enabled = next
      toast(
        next ? t('settings.shortcutsGlobalOn') : t('settings.shortcutsGlobalOff'),
        'info',
        'settings.globalShortcut',
      )
      return true
    },
    /** 更新单个全局绑定（null = 禁用）；启用状态下即时重新注册，失败自动回滚原键位 */
    setBinding: async (action: GlobalAction, accel: string | null): Promise<boolean> => {
      const prev = globalConfig.value.bindings[action]
      if (prev === accel) return true
      globalConfig.value.bindings[action] = accel
      if (!globalConfig.value.enabled) return true
      const ok = await applyGlobalBindings()
      if (!ok) {
        // 回滚键位，保持存档与实际注册状态一致
        globalConfig.value.bindings[action] = prev
        return false
      }
      return true
    },
  }
}

