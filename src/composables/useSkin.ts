import { ref, watch } from 'vue'

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

export function useSkin() {
  return skin
}
