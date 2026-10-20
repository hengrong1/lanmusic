import { api } from '@/api/commands'

// 响度分析（ReplayGain 音量归一化）：
// 用 Web Audio 解码音频前缀，统计 RMS 与峰值，据此给出建议增益并落库。
// 之所以放在前端：Rust 侧的 symphonia 仅 macOS 编译（见 src-tauri/Cargo.toml），
// 而 WebView 的 Web Audio 在所有平台都能解码（mp3/flac/wav/m4a/ogg 视平台编解码支持）。
//
// 说明：实现的是「门限 RMS 归一化」而非严格 EBU R128（LUFS），对本地曲库拉齐音量已足够。
// 目标 RMS -18 dBFS，并按峰值（保留余量）限制增益上限，避免削波。
//
// 取数策略（2026-10-20 修正）：只统计前 MAX_ANALYZE_SECONDS 秒。
// 关键坑：Rust 端 serve_file_response 对**任意** Range（含闭区间 bytes=0-N）一律按 CHUNK(2MB) 封顶，
// 所以「请求一次 bytes=0-cap」根本拿不到 cap 字节。旧实现因此只解码了前 2MB——高码率 FLAC
// 仅几秒的前奏/淡入，RMS 与峰值被算得极低（实测某曲 peak 0.01，真实值 0.92），
// 于是给出 +24dB 的疯狂增益，播放时削波成「又响又闷」。
// 现在按 SERVE_CHUNK 分块循环续传，直到拿满计划字节或文件结束；解码后再校验时长，
// 前缀不足一律判失败（不写库），宁可暂时不归一化，也不能写一个错误增益。

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
/** 增益上下限（dB）。上限收紧到 +12：RS 标准实践中极少需要更大正增益，
 *  而 +24 一旦数据有误就是把歌推爆（听感为「非常大而且闷」的削波失真） */
const GAIN_CLAMP_DB = 12
/** 单曲最多分析的秒数：门限 RMS 用 30 秒已足够稳定，拉取与解码量也小一个量级 */
const MAX_ANALYZE_SECONDS = 30
/** 至少要分析到的秒数（不足即判前缀不完整）。取 min(曲长, 此值) 作为验收下限 */
const MIN_ANALYZE_SECONDS = 20
/** 拉取字节时的余量系数：VBR / 元数据块会让前 N 秒的字节数略高于平均值 */
const PREFIX_SLACK = 1.15
/** 解码采样率：仅用于统计响度，降低采样率可显著减少内存占用 */
const DECODE_SAMPLE_RATE = 24_000
/** Rust 端单次 Range 响应的封顶字节数（见 src-tauri/src/scheme.rs 的 CHUNK）：
 *  续传必须按此步长循环，不能指望一次请求拿满 */
const SERVE_CHUNK = 2 * 1024 * 1024
/** 单文件拉取硬上限：防止远程源不返回 total 时无限续传（也覆盖回退整首的场景） */
const MAX_PREFIX_BYTES = 64 * 1024 * 1024
/** 总大小未知时的码率假设（kbps）：按 24bit/48kHz 无损的常见上限估。
 *  宁可多读几 MB（局域网毫秒级），也不要因低估只解出前奏——解码后还有时长校验兜底 */
const UNKNOWN_BITRATE_KBPS = 1200
/** 单次 fetch 拉取超时（ms）：音频元素正在流式播放时，同一 host 的连接会被占用，
 *  偶发单块读取可能卡住十几秒，因此给到 60s 并允许重试一次 */
const FETCH_TIMEOUT_MS = 60_000
/** 单块拉取的超时重试次数（只重试超时，HTTP 错误直接失败） */
const FETCH_RETRY = 1
/** 解码单首音频超时（ms）：WebView2 无音频设备时 decodeAudioData 可能永久挂起 */
const DECODE_TIMEOUT_MS = 90_000
/** 整次分析总超时（ms）：fetch + 解码 + 落库的总兜底 */
const OP_TIMEOUT_MS = 300_000
/** 帧长（秒）：门限判断的统计粒度 */
const FRAME_SECONDS = 0.05
/** 相对门限（dB）：低于本曲最响帧这么多的一律视为静音/间隙，不计入响度 */
const GATE_BELOW_LOUDEST_DB = 30
/** 绝对门限（dBFS）：低于此值一律忽略（数字静音/极低底噪） */
const ABSOLUTE_GATE_DB = -60

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

