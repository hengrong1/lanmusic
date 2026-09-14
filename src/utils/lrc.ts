import type { QrcLine, QrcWord } from '@/types'

export interface LrcLine {
  /** 秒 */
  time: number
  text: string
}

const TIME_TAG = /\[(\d{1,3}):(\d{1,2})(?:[.:](\d{1,3}))?\]/g

/**
 * 解析 LRC 歌词。支持多时间标签 `[00:12.5][01:20.0]歌词`。
 * 返回 synced=false 表示纯文本歌词（无时间轴）。
 */
export function parseLrc(raw: string): { lines: LrcLine[]; synced: boolean } {
  const lines: LrcLine[] = []
  for (const rawLine of raw.split(/\r?\n/)) {
    const tags = [...rawLine.matchAll(TIME_TAG)]
    if (!tags.length) continue
    const text = rawLine.replace(TIME_TAG, '').trim()
    for (const m of tags) {
      const mm = Number(m[1])
      const ss = Number(m[2])
      const frac = m[3] ? Number(`0.${m[3]}`) : 0
      if (Number.isNaN(mm) || Number.isNaN(ss)) continue
      lines.push({ time: mm * 60 + ss + frac, text })
    }
  }
  if (!lines.length) return { lines: [], synced: false }
  lines.sort((a, b) => a.time - b.time)
  // 折叠连续的间奏占位行：多个连续无文本行只保留第一个（前奏/间奏常是十几个空时间戳）
  const collapsed: LrcLine[] = []
  for (const line of lines) {
    const last = collapsed[collapsed.length - 1]
    if (!line.text && last && !last.text) continue
    collapsed.push(line)
  }
  return { lines: collapsed, synced: true }
}

/** 二分查找当前播放行 */
export function activeLineIndex(lines: LrcLine[], position: number): number {
  let lo = 0
  let hi = lines.length - 1
  let ans = -1
  const t = position + 0.3
  while (lo <= hi) {
    const mid = (lo + hi) >> 1
    if (lines[mid].time <= t) {
      ans = mid
      lo = mid + 1
    } else {
      hi = mid - 1
    }
  }
  return ans
}

/** 纯文本歌词按行拆分（去空行） */
export function plainLines(raw: string): string[] {
  return raw
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter((l) => l.length > 0)
}

