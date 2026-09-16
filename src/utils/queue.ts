import type { Track } from '@/types'

/**
 * 「下一首播放」的插入算法（纯函数，便于单测）。
 *
 * 语义：把 track 放到当前曲目之后，且队列里**不产生重复条目**——
 * - 队列里没有这首 → 插到 index + 1（inserted）；
 * - 已经在 index + 1（本来就是下一首）→ 原样不动（alreadyNext），连点只提示不叠加；
 * - 出现在队列其它位置 → 从原位置摘出来再放到 index + 1（moved）；
 * - 就是当前正在播的这首 → 什么都不做（noop）。
 *
 * 摘掉当前曲目**之前**的条目时，当前曲目的下标要跟着前移（index - 1），
 * 否则 current 会指向下一首、正在播的歌被"跳"掉。
 *
 * 注意：index < 0（队列为空 / 还没开始播）由调用方另行处理，本函数不做兜底。
 */
export type NextInsertAction = 'inserted' | 'moved' | 'alreadyNext' | 'noop'

export interface NextInsertResult {
  /** 新的队列（alreadyNext / noop 时为原数组） */
  queue: Track[]
  /** 当前播放曲目在新队列里的下标 */
  index: number
  action: NextInsertAction
}

export function insertNextAfter(queue: Track[], index: number, track: Track): NextInsertResult {
  if (index >= 0 && queue[index]?.id === track.id) return { queue, index, action: 'noop' }

  const exist = queue.findIndex((x) => x.id === track.id)
  if (exist === index + 1) return { queue, index, action: 'alreadyNext' }

  const next = [...queue]
  if (exist >= 0) {
    // 已在队列里：先摘出来，再插到当前曲目之后（去重的关键 —— 不新增条目）
    next.splice(exist, 1)
    const at = exist < index ? index : index + 1
    next.splice(at, 0, track)
    return { queue: next, index: exist < index ? index - 1 : index, action: 'moved' }
  }

  next.splice(index + 1, 0, track)
  return { queue: next, index, action: 'inserted' }
}
