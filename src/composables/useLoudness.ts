import { api } from '@/api/commands'

// 响度分析（ReplayGain 音量归一化）：
// 用 Web Audio 解码整首音频，统计 RMS 与峰值，据此给出建议增益并落库。
// 之所以放在前端：Rust 侧的 symphonia 仅 macOS 编译（见 src-tauri/Cargo.toml），
// 而 WebView 的 Web Audio 在所有平台都能解码（mp3/flac/wav/m4a/ogg 视平台编解码支持）。
//
// 说明：实现的是「RMS 归一化」而非严格 EBU R128（LUFS），对本地曲库拉齐音量已足够。
// 目标 RMS -18 dBFS，并按峰值（保留 0.1 dB 余量）限制增益上限，避免削波。

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
/** 解码采样率：仅用于统计响度，降低采样率可显著减少内存占用 */
const DECODE_SAMPLE_RATE = 24_000
/** music:// 协议单次响应封顶 2MB（见 scheme.rs CHUNK），大文件需循环 Range 取块拼全 */
const CHUNK_BYTES = 2 * 1024 * 1024
/** 取数上限（防御异常超大文件）：300s 无损约 40MB，留余量 */
const MAX_FETCH_BYTES = 64 * 1024 * 1024

function round2(n: number): number {
  return Math.round(n * 100) / 100
}

/**
 * 从 music:// 协议取完整音频字节。协议端对「无 Range」请求的大文件只返回
 * 前 2MB（206 + Content-Range），直接 fetch 会拿到截断流 → 解码失败或响度
 * 只反映开头几秒；因此按 Content-Range 的 total 循环 Range 取块拼接。
 */
async function fetchAllBytes(url: string): Promise<ArrayBuffer> {
  const chunks: Uint8Array[] = []
  let received = 0
  let total = Number.POSITIVE_INFINITY
  let first = true
  while (received < total) {
    if (received > MAX_FETCH_BYTES) throw new Error('audio too large')
    const resp = await fetch(url, first ? undefined : { headers: { Range: `bytes=${received}-` } })
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`)
    const buf = new Uint8Array(await resp.arrayBuffer())
    if (buf.length === 0) break
    chunks.push(buf)
    received += buf.length
    if (first) {
      first = false
      if (resp.status === 200) break // 一次拿全（小文件）
      // 206：从 Content-Range（"bytes 0-2097151/12345"）提取 total
      const m = resp.headers.get('Content-Range')?.match(/\/(\d+)$/)
      if (m) total = Number(m[1])
      else if (buf.length < CHUNK_BYTES) break // 无 total 且不足一块 = 已是全部
    }
  }
  const out = new Uint8Array(received)
  let off = 0
  for (const c of chunks) {
    out.set(c, off)
    off += c.length
  }
  return out.buffer as ArrayBuffer
}

/** 分析某曲目的响度并写入曲库；失败抛错（调用方提示）。 */
export async function analyzeLoudness(trackId: number): Promise<LoudnessResult> {
  const url = await api.getStreamUrl(trackId)
  const buf = await fetchAllBytes(url)

  const ctx = new AudioContext({ sampleRate: DECODE_SAMPLE_RATE })
  let audio: AudioBuffer
  try {
    audio = await ctx.decodeAudioData(buf)
  } finally {
    void ctx.close().catch(() => {})
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
