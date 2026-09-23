import { api } from '@/api/commands'

// 响度分析（ReplayGain 音量归一化）：
// 用 Web Audio 解码音频前缀，统计 RMS 与峰值，据此给出建议增益并落库。
// 之所以放在前端：Rust 侧的 symphonia 仅 macOS 编译（见 src-tauri/Cargo.toml），
// 而 WebView 的 Web Audio 在所有平台都能解码（mp3/flac/wav/m4a/ogg 视平台编解码支持）。
//
// 说明：实现的是「RMS 归一化」而非严格 EBU R128（LUFS），对本地曲库拉齐音量已足够。
// 目标 RMS -18 dBFS，并按峰值（保留 0.1 dB 余量）限制增益上限，避免削波。
//
// 取数策略：measure() 只统计前 300 秒，故只拉音频「前缀」即可，无需整首进内存。
// 关键点：Rust 端 serve_file_response 对开区间 Range（bytes=start-）会回「start→末尾」整段，
// 因此续传循环无法限制字节数；这里改用一次性闭区间 Range（bytes=0-(cap-1)）精确取前缀。

export interface LoudnessResult {
  /** 建议增益（dB，两段小数） */
  gainDb: number
  /** 采样峰值（线性 0..1+） */
  peak: number
}

/** 目标 RMS 电平（dBFS） */
const TARGET_RMS_DB = -18
/** 峰值保留余量（dB） */
const PEAK_HEADROOM_DB = -0.1
/** 增益上下限（dB） */
const GAIN_CLAMP_DB = 24
/** 单曲最多分析的秒数（超长音频只取前段） */
const MAX_ANALYZE_SECONDS = 300
/** 拉取字节时的余量秒数：确保前 MAX_ANALYZE_SECONDS 秒完整落在取到的区间里，
 *  末尾截断只影响用不到的尾部，不影响响度统计。 */
const ANALYZE_SLACK_SECONDS = 30
/** 解码采样率：仅用于统计响度，降低采样率可显著减少内存占用 */
const DECODE_SAMPLE_RATE = 24_000
/** 单文件硬上限：仅当「未知时长整首」超过此值才报错；已知时长时只取前段，几乎不会触发 */
const MAX_FETCH_BYTES = 200 * 1024 * 1024
/** 单次 fetch 拉取超时（ms）：自定义协议/网络异常时兜底，避免无限等待 */
const FETCH_TIMEOUT_MS = 30_000
/** 解码单首音频超时（ms）：WebView2 无音频设备时 decodeAudioData 可能永久挂起 */
const DECODE_TIMEOUT_MS = 90_000
/** 整次分析总超时（ms）：fetch + 解码 + 落库的总兜底 */
const OP_TIMEOUT_MS = 180_000

function round2(n: number): number {
  return Math.round(n * 100) / 100
}

/** 给 Promise 套超时：到点未 settle 即 reject，确保调用方一定从 loading 中恢复。 */
function withTimeout<T>(p: Promise<T>, ms: number, label: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const t = setTimeout(() => reject(new Error(`${label}超时`)), ms)
    p.then(
      (v) => {
        clearTimeout(t)
        resolve(v)
      },
      (e) => {
        clearTimeout(t)
        reject(e)
      },
    )
  })
}

