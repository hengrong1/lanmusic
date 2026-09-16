import type { QrcLine, QrcWord } from '@/types'

export interface LrcLine {
  /** 秒 */
  time: number
  text: string
  /** 行尾时间戳标记的本句结束时间（秒）：`[00:26.76]我站在屋顶[00:32.45]` → end=32.45；无则留空 */
  end?: number
  /** 音译（罗马字）副行：显示在原文上方；无则留空 */
  transliteration?: string
  /** 译文副行：显示在原文下方；无则留空 */
  translation?: string
}

const TIME_TAG = /\[(\d{1,3}):(\d{1,2})(?:[.:](\d{1,3}))?\]/g

/**
 * 译文行与原文行的起始时间容差（毫秒）：双语歌词的两行是同一起点，
 * 允许各来源之间几十毫秒的偏差，但不能大到把「紧接着的下一句」吞掉。
 */
const TRANSLATION_TOLERANCE_MS = 120

/**
 * 折叠「译文行」（双语歌词的第二行）。
 *
 * 判定：某行紧跟在上一行之后、起始时间几乎相同、自身不是逐字行（译文一般只有整行文本）、
 * 且文本与上一行不同 → 认作上一行的译文，写入 `translation` 并从行序列中移除。
 *
 * 不做这步的话，译文会作为一行独立歌词插进列表：滚动到该处会闪出一行译文，
 * 高亮也会在原文/译文之间来回跳（双语 LRC 与 QRC 都能碰到）。
 *
 * 行级歌词（本函数）**不识别音译**：没有字级时间轴时，一行拉丁文本无法区分
 * 「罗马字音译」与「英文翻译」，误判会让译文开关管错行。音译只在逐字链路识别。
 */
function foldLrcTranslations(lines: LrcLine[]): LrcLine[] {
  const out: LrcLine[] = []
  for (const line of lines) {
    const prev = out[out.length - 1]
    const text = line.text.trim()
    if (
      prev &&
      text &&
      !prev.translation &&
      prev.text.trim() &&
      text !== prev.text.trim() &&
      Math.abs(line.time - prev.time) * 1000 <= TRANSLATION_TOLERANCE_MS
    ) {
      prev.translation = text
      continue
    }
    out.push(line)
  }
  return out
}

/** 单个时间标签 → 秒；非法返回 NaN */
function tagTime(m: RegExpMatchArray): number {
  const mm = Number(m[1])
  const ss = Number(m[2])
  const frac = m[3] ? Number(`0.${m[3]}`) : 0
  return Number.isNaN(mm) || Number.isNaN(ss) ? NaN : mm * 60 + ss + frac
}

/**
 * 解析 LRC 歌词。支持多时间标签 `[00:12.5][01:20.0]歌词`（同一文本多次演唱）。
 * 返回 synced=false 表示纯文本歌词（无时间轴）。
 */
export function parseLrc(raw: string): { lines: LrcLine[]; synced: boolean } {
  const lines: LrcLine[] = []
  for (const rawLine of raw.split(/\r?\n/)) {
    const tags = [...rawLine.matchAll(TIME_TAG)]
    if (!tags.length) continue
    const text = rawLine.replace(TIME_TAG, '').trim()
    // 行尾时间戳行（[start]文本[end]）：最后一个标签之后没有文本 → 它标记本句的
    // 结束时间，而不是「同一文本的第二次演唱」。只产出一行（带 end）——否则同文本
    // 副本行会与下一句同起点，被 foldLrcTranslations 误判成下一句的译文（亮起
    // 「译」图标）、还会造成歌词重复显示。
    const lastTag = tags[tags.length - 1]
    if (tags.length >= 2 && !rawLine.slice(lastTag.index! + lastTag[0].length).trim()) {
      const first = tags[0]
      const between = rawLine
        .slice(first.index! + first[0].length, lastTag.index)
        .replace(TIME_TAG, '')
        .trim()
      const t0 = tagTime(first)
      const t1 = tagTime(lastTag)
      if (between && !Number.isNaN(t0) && !Number.isNaN(t1) && t1 > t0) {
        lines.push({ time: t0, end: t1, text: between })
        continue
      }
    }
    for (const m of tags) {
      const time = tagTime(m)
      if (Number.isNaN(time)) continue
      lines.push({ time, text })
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
  return { lines: foldLrcTranslations(collapsed), synced: true }
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

/** 中日韩文字（汉字 / 假名 / 谚文）：音译行只有拉丁字母，据此把音译行与原文行区分开 */
const CJK_RE = /[\u3040-\u30ff\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff\uac00-\ud7af]/

function hasCjk(text: string): boolean {
  return CJK_RE.test(text)
}

/**
 * 折叠逐字歌词里的「副行」——同一句的读音转写（音译）与译文。
 * 界面按歌词文件的行序排布：音译在上、原文居中、译文在下。
 *
 * 判定（均要求与上一行起始时间几乎相同、文本不同）：
 * - 自身不是逐字行（整行文本）→ 译文（translation）；
 * - 自身与上一行都是逐字行、且两者「是否含中日韩文字」不同 → 全拉丁的那行是音译
 *   （transliteration）；若音译写在原文之前，把音译整行挪到原文上，
 *   让原文行继续占据序列里的位置。
 *
 * 副行不折叠的话，它们会各占一行歌词：滚动、高亮与左下角跳转按钮都会出现重复行。
 * Rust 侧 QRC 解析结果也走这里，这样 lyricsWordLines 与 lyricsLines 行数始终一一对应。
 */
export function foldQrcSubLines(lines: QrcLine[]): QrcLine[] {
  const out: QrcLine[] = []
  for (const line of lines) {
    const prev = out[out.length - 1]
    const text = line.text.trim()
    if (
      prev &&
      text &&
      prev.text.trim() &&
      text !== prev.text.trim() &&
      Math.abs(line.startTime - prev.startTime) <= TRANSLATION_TOLERANCE_MS
    ) {
      // ① 整行文本 → 译文（已有译文时不再覆盖：宁可多留一行，也不吞掉内容）
      if (line.words.length <= 1) {
        if (!prev.translation) {
          prev.translation = text
          continue
        }
      } else if (prev.words.length > 1 && hasCjk(prev.text) !== hasCjk(text)) {
        // ② 两条逐字行同起点、其中一行全是拉丁字母 → 那是音译行
        if (hasCjk(text)) {
          out.pop() // 音译在前、原文在后：音译整行挪到原文上
          line.transliteration = prev.text.trim()
          out.push(line)
        } else {
          prev.transliteration = text
        }
        continue
      }
    }
    out.push(line)
  }
  return out
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

  // 与 parseLrc / Rust QRC 同口径：折叠连续间奏占位行（前奏常是一串空时间戳），
  // 再把同起点的副行（音译 / 译文）并进原文行
  const collapsed = out.filter((l, i) => !(l.text === '' && out[i - 1]?.text === ''))
  out.length = 0
  out.push(...foldQrcSubLines(collapsed))

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
