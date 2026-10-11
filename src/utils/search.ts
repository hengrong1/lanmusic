import type { Track } from '@/types'

/**
 * 歌词是否是唯一命中字段（标题/艺术家/专辑/文件名任一命中都以元数据为主）：
 * 「歌词」徽标与歌词行只在歌词独命中时展示；混合结果里只命中歌词的曲目仍然展示。
 */

export function isLyricOnlyHit(t: Track): boolean {
  const mf = t.matchedFields
  return !!mf && mf.length > 0 && mf.every((f) => f === 'lyrics')
}
