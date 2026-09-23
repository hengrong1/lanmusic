/* eslint-disable */
/**
 * 带参 i18n 调用点校验：
 *  1) 每个 t()/tr()/$t() 的 key 在 zh/en 都存在
 *  2) 调用点传入的参数名集合 === 语言包字符串里的 {占位符} 集合（两种语言都要对齐）
 *     —— 这是「接 i18n」最容易留下的隐性 bug：key 存在但占位符名字写错，界面会显示字面量 {count}
 *  3) 打印 toast 命名空间的英文成品串（把 store/composable 的提示直接渲染出来看）
 *
 * 用法：node scripts/check-placeholders.cjs
 */
const fs = require('fs')
const path = require('path')

const root = path.resolve(__dirname, '..')

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

function lookup(obj, key) {
  return key.split('.').reduce((acc, part) => (acc == null ? undefined : acc[part]), obj)
}

function placeholders(str) {
  const out = new Set()
  if (typeof str !== 'string') return out
  const re = /\{(\w+)\}/g
  let m
  while ((m = re.exec(str))) out.add(m[1])
  return out
}

function walk(dir, acc = []) {
  for (const name of fs.readdirSync(dir)) {
    const full = path.join(dir, name)
    const st = fs.statSync(full)
    if (st.isDirectory()) {
      if (name === 'node_modules' || name === '.workbuddy' || name === 'dist') continue
      walk(full, acc)
    } else if (/\.(vue|ts)$/.test(name)) {
      acc.push(full)
    }
  }
  return acc
}

// 只扫 src，且跳过语言包自身与 i18n 基础设施
const files = walk(path.join(root, 'src')).filter(
  (f) => !f.includes(path.join('i18n', 'locales')) && !f.includes(path.join('i18n', 'index.ts')),
)

