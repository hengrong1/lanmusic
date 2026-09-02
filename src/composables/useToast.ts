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
}

export function useToast() {
  return { toasts }
}
