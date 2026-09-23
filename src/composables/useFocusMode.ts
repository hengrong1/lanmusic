/**
 * 播放页专注模式开关（设置 → 播放）：读写单点。
 * 默认开启——遵循项目「'0' 才算关」通例（!=='0' 即开）。
 * App.vue 播放时按此计时隐藏控制条；SettingsView 提供开关。
 * 口径只此一处，别在调用侧再写 localStorage.getItem('lm.focusMode')。
 */

const LS_KEY = 'lm.focusMode'

/** 专注模式是否开启（每次调用实时读，设置变更即时生效） */
export function isFocusModeEnabled(): boolean {
  return localStorage.getItem(LS_KEY) !== '0'
}

/** 写入专注模式开关 */
export function setFocusModeEnabled(v: boolean): void {
  localStorage.setItem(LS_KEY, v ? '1' : '0')
}
