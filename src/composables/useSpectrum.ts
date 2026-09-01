import { usePlayerStore } from '@/stores/player'

// 模块级单例：AudioContext / AnalyserNode 全局只需一份
// createMediaElementSource 绑定的是 audio 元素本身（不是某次媒体资源），
// 因此切歌、seek 后依然有效；前提是元素设置了 crossOrigin（player store 已设置），
// 否则跨域媒体被视为污染源，节点会输出静音。
let ctx: AudioContext | null = null
let analyser: AnalyserNode | null = null

function resumeIfNeeded() {
  if (ctx && ctx.state === 'suspended') void ctx.resume().catch(() => {})
}

// 任何用户手势都尝试恢复挂起的 context（自动播放策略兜底）
let gestureArmed = false
function armGesture() {
  if (gestureArmed) return
  gestureArmed = true
  window.addEventListener('pointerdown', resumeIfNeeded)
  window.addEventListener('keydown', resumeIfNeeded)
}

/** 是否已发生过用户手势（AudioContext 需在手势内创建/恢复才能运行） */
function hasUserGesture(): boolean {
  return navigator.userActivation?.isActive ?? true
}

/**
 * 确保分析器就绪。
 * 音频输出图：source -> analyser -> destination（元素声音经 context 输出）。
 * 若当前不在用户手势内（如启动时持久化皮肤直接渲染画布），推迟到首次手势再创建，
 * 避免 context 因自动播放策略保持 suspended 导致整条链路无声。
 */
export function ensureAnalyser(): AnalyserNode | null {
  if (analyser) {
    resumeIfNeeded()
    return analyser
  }
  if (!hasUserGesture()) {
    // 推迟到首次用户手势时创建
    const tryCreate = () => {
      if (!analyser) ensureAnalyser()
      if (analyser) {
        window.removeEventListener('pointerdown', tryCreate)
        window.removeEventListener('keydown', tryCreate)
      }
    }
    window.addEventListener('pointerdown', tryCreate)
    window.addEventListener('keydown', tryCreate)
    return null
  }
  try {
    const player = usePlayerStore()
    ctx = new AudioContext()
    const source = ctx.createMediaElementSource(player.audio)
    analyser = ctx.createAnalyser()
    analyser.fftSize = 512
    analyser.smoothingTimeConstant = 0.82
    source.connect(analyser)
    analyser.connect(ctx.destination)
    armGesture()
  } catch {
    // 创建失败时静默降级为无频谱，不影响播放
    ctx = null
    analyser = null
    return null
  }
  return analyser
}

let lastResumeTry = 0

/** 读取当前频谱数据到 out（0-255）；分析器未就绪 / context 未运行时返回 false（调用方按静音绘制） */
export function readSpectrum(out: Uint8Array): boolean {
  if (!analyser || !ctx) return false
  if (ctx.state === 'suspended') {
    // 每秒重试一次恢复（真正的恢复仍需用户手势配合）
    const now = performance.now()
    if (now - lastResumeTry > 1000) {
      lastResumeTry = now
      void ctx.resume().catch(() => {})
    }
    return false
  }
  analyser.getByteFrequencyData(out as Uint8Array<ArrayBuffer>)
  return true
}