/** 单次带超时的字节拉取；返回响应体、状态码与文件总大小（从 Content-Range 解析）。
 *  超时会自动重试一次：播放中的 <audio> 与它抢同一个 host 的连接，偶发单块卡住
 *  十几秒很常见，重试通常立刻就能拿到。 */
async function fetchBytes(
  url: string,
  range: string | undefined,
  attempt = 0,
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
    if (ctrl.signal.aborted) {
      if (attempt < FETCH_RETRY) return fetchBytes(url, range, attempt + 1)
      throw new Error('音频拉取超时')
    }
    throw e
  } finally {
    clearTimeout(timer)
  }
}

/**
 * 计划拉取多少字节：按「总字节 / 总时长」得到的平均码率折算前 MAX_ANALYZE_SECONDS 秒。
 * 平均码率对 CBR/VBR 都成立，比标签里的 bitrate 可靠；total 未知（远程未回 /size）
 * 时按 320kbps 保守估算——估少了会被解码时长校验拦下，估多了只是多读几 MB。
 */
function planPrefixBytes(total: number, durationSec?: number | null): number {
  if (!Number.isFinite(total)) {
    const sec = Math.min(durationSec && durationSec > 0 ? durationSec : MAX_ANALYZE_SECONDS, MAX_ANALYZE_SECONDS)
    return Math.min(MAX_PREFIX_BYTES, Math.ceil(sec * UNKNOWN_BITRATE_KBPS * 125 * PREFIX_SLACK) + 512 * 1024)
  }
  if (!durationSec || durationSec <= 0) return Math.min(total, MAX_PREFIX_BYTES)
  const sec = Math.min(durationSec, MAX_ANALYZE_SECONDS)
  const need = Math.ceil((total / durationSec) * sec * PREFIX_SLACK) + 512 * 1024
  return Math.max(1, Math.min(total, MAX_PREFIX_BYTES, need))
}

/** 前缀缓冲：增量续传的累积状态 */
interface PrefixBuffer {
  chunks: Uint8Array[]
  /** 已拿到的字节数 */
  got: number
  /** 文件总字节数；服务端未回告时为 Infinity */
  total: number
  /** 已确认读到文件末尾 */
  eof: boolean
}

/** 开始拉取：第一块不带 Range（Rust 回 206 + 首块，小文件回 200 整首） */
async function startPrefix(url: string): Promise<PrefixBuffer | ArrayBuffer> {
  const first = await fetchBytes(url, undefined)
  // 200 = 服务端把整个文件一次性给了（小文件），无需续传
  if (first.status === 200) return first.buf.buffer as ArrayBuffer
  return { chunks: [first.buf], got: first.buf.length, total: first.total, eof: false }
}

/**
 * 续传直到拿满 cap 字节或确认到文件末尾。
 * Rust 端单次响应封顶 SERVE_CHUNK，所以必须按块循环；判断「到末尾」不能只看
 * 返回块小于 SERVE_CHUNK——请求区间本身可能就不足一块，那种情况是正常的。
 */
async function fillPrefix(url: string, st: PrefixBuffer, cap: number): Promise<void> {
  while (!st.eof && st.got < cap) {
    const want = cap - st.got
    const t0 = performance.now()
    const r = await fetchBytes(url, `bytes=${st.got}-${cap - 1}`)
    const cost = performance.now() - t0
    if (r.buf.length === 0) {
      st.eof = true
      break
    }
    // 实际返回少于「本轮理论最大值」= 服务端已给到文件末尾 (total 未知时的唯一判据)
    const short = r.buf.length < Math.min(SERVE_CHUNK, want)
    st.chunks.push(r.buf)
    st.got += r.buf.length
    // 慢分块留痕（正常局域网单块几十毫秒）：事后排查超时卡在哪一跳
    if (cost > 3000) {
      api
        .frontendLog(
          'warn',
          `[loudness] slow chunk got=${st.got} len=${r.buf.length} cap=${cap} cost=${cost.toFixed(0)}ms`,
        )
        .catch(() => {})
    }
    if (Number.isFinite(st.total) && st.got >= st.total) {
      st.eof = true
      break
    }
    if (short) {
      st.eof = true
      break
    }
  }
}