/** 单次带超时的字节拉取；返回响应体、状态码与文件总大小（从 Content-Range 解析）。 */
async function fetchBytes(
  url: string,
  range: string | undefined,
): Promise<{ buf: Uint8Array; status: number; total: number }> {
  const ctrl = new AbortController()
  const timer = setTimeout(() => ctrl.abort(), FETCH_TIMEOUT_MS)
  try {
    const resp = await fetch(url, range ? { headers: { Range: range }, signal: ctrl.signal } : { signal: ctrl.signal })
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`)
    const buf = new Uint8Array(await resp.arrayBuffer())
    const m = resp.headers.get('Content-Range')?.match(/\/(\d+)$/)
    const total = m ? Number(m[1]) : resp.status === 200 ? buf.length : Number.POSITIVE_INFINITY
    return { buf, status: resp.status, total }
  } catch (e) {
    if (ctrl.signal.aborted) throw new Error('音频拉取超时')
    throw e
  } finally {
    clearTimeout(timer)
  }
}

/**
 * 取解码所需的前缀字节（按曲目时长只取前 ~330s；未知时长则取整首但受 MAX_FETCH_BYTES 约束）。
 * 最多两次请求，字节数严格受控：先无 Range 取首块拿到 total，再用闭区间 Range 精确取前缀。
 */
async function fetchPrefix(url: string, durationSec?: number | null): Promise<ArrayBuffer> {
  // 1) 先无 Range 取首块，拿到 total 与首 2MB
  const first = await fetchBytes(url, undefined)
  if (first.status === 200) {
    return first.buf.buffer as ArrayBuffer
  }

  const total = first.total
  // 已知时长 → 只取前 (MAX_ANALYZE_SECONDS + 余量) 秒对应字节；否则取整首（受硬上限约束）
  let cap = MAX_FETCH_BYTES
  if (durationSec && durationSec > 0 && Number.isFinite(total)) {
    const needSec = Math.min(1, (MAX_ANALYZE_SECONDS + ANALYZE_SLACK_SECONDS) / durationSec)
    cap = Math.min(total, Math.ceil(total * needSec))
  } else if (Number.isFinite(total)) {
    cap = Math.min(total, MAX_FETCH_BYTES)
  }

  // 首块已够覆盖所需前缀 → 直接用它（如短曲目、或 cap <= 2MB）
  if (cap <= first.buf.length) {
    return first.buf.buffer as ArrayBuffer
  }
  if (cap > MAX_FETCH_BYTES) throw new Error('audio too large')

  // 2) 一次性闭区间 Range 精确取前缀（Rust 端对 bytes=start- 会回整段剩余，故必须闭区间）
  const r = await fetchBytes(url, `bytes=0-${cap - 1}`)
  return r.buf.buffer as ArrayBuffer
}

/** 分析某曲目的响度并写入曲库；失败抛错（调用方提示）。
 * durationSec 提供时只拉前段字节，长曲目不再整首进内存或触发 "audio too large"。 */
export async function analyzeLoudness(
  trackId: number,
  durationSec?: number | null,
): Promise<LoudnessResult> {
  return withTimeout(innerAnalyze(trackId, durationSec), OP_TIMEOUT_MS, '响度分析')
}

/** analyzeLoudness 的实际实现；外层套总超时，确保任何环节挂起都能恢复。 */
async function innerAnalyze(trackId: number, durationSec?: number | null): Promise<LoudnessResult> {
  const url = await api.getStreamUrl(trackId)
  let buf = await fetchPrefix(url, durationSec)

  const ctx = new AudioContext({ sampleRate: DECODE_SAMPLE_RATE })
  try {
    let audio: AudioBuffer
    try {
      // decodeAudioData 在部分 WebView2 环境（无音频设备 / 音频服务异常）下
      // Promise 会永久不 resolve → 前端永远停在「分析中」。用超时兜住，超时就
      // 抛错让调用方结束 loading 并提示，而不是卡死。
      audio = await withTimeout(ctx.decodeAudioData(buf), DECODE_TIMEOUT_MS, '音频解码')
    } catch (e) {
      // 极少数容器（需完整文件才能解码）在截断前缀上会失败 → 回退取整首再解一次
      console.warn('[loudness] prefix decode failed, retry with full file:', e)
      buf = await fetchPrefix(url, null)
      audio = await withTimeout(ctx.decodeAudioData(buf), DECODE_TIMEOUT_MS, '音频解码')
    }

    const { rmsDb, peak } = measure(audio)
    let gainDb = TARGET_RMS_DB - rmsDb
    // 峰值限制：应用增益后峰值不得超过 PEAK_HEADROOM_DB
    if (peak > 0) {
      const peakDb = 20 * Math.log10(peak)
      const allowed = PEAK_HEADROOM_DB - peakDb
      if (gainDb > allowed) gainDb = allowed
    }
    gainDb = Math.max(-GAIN_CLAMP_DB, Math.min(GAIN_CLAMP_DB, gainDb))
    const rounded = round2(gainDb)

    await api.saveLoudness(trackId, rounded, round2(peak))
    return { gainDb: rounded, peak: round2(peak) }
  } finally {
    void ctx.close().catch(() => {})
  }
}

/** 从解码后的多声道采样统计 RMS（dBFS）与峰值（线性） */
function measure(audio: AudioBuffer): { rmsDb: number; peak: number } {
  const maxFrames = Math.min(audio.length, Math.floor(audio.sampleRate * MAX_ANALYZE_SECONDS))
  let sumSq = 0
  let peak = 0
  for (let ch = 0; ch < audio.numberOfChannels; ch++) {
    const data = audio.getChannelData(ch)
    for (let i = 0; i < maxFrames; i++) {
      const v = data[i]
      sumSq += v * v
      const a = v < 0 ? -v : v
      if (a > peak) peak = a
    }
  }
  const count = maxFrames * audio.numberOfChannels
  const rms = count > 0 ? Math.sqrt(sumSq / count) : 0
  const rmsDb = rms > 0 ? 20 * Math.log10(rms) : -120
  return { rmsDb, peak }
}