/** A2 / Enhanced LRC 的字级标签（尖括号）：`<mm:ss.xx>` */
const WORD_TIME_TAG = /<(\d{1,3}):(\d{1,2})(?:[.:](\d{1,3}))?>/g
/** 元数据标签行（`[ti:]` / `[ar:]` / `[al:]` / `[by:]` / `[offset:]` 等），不产出歌词行 */
const META_TAG = /^\s*\[[a-zA-Z]/

/** 时间标签匹配结果 → 毫秒 */
function tagToMs(m: RegExpMatchArray): number {
  const mm = Number(m[1])
  const ss = Number(m[2])
  const frac = m[3] ? Number(`0.${m[3]}`) : 0
  if (Number.isNaN(mm) || Number.isNaN(ss)) return 0
  return Math.round((mm * 60 + ss + frac) * 1000)
}

/** 词段：起点（毫秒）+ 文本（可含空格，不 trim——逐字格式里空格也是独立一段） */
interface WordSeg {
  start: number
  text: string
}

/** 词段 → QrcLine（每段终点 = 下一段起点；末段用行结束标记或自身，稍后统一修正） */
function toQrcLine(segs: WordSeg[], endHint?: number): QrcLine {
  const words: QrcWord[] = segs.map((s, i) => ({
    word: s.text,
    startTime: s.start,
    endTime: segs[i + 1] ? segs[i + 1].start : endHint ?? s.start,
  }))
  return {
    startTime: words[0].startTime,
    endTime: words[words.length - 1].endTime,
    text: words.map((w) => w.word).join(''),
    words,
  }
}

/** A2 尖括号写法：`<mm:ss.xx>词` 序列 → 词段；标记前的裸文本归到行首时间（少见但存在） */
function splitAngleWords(src: string, fallbackStart: number): WordSeg[] {
  const marks = [...src.matchAll(new RegExp(WORD_TIME_TAG.source, 'g'))]
  const words: WordSeg[] = []
  const head = src.slice(0, marks[0]?.index ?? src.length).trim()
  if (head) words.push({ start: fallbackStart, text: head })
  marks.forEach((m, i) => {
    const text = src.slice(
      (m.index ?? 0) + m[0].length,
      i + 1 < marks.length ? (marks[i + 1].index ?? src.length) : src.length,
    )
    if (text) words.push({ start: tagToMs(m), text })
  })
  return words
}

/**
 * 逐字歌词解析（一个入口兼容多种字级时间戳写法），产出与 QRC 同构的数据（毫秒），
 * 直接复用现有逐字渲染链路（歌词面板 / 播放条歌词 / 桌面歌词）。
 *
 * 支持的写法：
 * ① A2 / Enhanced LRC：`[00:12.00]<00:12.00>你<00:12.35>好<00:13.10>世界`
 * ② 多标签逐字（每段前带同级方括号时间戳，常见于逐字 LRC 导出）：
 *    `[00:00.000]身[00:00.582]骑[00:01.164]白[00:02.328] [00:02.910]-[00:09.312]`
 * ③ 混合：同一文件里部分行逐字、部分行整行（整行仍按行显示，不做伪逐字）
 *
 * 其它兼容：
 * - 元数据标签忽略；`[offset:±ms]` 作为整曲时间偏移生效
 * - 行尾「有标签无文本」（如样本结尾的 `[00:09.312]`）作为该行结束时间，不当成一段
 * - 行中/行首的空标签（`[00:12][01:20]同一句`）视为重复行：内容整体平移克隆，各产一行
 * - 全曲无任何可识别字级标记时返回 null（交由 parseLrc 按行处理）
 */
export function parseWordLrc(raw: string): QrcLine[] | null {
  if (!raw || !raw.includes('[')) return null
  const offsetMs = Number(raw.match(/\[offset:\s*([+-]?\d+)\s*\]/i)?.[1] ?? 0) || 0
  const out: QrcLine[] = []
  let sawWord = false

  for (const rawLine of raw.split(/\r?\n/)) {
    if (META_TAG.test(rawLine)) continue
    // 每行新建正则实例：模块级 g 正则在 replace/matchAll 交替时会互相影响 lastIndex
    const lineTags = [...rawLine.matchAll(new RegExp(TIME_TAG.source, 'g'))]
    if (!lineTags.length) continue
    // 每个时间标签 + 其后文本（到下一个标签前）即一段。注意：逐字格式里空格也是独立一段，
    // 不 trim，否则会丢掉句读之间的间隔。
    const segs: WordSeg[] = lineTags.map((m, i) => ({
      start: tagToMs(m),
      text: rawLine.slice(
        (m.index ?? 0) + m[0].length,
        i + 1 < lineTags.length ? (lineTags[i + 1].index ?? rawLine.length) : rawLine.length,
      ),
    }))
    // 行尾的空文本时间戳 = 行结束标记（多标签逐字格式常见），取出后从段落中移除
    const tailMs =
      segs.length > 1 && segs[segs.length - 1].text === '' ? segs[segs.length - 1].start : undefined
    const body = tailMs === undefined ? segs : segs.slice(0, -1)
    if (!body.length) continue

    // 首个有文本的段之前的空标签 = 「同一句歌词的重复时间点」（`[00:01][00:20]同一句`）。
    // 内容段作为模板，为每个前导标签各克隆一份（时间整体平移），沿用 parseLrc 的语义。
    const firstIdx = body.findIndex((s) => s.text !== '')
    if (firstIdx < 0) {
      // 整行只有空时间戳（前奏/间奏）：留一个锚点行，保证行下标与逐字数据一一对应
      out.push({ startTime: body[0].start, endTime: body[0].start, text: '', words: [] })
      continue
    }
    const leads = body.slice(0, firstIdx)
    const core = body.slice(firstIdx)

    // 核心段 → 词段：① A2 尖括号 ② 多段方括号逐字
    let words: WordSeg[] | null = null
    let base = core[0].start
    if (core.length === 1 && core[0].text.includes('<')) {
      words = splitAngleWords(core[0].text, core[0].start)
      if (!words.length) words = null
      // A2 的字级时间戳是相对「行内第一个标签」写的，所以克隆的基准取 body[0] 而非核心段
      else base = body[0].start
    } else if (core.length >= 2) {
      words = core
    }
    if (words) sawWord = true

    if (words) {
      const starts = [...new Set([core[0].start, ...leads.map((l) => l.start)])]
      for (const st of starts) {
        const shift = st - base
        const shifted = shift ? words.map((w) => ({ start: w.start + shift, text: w.text })) : words
        out.push(toQrcLine(shifted, tailMs === undefined ? undefined : tailMs + shift))
      }
      continue
    }

    // 单段纯文本 → 行级（同样是「模板 + 克隆」，只是不带字级时间）
    for (const s of [core[0], ...leads]) {
      out.push({
        startTime: s.start,
        endTime: s.start,
        text: core[0].text.trim(),
        words: [{ word: core[0].text.trim(), startTime: s.start, endTime: s.start }],
      })
    }
  }

  if (!sawWord || !out.length) return null
  out.sort((a, b) => a.startTime - b.startTime)

  // 与 parseLrc / Rust QRC 同口径：折叠连续间奏占位行（前奏常是一串空时间戳）
  const collapsed = out.filter((l, i) => !(l.text === '' && out[i - 1]?.text === ''))
  out.length = 0
  out.push(...collapsed)

  if (offsetMs) {
    for (const line of out) {
      line.startTime += offsetMs
      line.endTime += offsetMs
      for (const w of line.words) {
        w.startTime += offsetMs
        w.endTime += offsetMs
      }
    }
  }

  // 收尾修正终止时间：染色进度按 (now - start) / (end - start) 计算，
  // 分母为 0 会让整段瞬间点亮，故末段与间奏行都要有合理区间
  for (let i = 0; i < out.length; i++) {
    const line = out[i]
    const nextStart = i + 1 < out.length ? out[i + 1].startTime : line.startTime + 4000
    if (!line.words.length) {
      line.endTime = Math.max(nextStart, line.startTime + 200)
      continue
    }
    const last = line.words[line.words.length - 1]
    if (last.endTime <= last.startTime) {
      last.endTime = Math.max(nextStart, last.startTime + 200)
    }
    line.endTime = last.endTime
  }
  return out
}
