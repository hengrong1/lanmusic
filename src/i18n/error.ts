import { i18n } from '@/i18n'
import { t } from '@/i18n/translate'

/**
 * Rust 侧结构化错误的解码（信封格式见 src-tauri/src/error.rs）。
 *
 * 契约：`LMERR:{"code":"webdav.invalidUrl","params":{"error":"..."}}`。
 * code 是前后端稳定契约，文案挂在两份语言包的 `error.*` 下（按 code 的 `.` 分层）。
 */
const PREFIX = 'LMERR:'

interface LmError {
  code: string
  params?: Record<string, string>
}

function parseCoded(raw: string): LmError | null {
  if (!raw.startsWith(PREFIX)) return null
  try {
    const parsed = JSON.parse(raw.slice(PREFIX.length)) as LmError | null
    if (parsed && typeof parsed.code === 'string') return parsed
  } catch {
    // 信封损坏：按原样显示，绝不把解析异常抛给调用方
  }
  return null
}

/**
 * 把 IPC / 事件带来的错误转成当前语言的用户可读文案：
 * - `LMERR:` 信封 → `error.<code>` + params 插值；语言包缺 key 时回退显示 code 本身；
 * - 其余错误（第三方 / OS 错误，多为英文）原样透传。
 *
 * 替代此前的 `String(e)`——那个写法遇到信封会把 JSON 原文弹给用户。
 */
export function errorText(e: unknown): string {
  const coded = typeof e === 'string' ? parseCoded(e) : null
  if (coded) {
    const key = `error.${coded.code}`
    if (i18n.global.te(key)) return t(key, coded.params ?? {})
    return coded.code
  }
  return typeof e === 'string' ? e : String(e)
}
