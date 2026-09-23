import { ref } from 'vue'
import { usePlayerStore } from '@/stores/player'
import { toast } from '@/composables/useToast'
import { t as tr } from '@/i18n/translate'

// 睡眠定时器：到时（或当前曲目播完）淡出并暂停。模块级单例，UI 与播放器共享状态。
// 选项：分钟数（15/30/45/60）或 'endOfTrack'（播完当前曲目后停止）。

export type SleepMode = number | 'endOfTrack'

const LS_MODE = 'lm.sleepTimerMode'

/** 预设分钟数 */
export const SLEEP_PRESETS: number[] = [15, 30, 45, 60]

const remaining = ref(0) // 剩余秒数（0 = 未启用）
const mode = ref<SleepMode | null>(null)
let tickTimer: ReturnType<typeof setInterval> | undefined
let fireTimer: ReturnType<typeof setTimeout> | undefined
let onEnded: (() => void) | null = null

function clearTimers() {
  if (tickTimer) clearInterval(tickTimer)
  if (fireTimer) clearTimeout(fireTimer)
  tickTimer = undefined
  fireTimer = undefined
  const player = usePlayerStore()
  if (onEnded) {
    player.audio.removeEventListener('ended', onEnded)
    onEnded = null
  }
}

/** 取消睡眠定时器 */
export function cancelSleepTimer() {
  clearTimers()
  remaining.value = 0
  mode.value = null
}

/** 按分钟数启动 */
export function startSleepTimer(minutes: number) {
  cancelSleepTimer()
  const ms = Math.round(minutes * 60_000)
  mode.value = minutes
  try {
    localStorage.setItem(LS_MODE, String(minutes))
  } catch {
    /* ignore */
  }
  const deadline = Date.now() + ms
  remaining.value = Math.ceil(ms / 1000)
  tickTimer = setInterval(() => {
    remaining.value = Math.max(0, Math.ceil((deadline - Date.now()) / 1000))
  }, 1000)
  fireTimer = setTimeout(fire, ms)
  toast(tr('toast.sleepTimerOn', { minutes }), 'info', 'sleep-timer')
}

/** 当前曲目播完后停止 */
export function startSleepTimerEndOfTrack() {
  cancelSleepTimer()
  mode.value = 'endOfTrack'
  try {
    localStorage.setItem(LS_MODE, 'endOfTrack')
  } catch {
    /* ignore */
  }
  const player = usePlayerStore()
  // 当前曲已播完停在结尾时 ended 不会再派发，监听会永久挂起——直接按到点处理
  if (player.audio.ended) {
    fire()
    return
  }
  onEnded = () => fire()
  player.audio.addEventListener('ended', onEnded)
  toast(tr('toast.sleepTimerEndOfTrack'), 'info', 'sleep-timer')
}

/** 到时：淡出并暂停 */
function fire() {
  const player = usePlayerStore()
  if (!player.current) {
    cancelSleepTimer()
    return
  }
  // 「播完当前曲」模式的暂停由 player 的 ended 处理器拦截（consumeSleepStop，
  // 防止自动切下一首）；走到这里的若已是暂停/结束态就不再 toggle
  if (mode.value !== 'endOfTrack' && !player.audio.paused) player.toggle()
  toast(tr('toast.sleepTimerFired'), 'info', 'sleep-timer')
  cancelSleepTimer()
}

/**
 * player 的 ended 处理器最先调用：「播完当前曲」模式到此为止（消费掉标志并
 * 返回 true），player 据此跳过自动切下一首。返回 false = 非睡眠停止，正常推进。
 */
export function consumeSleepStop(): boolean {
  if (mode.value !== 'endOfTrack') return false
  clearTimers()
  remaining.value = 0
  mode.value = null
  return true
}

export function useSleepTimer() {
  return { remaining, mode, presets: SLEEP_PRESETS, startSleepTimer, startSleepTimerEndOfTrack, cancelSleepTimer }
}
