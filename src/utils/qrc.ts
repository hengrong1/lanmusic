import type { LrcLine } from './lrc'
import type { QrcLine } from '@/types'

/**
 * QRC 逐字歌词（QQ音乐格式）：解密与词级解析都在 Rust 侧（src-tauri/src/qrc.rs，
 * 经 api.parseQrc 调用），前端只负责把词级时间轴接到滚动定位与逐字高亮渲染。
 */

/** 是否为加密/二进制内容：整文件是十六进制串（加密 QRC 归一载体，Rust 未能解密时提示不支持） */
export function looksLikeHexQrc(raw: string): boolean {
  const t = raw.trim()
  return t.length >= 200 && /^[0-9a-fA-F\s]+$/.test(t)
}

/** 内容是否像二进制/加密数据（控制符与替换符占比高），不应作为歌词文本展示 */
export function looksBinaryish(raw: string): boolean {
  if (!raw.length) return false
  let bad = 0
  for (const ch of raw) {
    const c = ch.codePointAt(0) ?? 0
    if ((c < 0x20 && c !== 9 && c !== 10 && c !== 13) || c === 0xfffd) bad++
  }
  return bad / raw.length > 0.03
}

/** QRC 行 → 行级时间轴（滚动定位 / 点击跳转 / 桌面歌词复用现有链路）。
 * 间奏空行与副行（音译 / 译文）已折叠，行下标与逐字数据一一对应；两条副行原样透传给界面。 */
export function qrcToLrcLines(lines: QrcLine[]): LrcLine[] {
  return lines.map((l) => ({
    time: l.startTime / 1000,
    text: l.text,
    transliteration: l.transliteration,
    translation: l.translation,
  }))
}
