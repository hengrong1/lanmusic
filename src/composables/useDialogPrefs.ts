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

/* ---------------------------------------------------------------
   层叠约定（新增弹窗 / 浮层前先看这里，别现填 z 值）
   ---------------------------------------------------------------
   z-10   顶栏与页面内浮标          z-[15] 播放页环境渐变        z-20 播放条
   z-30   进度条 / 批量操作条
   z-50   非模态浮层：右键菜单、搜索下拉、队列面板、MV 播放器、BaseDropdown 面板
   z-[70] 普通弹窗（DIALOG_Z：BaseModal / UpdateDialog / PlaylistEditDialog / TrackPicker）
   z-[80] 弹窗内或播放条上的下拉面板（BaseSelect 面板、歌手名单浮层）
   z-[90] 二次确认 / 关闭确认（PROMPT_Z）
   z-[100] Toast（要能压在弹窗之上，见 Toast.vue）
   z-[9999] tooltip
   同一层里谁压谁是「DOM 顺序」说了算 —— 而 Teleport 的顺序取决于**组件挂载顺序**，
   视图是导航后才挂载的，所以「谁后挂」并不等于「谁后开」，同层等于随机。
   凡是有可能叠在一起的两种东西，必须分到不同的层。 */
export const DIALOG_Z = 'z-[70]'

/** 二次确认 / 关闭确认层：**必须**高于 DIALOG_Z。
    踩过的坑：设置页「已移除歌曲」弹窗（BaseModal，DIALOG_Z）里点「清空记录」弹出的
    确认框也是 DIALOG_Z → 两者同层，而设置页是导航后才挂载的、Teleport 排在 App 根部
    挂载的 ConfirmDialog 之后 → 确认框被主弹窗压住，点不到、看不见。 */
export const PROMPT_Z = 'z-[90]'

/** 统一的弹窗遮罩类：居中布局 + 半透明底 + 按设置的模糊度（层由调用方传入，默认普通弹窗层）。
    dialog-overlay 供 style.css 在自定义背景图（has-bg）下强制加强模糊 */
export function dialogOverlayClass(z = DIALOG_Z): string {
  return `fixed inset-0 dialog-overlay ${z} flex items-center justify-center bg-black/40 ${BLUR_CLASSES[dialogBlur()]}`
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
