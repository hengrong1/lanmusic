/**
 * 播放时阻止系统休眠/锁屏。
 * - Windows：Rust 侧 SetThreadExecutionState(ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED)
 * - 其他平台：尝试 Web Wake Lock API（navigator.wakeLock），不支持则静默降级
 * 开关存 localStorage `lm.preventSleep`（默认启用）。
 */
import { api } from '@/api/commands'

const LS = 'lm.preventSleep'

/** 读取防休眠开关（默认启用） */
export function getPreventSleep(): boolean {
  return localStorage.getItem(LS) !== '0'
}

/** 设置开关（设置页调用），立即按当前播放状态应用 */
export function setPreventSleepSetting(v: boolean, playing: boolean) {
  localStorage.setItem(LS, v ? '1' : '0')
  void applyPowerGuard(playing)
}

let wakeLock: { release: () => Promise<void> } | null = null

async function lockWake() {
  try {
    const nav = navigator as Navigator & {
      wakeLock?: { request(type: 'screen'): Promise<{ release: () => Promise<void> }> }
    }
    if (nav.wakeLock) wakeLock = await nav.wakeLock.request('screen')
  } catch {
    /* 不支持/被拒绝则忽略 */
  }
}

function releaseWake() {
  if (wakeLock) {
    try {
      void wakeLock.release()
    } catch {
      /* ignore */
    }
    wakeLock = null
  }
}

/** 应用防休眠策略：开关开启且正在播放 → 请求保持系统唤醒，否则释放 */
export function applyPowerGuard(playing: boolean) {
  const on = getPreventSleep() && playing
  if (on) void lockWake()
  else releaseWake()
  void api.setPreventSleep(on).catch(() => {})
}