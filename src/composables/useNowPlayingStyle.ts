import { ref, watch } from 'vue'

/**
 * 播放页布局预设：
 * side = 左封面右歌词（经典/现状）、stacked = 上封面下歌词。
 */
export type NowPlayingStyleId = 'side' | 'stacked'

const LS_KEY = 'lm.npStyle'
const IDS: NowPlayingStyleId[] = ['side', 'stacked']

function load(): NowPlayingStyleId {
  const raw = localStorage.getItem(LS_KEY)
  return IDS.includes(raw as NowPlayingStyleId) ? (raw as NowPlayingStyleId) : 'side'
}

// 模块级单例：装扮面板写入，NowPlayingView 消费
const npStyle = ref<NowPlayingStyleId>(load())
watch(npStyle, (v) => localStorage.setItem(LS_KEY, v))

export function useNowPlayingStyle() {
  return npStyle
}

/**
 * 播放页主题色来源（装扮面板可切）：
 * auto = 自动跟随封面主色（默认，原行为）；theme = 跟随软件主题色
 * （useThemeColor 选中的主题色，经 themeAmbientPalette 派生整套环境色）。
 */
export type NpAccentMode = 'auto' | 'theme'

const ACCENT_LS_KEY = 'lm.npAccentMode'

function loadAccentMode(): NpAccentMode {
  return localStorage.getItem(ACCENT_LS_KEY) === 'theme' ? 'theme' : 'auto'
}

// 模块级单例：useAmbient 据此切换环境色来源，装扮面板写入
export const npAccentMode = ref<NpAccentMode>(loadAccentMode())
watch(npAccentMode, (v) => localStorage.setItem(ACCENT_LS_KEY, v))

export function useNpAccentMode() {
  return npAccentMode
}
