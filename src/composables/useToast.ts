import { ref } from 'vue'

export interface ToastItem {
  id: number
  text: string
  kind: 'info' | 'error'
}

const toasts = ref<ToastItem[]>([])
let seq = 0

export function toast(text: string, kind: 'info' | 'error' = 'info') {
  const id = ++seq
  toasts.value.push({ id, text, kind })
  setTimeout(() => {
    toasts.value = toasts.value.filter((t) => t.id !== id)
  }, 3500)
}

export function useToast() {
  return { toasts }
}
