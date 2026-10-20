import { ref } from 'vue'
import { usePlayerStore } from '@/stores/player'

// 共享 Web Audio 图：整个应用只有这一条「元素 → 处理 → 扬声器」的链路。
// 关键点：createMediaElementSource 对同一个 <audio> 元素**终身只能调用一次**，
// 频谱（useSpectrum）、均衡器、音量归一化都必须挂在这同一张图上，不能各自创建 source。
//
// 链路：source -> inputGain -> [10 段 BiquadFilter(peaking)] -> normalizeGain -> analyser -> destination
// - audio.volume（用户音量）在 source 节点之前生效（MediaElementAudioSourceNode 仍受元素音量影响），保持不变
// - inputGain：纯透传（保留扩展位）
// - BiquadFilter 段：图形均衡器
// - normalizeGain：ReplayGain / 音量归一化（在用户音量之上再乘一个修正增益）
// - analyser：频谱取样，串在主输出链上（normalizeGain -> analyser -> destination），
//   频谱从它读数据；音频经 analyser 直通扬声器

export const EQ_FREQS = [31, 62, 125, 250, 500, 1000, 2000, 4000, 8000, 16000]
export const EQ_BANDS = EQ_FREQS.length
/** 各预设：每段增益（dB），下标对应 EQ_FREQS；Flat = 全 0。
 *  幅度刻意克制（2026-10-13 按用户反馈从原值约 60% 收敛）——预设是套用起点，
 *  过猛会一开就「过冲」，忠实保留原曲线的形态即可 */
export const EQ_PRESETS: Record<string, number[]> = {
  flat: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
  rock: [3, 2, -0.5, -1, -0.5, 0.5, 2, 2.5, 2.5, 2.5],
  pop: [-0.5, 1, 2.5, 2.5, 1, 0, -0.5, -0.5, 0.5, 1],
  classical: [2.5, 2, 1, 0.5, -0.5, -0.5, 0, 1, 2, 2.5],
  jazz: [2, 1, 0.5, 1, -0.5, -0.5, 0, 0.5, 1, 2],
  bass: [4, 3.5, 3, 2, 0.5, 0, 0, 0, 0, 0],
  vocal: [-1, -0.5, 0.5, 2, 2.5, 2.5, 2, 0.5, -0.5, -1],
  treble: [0, 0, 0, 0, 0, 0.5, 1, 2.5, 3.5, 4],
}

export interface EqState {
  /** 是否启用均衡器（键不存在即关，只有显式存 '1' 才算开——遵循项目「'0' 才算关」通例的反面：默认关） */
  enabled: boolean
  preset: string
  /** 自定义增益（应用预设后用户再微调），与 EQ_FREQS 等长 */
  gains: number[]
}

const LS_EQ = 'lm.eq'
const LS_NORM_ON = 'lm.normOn'

// ---- 单例节点 ----
let ctx: AudioContext | null = null
let source: MediaElementAudioSourceNode | null = null
let inputGain: GainNode | null = null
let bands: BiquadFilterNode[] = []
let normGain: GainNode | null = null
let analyser: AnalyserNode | null = null
let createFailed = false
let gestureArmed = false

function resumeIfNeeded() {
  if (ctx && ctx.state === 'suspended') void ctx.resume().catch(() => {})
}

function hasUserGesture(): boolean {
  return navigator.userActivation?.isActive ?? true
}

// 无手势时的延迟建图：只挂一次监听（固定闭包引用），手势到来时建图并自摘。
// deferredArmed 防重复挂；建图失败（createFailed）后允许重新挂——用户手势内
// 重建有可能成功（此前失败可能只是无手势上下文）
let deferredArmed = false
function armDeferredCreate() {
  if (deferredArmed) return
  deferredArmed = true
  const tryCreate = () => {
    window.removeEventListener('pointerdown', tryCreate)
    window.removeEventListener('keydown', tryCreate)
    deferredArmed = false
    if (!analyser && !createFailed) ensureGraph()
  }
  window.addEventListener('pointerdown', tryCreate)
  window.addEventListener('keydown', tryCreate)
}

function armGesture() {
  if (gestureArmed) return
  gestureArmed = true
  window.addEventListener('pointerdown', resumeIfNeeded)
  window.addEventListener('keydown', resumeIfNeeded)
}

