/* 校验：① zh/en 顶层命名空间与逐 key 对齐；② 源码中所有 $t()/t() 引用的 key 都存在。 */
const fs = require('fs')
const path = require('path')

const ROOT = path.resolve(__dirname, '..')
const LOCALES = path.join(ROOT, 'src/i18n/locales')

function load(file) {
  const src = fs.readFileSync(path.join(LOCALES, file), 'utf8')
  const body = src.replace(/export\s+default\s*/, '').trim()
  // eslint-disable-next-line no-new-func
  return new Function('return (' + body + ')')()
}

const zh = load('zh.ts')
const en = load('en.ts')

function flat(obj, prefix = '') {
  const out = new Set()
  for (const [k, v] of Object.entries(obj)) {
    const key = prefix ? `${prefix}.${k}` : k
    if (v && typeof v === 'object') for (const x of flat(v, key)) out.add(x)
    else out.add(key)
  }
  return out
}

const fz = flat(zh)
const fe = flat(en)
const onlyZh = [...fz].filter((k) => !fe.has(k))
const onlyEn = [...fe].filter((k) => !fz.has(k))
console.log(`zh key 数: ${fz.size}   en key 数: ${fe.size}`)
console.log(`仅 zh 有: ${onlyZh.length ? onlyZh.join(', ') : '(无)'}`)
console.log(`仅 en 有: ${onlyEn.length ? onlyEn.join(', ') : '(无)'}`)

// ---- 扫描源码引用 ----
function walk(dir, acc = []) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name)
    if (e.isDirectory()) walk(p, acc)
    else if (/\.(vue|ts)$/.test(e.name)) acc.push(p)
  }
  return acc
}

const files = walk(path.join(ROOT, 'src')).filter((p) => !p.includes('i18n/locales'))
const re = /(?<![\w.])\$?(?:t|tr)\(\s*(['"])([a-zA-Z][\w.]*)\1/g
const missing = new Map()
let refs = 0
for (const f of files) {
  const src = fs.readFileSync(f, 'utf8')
  let m
  while ((m = re.exec(src))) {
    const key = m[2]
    if (key === 't') continue
    refs++
    if (!fz.has(key)) {
      const rel = path.relative(ROOT, f).replace(/\\/g, '/')
      if (!missing.has(key)) missing.set(key, new Set())
      missing.get(key).add(rel)
    }
  }
}
console.log(`\n源码 i18n 引用: ${refs} 处，未命中 key: ${missing.size}`)
for (const [k, fs_] of missing) console.log(`  ✗ ${k}  ← ${[...fs_].join(', ')}`)
