import { ref } from 'vue'

export interface ToastItem {
  id: number
  text: string
  kind: 'info' | 'error'
  /** 可选去重键：相同 key 的提示不叠加，原地更新文字并重置计时 */
  key?: string
}

const toasts = ref<ToastItem[]>([])
let seq = 0
const timers = new Map<number, ReturnType<typeof setTimeout>>()

/** 同屏上限：连续操作（如连改多个设置）时新提示不排队，立刻收掉最旧的，气泡不再堆积 */
const MAX_VISIBLE = 3

/** 移除最旧的一条（连同它的计时器）；TransitionGroup 的 leave 动画自动接管退场 */
function dropOldest() {
  const oldest = toasts.value.shift()
  if (!oldest) return
  const timer = timers.get(oldest.id)
  if (timer) clearTimeout(timer)
  timers.delete(oldest.id)
}

function scheduleRemove(id: number) {
  timers.set(
    id,
    setTimeout(() => {
      toasts.value = toasts.value.filter((t) => t.id !== id)
      timers.delete(id)
    }, 3500),
  )
}

export function toast(text: string, kind: 'info' | 'error' = 'info', key?: string) {
  if (key) {
    const existing = toasts.value.find((t) => t.key === key)
    if (existing) {
      existing.text = text
      existing.kind = kind
      const timer = timers.get(existing.id)
      if (timer) clearTimeout(timer)
      scheduleRemove(existing.id)
      return
    }
  }
  const id = ++seq
  toasts.value.push({ id, text, kind, key })
  scheduleRemove(id)
  while (toasts.value.length > MAX_VISIBLE) dropOldest()
}

export function useToast() {
  return { toasts }
}