/** 拼接已拿到的字节。必须每次新建 ArrayBuffer：decodeAudioData 会 detach 传入的 buffer */
function joinPrefix(st: PrefixBuffer): ArrayBuffer {
  const out = new Uint8Array(st.got)
  let off = 0
  for (const c of st.chunks) {
    out.set(c, off)
    off += c.length
  }
  return out.buffer
}

/** 分析某曲目的响度并写入曲库；失败抛错（调用方提示）。
 * durationSec 提供时只拉前段字节，长曲目不再整首进内存或触发 "audio too large"。 */
export async function analyzeLoudness(
  trackId: number,
  durationSec?: number | null,
): Promise<LoudnessResult> {
  try {
    return await withTimeout(innerAnalyze(trackId, durationSec), OP_TIMEOUT_MS, '响度分析')
  } catch (e) {
    // 失败已由调用方 toast 告知用户，这里补持久化痕迹：release 版 WebView 无控制台，
    // catch 后只 toast 的错误不会触发全局兜底，日志文件是事后排查的唯一出口
    api
      .frontendLog('warn', `[loudness] analyze failed: trackId=${trackId} duration=${durationSec ?? '?'} err=${String(e)}`)
      .catch(() => {})
    throw e
  }
}

/** 同一曲目的并发分析请求复用一个任务：手动「分析」按钮与切歌自动分析共用此入口，
 *  避免对同一首歌同时跑两次解码（结果一致，纯粹浪费 CPU 与网络）。 */
const inflight = new Map<number, Promise<LoudnessResult>>()

export function analyzeTrackLoudness(
  trackId: number,
  durationSec?: number | null,
): Promise<LoudnessResult> {
  const running = inflight.get(trackId)
  if (running) return running
  const p = analyzeLoudness(trackId, durationSec).finally(() => {
    inflight.delete(trackId)
  })
  inflight.set(trackId, p)
  return p
}

