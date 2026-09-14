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

/** 增强版 LRC 的字级时间标签：`<mm:ss.xx>` / `<mm:ss.xxx>`（Enhanced LRC / A2 扩展） */
const WORD_TIME_TAG = /<(\d{1,3}):(\d{1,2})(?:[.:](\d{1,3}))?>/g

/** 时间标签匹配结果 → 毫秒 */
function tagToMs(m: RegExpMatchArray): number {
  const mm = Number(m[1])
  const ss = Number(m[2])
  const frac = m[3] ? Number(`0.${m[3]}`) : 0
  if (Number.isNaN(mm) || Number.isNaN(ss)) return 0
  return Math.round((mm * 60 + ss + frac) * 1000)
}

/**
 * 解析「增强版 LRC」（Enhanced LRC / A2 扩展）：行内含 `<mm:ss.xx>` 字级时间戳，
 * 产出与 QRC 同构的逐字数据（毫秒），直接复用现有逐字渲染链路
 * （歌词面板 / 播放条歌词 / 桌面歌词都按 `words[].word` + 起止时间做渐变点亮）。
 *
 * 示例：`[00:12.00]<00:12.00>你<00:12.35>好<00:13.10>世界`
 * - 行级 `[...]` 决定行起始时间（与普通 LRC 相同；同行多标签各产一行，适配重复副歌）
 * - 行内 `<...>` 给每个字/词的起点，终点为下一标记；行末词延伸到下一行起点
 * - 首个 `<...>` 之前的文本（少见）并入行首，起点取行时间
 * - 全曲没有任何 `<...>` 标记时返回 null（交由 parseLrc 按行处理）
 */
export function parseEnhancedLrc(raw: string): QrcLine[] | null {
  if (!raw || !raw.includes('<')) return null
  const out: QrcLine[] = []
  let hasWordTag = false

  for (const rawLine of raw.split(/\r?\n/)) {
    // 每行新建正则实例：模块级 g 正则在 replace/matchAll 交替使用时会互相影响 lastIndex
    const lineTags = [...rawLine.matchAll(new RegExp(TIME_TAG.source, 'g'))]
    if (!lineTags.length) continue
    const body = rawLine.replace(new RegExp(TIME_TAG.source, 'g'), '')

    // 词段：{ 起点毫秒, 文本 }；行首无标记文本（少见）先以 0 占位，稍后按行时间填
    const segs: { start: number; text: string }[] = []
    const marks = [...body.matchAll(new RegExp(WORD_TIME_TAG.source, 'g'))]
    if (marks.length) hasWordTag = true
    const head = body.slice(0, marks[0]?.index ?? body.length).trim()
    if (head) segs.push({ start: 0, text: head })
    marks.forEach((m, i) => {
      const from = (m.index ?? 0) + m[0].length
      const to = i + 1 < marks.length ? (marks[i + 1].index ?? body.length) : body.length
      const text = body.slice(from, to)
      if (text) segs.push({ start: tagToMs(m), text })
    })

    if (!segs.length) {
      // 纯时间戳行（间奏锚点）：保留空行，保证行下标与滚动定位一致
      for (const lt of lineTags) {
        const ms = tagToMs(lt)
        out.push({ startTime: ms, endTime: ms, text: '', words: [] })
      }
      continue
    }

    for (const lt of lineTags) {
      const lineMs = tagToMs(lt)
      // 同行多标签（重复副歌）：整行词时间平移到该行标签起点，否则第二份会沿用
      // 第一份的绝对词时间，行级滚动与逐字点亮就对不上了
      const shift = lineMs - (segs[0].start > 0 ? segs[0].start : lineMs)
      const words: QrcWord[] = segs.map((s) => ({
        word: s.text,
        startTime: (s.start > 0 ? s.start : lineMs) + shift,
        endTime: 0,
      }))
      // 每段终点 = 下一段起点；末段先截断到自身，稍后统一修正
      for (let i = 0; i < words.length; i++) {
        words[i].endTime = words[i + 1] ? words[i + 1].startTime : words[i].startTime
      }
      out.push({
        startTime: words[0].startTime,
        endTime: words[words.length - 1].endTime,
        text: words.map((w) => w.word).join(''),
        words,
      })
    }
  }

  if (!hasWordTag || !out.length) return null
  out.sort((a, b) => a.startTime - b.startTime)

  // 收尾修正终止时间：染色进度按 (now - start) / (end - start) 计算，
  // 分母为 0 会让整段瞬间点亮，故末段/间奏行都要有合理区间
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