function loadEq(): EqState {
  try {
    const raw = localStorage.getItem(LS_EQ)
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<EqState>
      const gains = Array.isArray(parsed.gains) && parsed.gains.length === EQ_BANDS
        ? parsed.gains.map((g) => Math.max(-24, Math.min(24, Number(g) || 0)))
        : [...EQ_PRESETS.flat]
      return {
        enabled: parsed.enabled === true,
        preset: typeof parsed.preset === 'string' ? parsed.preset : 'flat',
        gains,
      }
    }
  } catch {
    /* 损坏则回退默认 */
  }
  return { enabled: false, preset: 'flat', gains: [...EQ_PRESETS.flat] }
}

function saveEq(s: EqState) {
  try {
    localStorage.setItem(LS_EQ, JSON.stringify(s))
  } catch {
    /* ignore */
  }
}

function clampGain(db: number): number {
  return Math.max(-24, Math.min(24, db))
}

function dbToLinear(db: number): number {
  return Math.pow(10, db / 20)
}

/** 归一化增益上限（dB）：+12 已足够覆盖低电平录音；不设 +24 这种会把歌推爆的值 */
const NORM_CLAMP_DB = 12
/** 削波保护：应用增益后峰值不得超过此电平（dBFS） */
const PEAK_SAFE_DB = -0.3

function clampNorm(db: number): number {
  return Math.max(-NORM_CLAMP_DB, Math.min(NORM_CLAMP_DB, db))
}

/**
 * 由峰值反推「最大安全增益」（dB）。峰值缺失/无效时返回 +∞（无从限制）。
 * 这是归一化的最后一道保险：即便增益数据有误（如标签乱写、分析异常），
 * 也不会把超过 0dBFS 的信号送进扬声器——削波的听感正是「声音巨大又闷又破」。
 */
function maxSafeGainDb(peak: number | null): number {
  if (peak == null || !Number.isFinite(peak) || peak <= 0.0001) return Number.POSITIVE_INFINITY
  return PEAK_SAFE_DB - 20 * Math.log10(peak)
}

/** 把 EQ 状态落到图上（图未就绪时只更新内存状态，待建图时一并应用） */
function applyEqState(s: EqState) {
  // 内存态始终更新，便于图就绪后 apply
  pendingEq = s
  if (!ctx) {
    saveEq(s)
    return
  }
  const gains = s.enabled ? s.gains : EQ_PRESETS.flat
  for (let i = 0; i < bands.length; i++) {
    const target = clampGain(gains[i] ?? 0)
    const b = bands[i]
    // 平滑过渡避免突变（setTargetAtTime：约 30ms 到位）
    b.gain.setTargetAtTime(target, ctx!.currentTime, 0.015)
  }
  saveEq(s)
}

// 建图前的待应用 EQ 状态：从本地存档载入（否则首次建图会把已保存的均衡器覆盖成平直）
let pendingEq: EqState = loadEq()

function applyNormalize(db: number, enabled: boolean, peak: number | null, immediate = false) {
  if (!ctx || !normGain) return
  let effective = clampNorm(db)
  const safe = maxSafeGainDb(peak)
  if (effective > safe) effective = safe
  const linear = enabled ? dbToLinear(effective) : 1
  if (immediate) {
    // 换源瞬间必须瞬跳：此时旧音频已切断、新音频还没出声，平滑过渡没有意义，
    // 反而会让上一个值残留几十毫秒（旧曲尾巴被新曲增益放大 = 用户听到「响一声」）
    normGain.gain.cancelScheduledValues(ctx.currentTime)
    normGain.gain.setValueAtTime(linear, ctx.currentTime)
  } else {
    // 播放中调整（开关、分析完成、微调）用平滑过渡，避免可闻的跳变
    normGain.gain.setTargetAtTime(linear, ctx.currentTime, 0.02)
  }
}

export interface AudioGraph {
  ctx: AudioContext
  analyser: AnalyserNode
}

