/**
 * 弹窗偏好：背景模糊度 + 是否可拖动（设置 → 外观）。
 * 各弹窗打开时读取一次即可（非响应式，设置变更对下一个弹窗生效）。
 */

const BLUR_KEY = 'lm.dialogBlur'
const DRAG_KEY = 'lm.dialogDrag'

export type DialogBlur = 'none' | 'sm' | 'md' | 'lg'

const BLUR_CLASSES: Record<DialogBlur, string> = {
  none: '',
  sm: 'backdrop-blur-sm',
  md: 'backdrop-blur-md',
  lg: 'backdrop-blur-lg',
}

/** 读取模糊度设置（存档损坏/越界回退 sm） */
export function dialogBlur(): DialogBlur {
  const v = localStorage.getItem(BLUR_KEY)
  return v === 'none' || v === 'md' || v === 'lg' ? v : 'sm'
}

/** 弹窗是否可拖动（默认开启） */
export function dialogDraggable(): boolean {
  return localStorage.getItem(DRAG_KEY) !== '0'
}

/** 统一的弹窗遮罩类：居中布局 + 半透明底 + 按设置的模糊度（z 由调用方传入） */
export function dialogOverlayClass(z = 'z-[70]'): string {
  return `fixed inset-0 ${z} flex items-center justify-center bg-black/40 ${BLUR_CLASSES[dialogBlur()]}`
}

/** 弹窗面板的出入场动画（与遮罩淡入淡出叠加） */
export const dialogPanelTransition = {
  enterActiveClass: 'transition duration-200 ease-out',
  enterFromClass: 'opacity-0 scale-95',
  enterToClass: 'opacity-100 scale-100',
  leaveActiveClass: 'transition duration-150 ease-in',
  leaveFromClass: 'opacity-100 scale-100',
  leaveToClass: 'opacity-0 scale-95',
}
