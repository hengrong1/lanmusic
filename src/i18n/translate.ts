import { i18n } from './index'

/**
 * 非组件上下文（Pinia store / composable / 工具模块）里的翻译函数。
 *
 * 组件内请继续用 `useI18n()` 的 `t`；这里的实现读取 i18n 全局实例的当前 locale，
 * 因此切换语言后新产生的文案会立即跟随（已是既成事实的 toast 不会回溯改写）。
 */
export function t(key: string, named?: Record<string, unknown>): string {
  const translate = i18n.global.t as unknown as (k: string, n?: Record<string, unknown>) => string
  return translate(key, named)
}
