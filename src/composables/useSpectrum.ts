// 频谱分析：复用 useAudioGraph 的共享分析器节点（createMediaElementSource 对同一 audio
// 元素终身只能调用一次，因此频谱不再自建 source，而是取共享图上已挂好的 analyser 旁路）。
// 音频输出图由 useAudioGraph 统一管理：source -> ... -> analyser -> destination。
import { getAnalyser } from '@/composables/useAudioGraph'

let lastResumeTry = 0

/** 读取当前频谱数据到 out（0-255）；分析器未就绪 / context 未运行时返回 false（调用方按静音绘制） */
export function readSpectrum(out: Uint8Array): boolean {
  const analyser = getAnalyser()
  if (!analyser) return false
  if (analyser.context.state === 'suspended') {
    // 每秒重试一次恢复（真正的恢复仍需用户手势配合）
    const now = performance.now()
    if (now - lastResumeTry > 1000) {
      lastResumeTry = now
      // AnalyserNode.context 的类型是 BaseAudioContext（无 resume）：实际是 AudioContext
      void (analyser.context as AudioContext).resume().catch(() => {})
    }
    return false
  }
  analyser.getByteFrequencyData(out as Uint8Array<ArrayBuffer>)
  return true
}

/**
 * 确保分析器就绪：转调共享音频图（其内部处理用户手势推迟与创建失败降级）。
 * 返回 AnalyserNode 或 null。原 useSpectrum 自建 source 的逻辑已迁移到 useAudioGraph，
 * 这里只负责把「建图」这一步暴露给 App.vue 的预热 watch。
 */
export function ensureAnalyser(): AnalyserNode | null {
  return getAnalyser()
}
