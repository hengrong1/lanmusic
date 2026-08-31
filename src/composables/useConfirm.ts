import { ref } from 'vue'

export interface ConfirmOptions {
  title: string
  message: string
  danger?: boolean
  confirmText?: string
  cancelText?: string
}

interface ConfirmState {
  open: boolean
  title: string
  message: string
  danger: boolean
  confirmText: string
  cancelText: string
  resolve: ((v: boolean) => void) | null
}

const state = ref<ConfirmState>({
  open: false,
  title: '',
  message: '',
  danger: false,
  confirmText: '确定',
  cancelText: '取消',
  resolve: null,
})

/** 应用内确认弹窗（替代原生 ask()，保证中文界面） */
export function confirmDialog(opts: ConfirmOptions): Promise<boolean> {
  return new Promise<boolean>((resolve) => {
    state.value = {
      open: true,
      title: opts.title,
      message: opts.message,
      danger: opts.danger ?? false,
      confirmText: opts.confirmText ?? '确定',
      cancelText: opts.cancelText ?? '取消',
      resolve,
    }
  })
}

export function useConfirmState() {
  function answer(v: boolean) {
    state.value.resolve?.(v)
    state.value.open = false
  }
  return { state, answer }
}
