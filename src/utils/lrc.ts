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