/** 确保共享音频图就绪；返回图或 null（无手势 / 已失败降级）。 */
export function ensureGraph(): AudioGraph | null {
  if (analyser && ctx) {
    resumeIfNeeded()
    return { ctx, analyser }
  }
  if (createFailed) return null
  if (!hasUserGesture()) {
    // 推迟到首次用户手势时创建（Autoplay 策略要求 context 在手势内创建/恢复）。
    // 只挂一次监听：频谱逐帧轮询会反复走到这里（媒体键播放时 userActivation 过期），
    // 逐次挂监听器会堆积成百上千个，直到下一次真实点击才被批量清理
    armDeferredCreate()
    return null
  }
  try {
    const player = usePlayerStore()
    ctx = new AudioContext()
    source = ctx.createMediaElementSource(player.audio)
    inputGain = ctx.createGain()
    bands = EQ_FREQS.map((f) => {
      const b = ctx!.createBiquadFilter()
      b.type = 'peaking'
      b.frequency.value = f
      b.Q.value = 1.0
      b.gain.value = 0
      return b
    })
    normGain = ctx.createGain()
    normGain.gain.value = 1
    analyser = ctx.createAnalyser()
    analyser.fftSize = 512
    analyser.smoothingTimeConstant = 0.82

    // 串联：source -> inputGain -> bands... -> normGain -> analyser -> destination
    source.connect(inputGain)
    let node: AudioNode = inputGain
    for (const b of bands) {
      node.connect(b)
      node = b
    }
    node.connect(normGain)
    normGain.connect(analyser)
    analyser.connect(ctx.destination)

    // 应用已保存的 EQ 与归一化状态
    applyEqState(pendingEq)
    if (normEnabled.value) applyNormalize(currentNormalizeDb, true, currentNormalizePeak)
    armGesture()
  } catch {
    // 创建失败（如二次创建）：静默降级为无音效处理，不影响播放
    createFailed = true
    // 释放半建成的 AudioContext（占用音频线程资源，不 close 会成孤儿）
    try {
      ctx?.close()
    } catch {
      /* 关闭失败无需处理 */
    }
    ctx = null
    source = null
    analyser = null
    return null
  }
  return { ctx, analyser }
}

/** 供 useSpectrum 取分析器节点（频谱旁路，不重复连到 destination） */
export function getAnalyser(): AnalyserNode | null {
  const g = ensureGraph()
  return g?.analyser ?? null
}

// ---- 归一化（ReplayGain / 音量归一化）内存态 ----
export const normEnabled = ref(localStorage.getItem(LS_NORM_ON) === '1')
let currentNormalizeDb = 0
/** 最近一次写入的峰值（线性）：用于削波保护，null 表示无数据 */
let currentNormalizePeak: number | null = null

export function getNormalizeEnabled(): boolean {
  return normEnabled.value
}

/** 设置归一化开关；开启时按最近一次写入的增益生效，关闭时增益归 1（不影响用户音量） */
export function setNormalizeEnabled(v: boolean) {
  normEnabled.value = v
  try {
    localStorage.setItem(LS_NORM_ON, v ? '1' : '0')
  } catch {
    /* ignore */
  }
  applyNormalize(currentNormalizeDb, v, currentNormalizePeak)
}

/**
 * 写入某曲目的归一化增益（dB）与峰值（线性，可选），并在开启时立即应用。
 * peak 参与削波保护：实际生效增益会被压到「应用后峰值 ≤ PEAK_SAFE_DB」以内。
 * immediate=true 用于换源瞬间：直接跳到目标值，不做 20ms 平滑（见 applyNormalize）。
 */
export function setNormalizeGain(db: number, peak?: number | null, immediate = false) {
  currentNormalizeDb = clampNorm(db)
  currentNormalizePeak = peak ?? null
  applyNormalize(currentNormalizeDb, normEnabled.value, currentNormalizePeak, immediate)
}

// ---- EQ 内存态与接口 ----
export const eqEnabled = ref(loadEq().enabled)

export function getEqEnabled(): boolean {
  return eqEnabled.value
}

/** 设置均衡器开关：关闭时所有段增益归 0（平直），开启时恢复已保存的增益 */
export function setEqEnabled(v: boolean) {
  eqEnabled.value = v
  if (v) ensureGraph()
  const s = loadEq()
  s.enabled = v
  applyEqState(s)
}

/** 应用一个预设（同时视为启用均衡器） */
export function applyEqPreset(name: string) {
  const preset = EQ_PRESETS[name] ?? EQ_PRESETS.flat
  const s: EqState = { enabled: true, preset: name, gains: [...preset] }
  eqEnabled.value = true
  applyEqState(s)
}

/** 设置某段增益（dB），自动启用均衡器 */
export function setEqBandGain(index: number, db: number) {
  const s = loadEq()
  if (index < 0 || index >= s.gains.length) return
  s.gains[index] = clampGain(db)
  s.preset = 'custom' // 手动微调即视为自定义，脱离原预设（前端据此自动选中「自定义」）
  s.enabled = true
  eqEnabled.value = true
  applyEqState(s)
}

/** 取当前 EQ 状态（UI 初始化用） */
export function getEqState(): EqState {
  return loadEq()
}

/** 是否任一音频处理特性处于开启态（决定是否要建立音频图，见 App.vue） */
export function audioProcessingActive(): boolean {
  return eqEnabled.value || normEnabled.value
}
