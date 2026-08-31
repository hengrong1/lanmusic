export interface AmbientPalette {
  /** 强调色：活动歌词行、按钮 hover 等 */
  accent: string
  /** 强调色的半透明版：脉冲光环等 */
  accentSoft: string
  /** 页面顶部辉光色（深） */
  glow: string
  /** 页面深底色 */
  deep: string
}

function rgbToHsl(r: number, g: number, b: number): [number, number, number] {
  r /= 255
  g /= 255
  b /= 255
  const max = Math.max(r, g, b)
  const min = Math.min(r, g, b)
  let h = 0
  const l = (max + min) / 2
  const d = max - min
  const s = d === 0 ? 0 : d / (1 - Math.abs(2 * l - 1))
  if (d !== 0) {
    if (max === r) h = ((g - b) / d) % 6
    else if (max === g) h = (b - r) / d + 2
    else h = (r - g) / d + 4
    h *= 60
    if (h < 0) h += 360
  }
  return [h, s, l]
}

/**
 * 从专辑封面提取主色，生成播放页的环境配色。
 * 做法：封面缩小到 16x16 采样 → RGB 分桶统计 → 按"饱和度×中亮度×像素数"打分选主色
 * → 派生强调色/辉光色/深底色。失败返回 null，页面回落到默认紫色。
 */
export async function extractAmbient(url: string): Promise<AmbientPalette | null> {
  try {
    const img = await new Promise<HTMLImageElement | null>((resolve) => {
      const image = new Image()
      image.crossOrigin = 'anonymous' // 封面协议响应带 ACAO 头，画布不会被污染
      image.onload = () => resolve(image)
      image.onerror = () => resolve(null)
      image.src = url
    })
    if (!img) return null

    const size = 16
    const canvas = document.createElement('canvas')
    canvas.width = size
    canvas.height = size
    const ctx = canvas.getContext('2d', { willReadFrequently: true })
    if (!ctx) return null
    ctx.drawImage(img, 0, 0, size, size)
    const data = ctx.getImageData(0, 0, size, size).data

    const buckets = new Map<number, { r: number; g: number; b: number; n: number; score: number }>()
    for (let i = 0; i < data.length; i += 4) {
      const r = data[i]
      const g = data[i + 1]
      const b = data[i + 2]
      const max = Math.max(r, g, b)
      const min = Math.min(r, g, b)
      const sat = max === 0 ? 0 : (max - min) / max
      const lum = (0.299 * r + 0.587 * g + 0.114 * b) / 255
      // 过滤近黑/近白像素
      if (lum < 0.07 || lum > 0.93) continue
      const key = ((r >> 4) << 8) | ((g >> 4) << 4) | (b >> 4)
      const score = sat * 1.6 + (1 - Math.abs(lum - 0.5))
      const cur = buckets.get(key) ?? { r: 0, g: 0, b: 0, n: 0, score: 0 }
      cur.r += r
      cur.g += g
      cur.b += b
      cur.n += 1
      cur.score += score
      buckets.set(key, cur)
    }

    let best: { h: number; s: number; l: number; score: number } | null = null
    for (const v of buckets.values()) {
      if (v.n < 2) continue
      const [h, s, l] = rgbToHsl(v.r / v.n, v.g / v.n, v.b / v.n)
      // 主色得分 = 颜色质量 × 权重（像素占比越高越稳）
      const score = (v.score / v.n) * (0.4 + 0.6 * Math.min(1, v.n / 8))
      if (!best || score > best.score) best = { h, s, l, score }
    }
    if (!best) return null

    const h = Math.round(best.h)
    const s = Math.min(0.85, Math.max(0.35, best.s))
    const l = Math.min(0.72, Math.max(0.55, best.l))
    return {
      accent: `hsl(${h} ${Math.round(s * 100)}% ${Math.round(l * 100)}%)`,
      accentSoft: `hsl(${h} ${Math.round(s * 100)}% ${Math.round(l * 100)}% / 0.45)`,
      glow: `hsl(${h} ${Math.round(Math.min(70, s * 100))}% 16%)`,
      deep: `hsl(${h} ${Math.round(Math.min(55, s * 80))}% 7%)`,
    }
  } catch {
    return null
  }
}
