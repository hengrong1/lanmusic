import { computed, ref, watch } from 'vue'
import { ensureAnalyser } from './useSpectrum'

/** 频谱皮肤样式：particles = 封面周围圆形粒子，tree = 底部控制器上方树状频谱 */
export type SpectrumStyle = 'particles' | 'tree'

export interface SkinState {
  /** 是否开启频谱 */
  on: boolean
  /** 频谱样式 */
  style: SpectrumStyle
}

const LS_KEY = 'lm.skin'

function load(): SkinState {
  try {
    const raw = localStorage.getItem(LS_KEY)
    if (raw) {
      const s = JSON.parse(raw) as Partial<SkinState>
      return { on: s.on === true, style: s.style === 'tree' ? 'tree' : 'particles' }
    }
  } catch {
    /* 存档损坏走默认 */
  }
  return { on: false, style: 'particles' }
}

// 模块级单例：播放条上的皮肤弹层与播放页共享同一份状态
const skin = ref<SkinState>(load())
watch(skin, (v) => localStorage.setItem(LS_KEY, JSON.stringify(v)), { deep: true })

/** 皮肤弹层是否展开（UI 临时状态，不持久化）。App 的专注模式据此暂停，避免调整皮肤时被隐藏 */
const skinOpen = ref(false)

export function useSkin() {
  return skin
}

export function useSkinOpen() {
  return skinOpen
}

/** 频谱三态单选：none = 关闭；particles / tree = 对应样式（装扮弹层的选项口径，写回 on + style 两字段） */
const spectrumMode = computed<'none' | SpectrumStyle>({
  get: () => (skin.value.on ? skin.value.style : 'none'),
  set: (mode) => {
    if (mode === 'none') {
      skin.value.on = false
      return
    }
    skin.value.on = true
    skin.value.style = mode
    // 切到开启态时确保 AnalyserNode 已创建（AudioContext 需在用户手势内，点选选项即是手势）
    ensureAnalyser()
  },
})

export function useSpectrumMode() {
  return spectrumMode
}