/** analyzeLoudness 的实际实现；外层套总超时，确保任何环节挂起都能恢复。 */
async function innerAnalyze(trackId: number, durationSec?: number | null): Promise<LoudnessResult> {
  const url = await api.getStreamUrl(trackId)

  // 前缀完整性门槛：至少解出 min(曲长, MAX_ANALYZE_SECONDS, MIN_ANALYZE_SECONDS) 的 85%。
  // 截断的 FLAC/OGG 常常能「成功」解出前几秒（多为前奏/淡入），拿那几秒算响度会得出
  // 极偏乐观的增益——必须拒收，宁可这次不归一化，也不能写一个错误增益。
  const expected = Math.min(
    durationSec && durationSec > 0 ? durationSec : MAX_ANALYZE_SECONDS,
    MAX_ANALYZE_SECONDS,
  )
  const needSec = Math.min(expected, MIN_ANALYZE_SECONDS) * 0.85

  // 目标字节数：total 已知按「总字节/总时长」的平均码率算；total 未知（服务端没 expose
  // Content-Range）时按无损码率估。估少了不算错——下面解码后会校验时长，不够就翻倍再拉。
  const started = await startPrefix(url)
  let whole: ArrayBuffer | null = null
  let st: PrefixBuffer | null = null
  if (started instanceof ArrayBuffer) whole = started
  else st = started
  let cap = planPrefixBytes(st ? st.total : 0, durationSec)
  if (st) await fillPrefix(url, st, cap)

  const ctx = new AudioContext({ sampleRate: DECODE_SAMPLE_RATE })
  try {
    const decode = (b: ArrayBuffer) =>
      withTimeout(ctx.decodeAudioData(b), DECODE_TIMEOUT_MS, '音频解码')

    let audio: AudioBuffer | null = null
    let decodeErr: unknown = null
    let pending: ArrayBuffer | null = whole ?? joinPrefix(st!)

    // 「解码 → 时长不足 → 加量续传 → 再解码」最多三轮：覆盖码率估算偏低、
    // 以及必须拿到完整文件才能解码的容器（此前只回退一次、且不会继续加量）
    let round = 0
    while (pending) {
      try {
        audio = await decode(pending)
        decodeErr = null
      } catch (e) {
        audio = null
        decodeErr = e
        // release 版无控制台，落盘留痕
        api
          .frontendLog('warn', `[loudness] decode round ${round} failed: ${String(e)}`)
          .catch(() => {})
      }
      pending = null

      if (audio && audio.duration >= needSec) break
      // 整文件已到手 / 已到 EOF / 已到上限 / 轮次用尽：没有更多可拉
      if (whole || !st || st.eof || st.got >= MAX_PREFIX_BYTES || round >= 2) break

      round++
      const before = st.got
      cap = Math.min(MAX_PREFIX_BYTES, Math.max(cap * 2, st.got + 8 * 1024 * 1024))
      await fillPrefix(url, st, cap)
      if (st.got <= before) break // 没拉到新数据，再解一次也没意义
      pending = joinPrefix(st)
    }

    if (!audio) throw decodeErr ?? new Error('音频解码失败')
    if (audio.duration < needSec) {
      throw new Error(
        `音频前缀不完整（解码 ${audio.duration.toFixed(1)}s，至少需要 ${needSec.toFixed(0)}s）`,
      )
    }

    const { rmsDb, peak } = measure(audio)
    // 全静音/极低电平：无法得出有意义的增益，直接判失败（不写库、不改播放音量）
    if (peak < 0.0005 || rmsDb <= -70) throw new Error('音频几乎全静音，跳过分析')

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

/**
 * 从解码后的多声道采样统计 RMS（dBFS）与峰值（线性）。
 * 采用「分帧 + 相对门限」：先按 FRAME_SECONDS 算每帧 RMS，再只对高于
 * （本曲最响帧 - GATE_BELOW_LOUDEST_DB）且高于绝对门限的帧求功率平均。
 * 这样前奏/间奏的静音不会把整曲响度拉低——否则静音越长的歌越容易被推爆。
 */
function measure(audio: AudioBuffer): { rmsDb: number; peak: number } {
  const maxFrames = Math.min(audio.length, Math.floor(audio.sampleRate * MAX_ANALYZE_SECONDS))
  const frameLen = Math.max(1, Math.floor(audio.sampleRate * FRAME_SECONDS))
  const chCount = audio.numberOfChannels
  const chans: Float32Array[] = []
  for (let ch = 0; ch < chCount; ch++) chans.push(audio.getChannelData(ch))

  let peak = 0
  const frameSq: number[] = []
  const frameN: number[] = []

  for (let start = 0; start < maxFrames; start += frameLen) {
    const end = Math.min(start + frameLen, maxFrames)
    let sumSq = 0
    for (let ch = 0; ch < chCount; ch++) {
      const d = chans[ch]
      for (let i = start; i < end; i++) {
        const v = d[i]
        sumSq += v * v
        const a = v < 0 ? -v : v
        if (a > peak) peak = a
      }
    }
    frameSq.push(sumSq)
    frameN.push((end - start) * chCount)
  }

  const frameDb = frameSq.map((s, i) => {
    const n = frameN[i]
    if (n <= 0) return -120
    const rms = Math.sqrt(s / n)
    return rms > 0 ? 20 * Math.log10(rms) : -120
  })

  let loudest = -120
  for (const db of frameDb) if (db > loudest) loudest = db
  const gate = Math.max(ABSOLUTE_GATE_DB, loudest - GATE_BELOW_LOUDEST_DB)

  let gatedSq = 0
  let gatedN = 0
  for (let i = 0; i < frameDb.length; i++) {
    if (frameDb[i] > gate) {
      gatedSq += frameSq[i]
      gatedN += frameN[i]
    }
  }
  // 兜底：整段都没过门限（极安静的录音）→ 退回全段统计，仍交给上层静音判定
  if (gatedN === 0) {
    for (let i = 0; i < frameSq.length; i++) {
      gatedSq += frameSq[i]
      gatedN += frameN[i]
    }
  }

  const rms = gatedN > 0 ? Math.sqrt(gatedSq / gatedN) : 0
  return { rmsDb: rms > 0 ? 20 * Math.log10(rms) : -120, peak }
}
