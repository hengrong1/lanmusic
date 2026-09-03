/** 全局字体（localStorage 持久化）：设置为空串表示使用软件默认字体栈 */
import { emit } from '@tauri-apps/api/event'

const LS_FONT = 'lm.font'

/** 读取当前全局字体（CSS font-family 字符串，空 = 默认） */
export function getAppFont(): string {
  return localStorage.getItem(LS_FONT) ?? ''
}

/** 立即把字体应用到指定文档（body 内联样式，覆盖样式表的默认字体栈） */
function applyTo(doc: Document, family: string) {
  doc.body.style.fontFamily = family
}

/** 应用已存储的全局字体（启动时各窗口调用一次） */
export function applyStoredFont() {
  applyTo(document, getAppFont())
}

/** 修改全局字体并立即生效（设置页调用） */
export function setAppFont(family: string) {
  if (family) localStorage.setItem(LS_FONT, family)
  else localStorage.removeItem(LS_FONT)
  applyTo(document, family)
  // 通知歌词/托盘浮窗同步新字体
  void emit('font:changed', family).catch(() => {})
}

