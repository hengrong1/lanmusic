/* eslint-disable */
/**
 * 校验 Rust 错误码契约与前端语言包对齐：
 *  1) error.rs 里 codes:: 的每个常量，在 zh/en 的 error.* 下都有对应 key
 *  2) 反向：语言包 error.* 下的每个 key，都有对应的 Rust 常量（防止前端留孤儿文案）
 *  3) err1(code, "k", v) 的参数名与语言包字符串里的 {占位符} 对齐（zh/en 都查）
 * 用法：node scripts/check-error-codes.cjs
 */
const fs = require('fs')
const path = require('path')

const root = path.resolve(__dirname, '..')

// ---------- 1. 从 error.rs 抽码 ----------
const errSrc = fs.readFileSync(path.join(root, 'src-tauri/src/error.rs'), 'utf8')
const codes = [] // [constantName, code]
const CONST_RE = /pub const ([A-Z0-9_]+): &str = "([a-zA-Z][\w.]*)";/g
let m
while ((m = CONST_RE.exec(errSrc))) codes.push([m[1], m[2]])
if (!codes.length) {
  console.error('✗ 没从 error.rs 抽到任何错误码（解析逻辑失效？）')
  process.exit(1)
}

// ---------- 2. 语言包 ----------
function loadLocale(rel) {
  let code = fs.readFileSync(path.join(root, rel), 'utf8')
  code = code.replace(/export\s+default\s*/, 'module.exports = ')
  const mod = { exports: {} }
  // eslint-disable-next-line no-new-func
  new Function('module', 'exports', code)(mod, mod.exports)
  return mod.exports
}
const zh = loadLocale('src/i18n/locales/zh.ts')
const en = loadLocale('src/i18n/locales/en.ts')

function lookup(obj, dotted) {
  return dotted.split('.').reduce((acc, p) => (acc == null ? undefined : acc[p]), obj)
}
function placeholders(str) {
  const out = new Set()
  const re = /\{(\w+)\}/g
  let mm
  while ((mm = re.exec(String(str)))) out.add(mm[1])
  return out
}

// ---------- 3. 抽 Rust 调用点的参数名 ----------
// err(codes::X) / err(crate::error::codes::X) / err1(code, "k", expr) —— 三种路径形态都认
const callParams = new Map() // code -> Set(paramNames)
const CALL_RE = /\berr1?\(\s*(?:crate::error::)?codes::([A-Z0-9_]+)\s*(?:,\s*"(\w+)")?/g
const rustFiles = []
;(function walk(dir) {
  for (const name of fs.readdirSync(dir)) {
    const full = path.join(dir, name)
    if (fs.statSync(full).isDirectory()) walk(full)
    else if (name.endsWith('.rs')) rustFiles.push(full)
  }
})(path.join(root, 'src-tauri', 'src'))

for (const f of rustFiles) {
  const src = fs.readFileSync(f, 'utf8')
  let c
  while ((c = CALL_RE.exec(src))) {
    if (!c[2]) continue
    const nameToCode = new Map(codes)
    const code = nameToCode.get(c[1])
    if (!code) continue
    if (!callParams.has(code)) callParams.set(code, new Set())
    callParams.get(code).add(c[2])
  }
}

// ---------- 4. 校验 ----------
const missing = []
const orphan = []
const mismatch = []

for (const [name, code] of codes) {
  const key = `error.${code}`
  const z = lookup(zh, key)
  const e = lookup(en, key)
  if (z === undefined) missing.push(`zh 缺 ${key}（codes::${name}）`)
  if (e === undefined) missing.push(`en 缺 ${key}（codes::${name}）`)
  const given = callParams.get(code) || new Set()
  for (const [lang, val] of [['zh', z], ['en', e]]) {
    if (typeof val !== 'string') continue
    const ph = placeholders(val)
    const lack = [...given].filter((g) => !ph.has(g))
    const extra = [...ph].filter((p) => !given.has(p))
    if (lack.length || extra.length) {
      mismatch.push(
        `${key} [${lang}] 调用点={${[...given].join(',')}} 串内={${[...ph].join(',')}}` +
          (lack.length ? ` 缺:${lack.join(',')}` : '') +
          (extra.length ? ` 多:${extra.join(',')}` : ''),
      )
    }
  }
}

// 语言包里 error.* 的叶子 key（拍平成 dotted path）
function flatten(obj, prefix, out) {
  for (const [k, v] of Object.entries(obj || {})) {
    const p = prefix ? `${prefix}.${k}` : k
    if (v && typeof v === 'object') flatten(v, p, out)
    else out.push(p)
  }
  return out
}
const zhLeaf = flatten(zh.error, 'error', [])
const enLeaf = flatten(en.error, 'error', [])
for (const key of new Set([...zhLeaf, ...enLeaf])) {
  const code = key.replace(/^error\./, '')
  if (!codes.some(([, c]) => c === code)) orphan.push(key)
}

console.log(`Rust 错误码: ${codes.length} 个；zh error 叶子: ${zhLeaf.length}；en error 叶子: ${enLeaf.length}`)
console.log(missing.length ? `\n== 1) 缺 key ==\n${missing.join('\n')}` : '✓ 每个错误码在 zh/en 都有 key')
console.log(orphan.length ? `\n== 2) 孤儿 key ==\n${orphan.join('\n')}` : '✓ 语言包没有孤儿 key')
console.log(mismatch.length ? `\n== 3) 占位符 ==\n${mismatch.join('\n')}` : '✓ 占位符与 err1 参数对齐')

const ok = !missing.length && !orphan.length && !mismatch.length
console.log(`\n${ok ? '✓ PASS' : '✗ FAIL'}`)
process.exit(ok ? 0 : 1)
