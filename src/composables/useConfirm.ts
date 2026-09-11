import { ref } from 'vue'
import { t } from '@/i18n/translate'

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
  confirmText: t('common.ok'),
  cancelText: t('common.cancel'),
  resolve: null,
})

/** 应用内确认弹窗（替代原生 ask()，保证中文界面）。
 * 单例弹窗同一时刻只能显示一个：新确认覆盖旧确认时，先用 false 结算旧调用，
 * 否则上一个 await 会永远悬挂。 */
export function confirmDialog(opts: ConfirmOptions): Promise<boolean> {
  // 有未结算的旧确认：视为取消，避免调用方永久挂起
  state.value.resolve?.(false)
  return new Promise<boolean>((resolve) => {
    state.value = {
      open: true,
      title: opts.title,
      message: opts.message,
      danger: opts.danger ?? false,
      confirmText: opts.confirmText ?? t('common.ok'),
      cancelText: opts.cancelText ?? t('common.cancel'),
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