const CALL_RE = /(?<![\w.])\$?(?:t|tr)\(\s*(['"])([A-Za-z][\w.]*)\1/g

/**
 * 从 `{` 开始做花括号配对扫描，返回体内容。
 * 不能用「非右花括号」的简单字符类 —— 模板字面量里的 ${...} 会提前截断（本项目曾因此误报）。
 */
function readObjectBody(src, openIdx) {
  let depth = 0
  for (let i = openIdx; i < src.length; i++) {
    const c = src[i]
    if (c === '{') depth++
    else if (c === '}') {
      depth--
      if (depth === 0) return src.slice(openIdx + 1, i)
    }
  }
  return null
}

/** 按顶层逗号切分（跳过字符串/模板字面量与嵌套括号内的逗号） */
function splitTopLevel(body) {
  const parts = []
  let depth = 0
  let quote = null
  let cur = ''
  for (let i = 0; i < body.length; i++) {
    const c = body[i]
    if (quote) {
      cur += c
      if (c === '\\') {
        cur += body[++i] ?? ''
      } else if (c === quote) {
        quote = null
      }
      continue
    }
    if (c === "'" || c === '"' || c === '`') {
      quote = c
      cur += c
      continue
    }
    if (c === '(' || c === '[' || c === '{') depth++
    if (c === ')' || c === ']' || c === '}') depth--
    if (c === ',' && depth === 0) {
      parts.push(cur)
      cur = ''
      continue
    }
    cur += c
  }
  parts.push(cur)
  return parts
}

/** 顶层冒号位置（跳过字符串与嵌套括号），找不到返回 -1 */
function topLevelColon(seg) {
  let depth = 0
  let quote = null
  for (let i = 0; i < seg.length; i++) {
    const c = seg[i]
    if (quote) {
      if (c === '\\') i++
      else if (c === quote) quote = null
      continue
    }
    if (c === "'" || c === '"' || c === '`') {
      quote = c
      continue
    }
    if (c === '(' || c === '[' || c === '{') depth++
    else if (c === ')' || c === ']' || c === '}') depth--
    else if (c === ':' && depth === 0) return i
  }
  return -1
}

let refs = 0
let missingKey = []
let mismatch = []
const withParams = []

for (const file of files) {
  const rel = path.relative(root, file).replace(/\\/g, '/')
  const src = fs.readFileSync(file, 'utf8')
  let m
  CALL_RE.lastIndex = 0
  while ((m = CALL_RE.exec(src))) {
    const key = m[2]
    refs++
    // 看 key 之后是不是 ", {"
    let i = CALL_RE.lastIndex
    while (i < src.length && /\s/.test(src[i])) i++
    let rawBody = null
    if (src[i] === ',') {
      i++
      while (i < src.length && /\s/.test(src[i])) i++
      if (src[i] === '{') rawBody = readObjectBody(src, i)
    }
    // 调用点参数名：按顶层逗号切分，取每段冒号前的标识符（无冒号则整段就是参数名）
    const given = new Set()
    if (rawBody && rawBody.trim()) {
      for (const seg of splitTopLevel(rawBody)) {
        const s = seg.trim()
        if (!s) continue
        const colon = topLevelColon(s)
        const name = (colon === -1 ? s : s.slice(0, colon)).trim()
        if (/^[A-Za-z_$][\w$]*$/.test(name)) given.add(name)
      }
    }
    const z = lookup(zh, key)
    const e = lookup(en, key)
    if (z === undefined) missingKey.push(`${rel}: zh 缺 ${key}`)
    if (e === undefined) missingKey.push(`${rel}: en 缺 ${key}`)
    if (given.size === 0) continue
    withParams.push({ rel, key, given: [...given], z, e })
    for (const [lang, val] of [
      ['zh', z],
      ['en', e],
    ]) {
      if (typeof val !== 'string') continue
      const ph = placeholders(val)
      const lack = [...given].filter((g) => !ph.has(g))
      const extra = [...ph].filter((p) => !given.has(p))
      if (lack.length || extra.length) {
        mismatch.push(
          `${rel}  ${key}  [${lang}]  调用点={${[...given].join(',')}}  串内={${[...ph].join(',')}}` +
            (lack.length ? `  缺:${lack.join(',')}` : '') +
            (extra.length ? `  多:${extra.join(',')}` : ''),
        )
      }
    }
  }
}

console.log(`扫描文件 ${files.length} 个，带参调用点 ${withParams.length} 个，总引用 ${refs} 处`)
console.log(`\n== 1) key 存在性 ==`)
console.log(missingKey.length ? missingKey.join('\n') : '✓ 全部命中')

console.log(`\n== 2) 占位符对齐 ==`)
console.log(mismatch.length ? mismatch.join('\n') : '✓ 全部对齐')

// 3) toast 命名空间英文成品串：把 store/composable 的提示按样例参数渲染出来
function fill(str, params) {
  return String(str).replace(/\{(\w+)\}/g, (_, k) => (k in params ? String(params[k]) : `«${k}?»`))
}

console.log(`\n== 3) toast 成品串（样例参数；detail 按 library.ts 的真实拼接方式合成）==`)

/** 复刻 stores/library.ts 的 scanDone 组装：各分项用 common.listSep 连接 */
function composeScanDetail(L, params) {
  const parts = []
  if (params.added) parts.push(fill(L.toast.scanAdded, { count: params.added }))
  if (params.updated) parts.push(fill(L.toast.scanUpdated, { count: params.updated }))
  if (params.removed) parts.push(fill(L.toast.scanRemoved, { count: params.removed }))
  return parts.join(L.common.listSep)
}

const scanParams = { added: 2, updated: 1, removed: 1, time: '1.2' }
const samples = {
  addedToPlaylistCount: { count: 2 },
  allAlreadyInPlaylist: {},
  scanAdded: { count: 2 },
  scanUpdated: { count: 1 },
  scanRemoved: { count: 1 },
  scanDone: { time: scanParams.time, detail: composeScanDetail(zh, scanParams) },
  scanFailedDetail: { message: 'permission denied' },
  lyricOffsetReset: {},
  lyricOffsetDelay: { value: '0.5' },
  lyricOffsetAdvance: { value: '0.5' },
  playFailed: { title: 'Midnight Drive' },
  playNextAfter: { title: 'Midnight Drive' },
  addedToQueue: {},
  desktopLyricsOpenFailed: {},
  desktopLyricsFailed: { error: 'permission denied' },
  checkUpdateFailed: { error: 'network unreachable' },
  updateDownloadFailed: { error: 'disk full' },
}
for (const [k, params] of Object.entries(samples)) {
  if (!(k in (en.toast || {}))) {
    console.log(`  ✗ toast.${k}  en 缺失`)
    continue
  }
  // scanDone 的 detail 在 zh/en 各自要按对应语言合成，才能看出真实成品
  const pEn = k === 'scanDone' ? { ...params, detail: composeScanDetail(en, scanParams) } : params
  const pZh = k === 'scanDone' ? { ...params, detail: composeScanDetail(zh, scanParams) } : params
  console.log(`  toast.${k}`)
  console.log(`    en: ${fill(en.toast[k], pEn)}`)
  console.log(`    zh: ${fill(zh.toast[k], pZh)}`)
}

const ok = !missingKey.length && !mismatch.length
console.log(`\n${ok ? '✓ PASS' : '✗ FAIL'}`)
process.exit(ok ? 0 : 1)
