import { ref, watchEffect } from 'vue'
import { hexToRgba, type AmbientPalette } from '@/utils/color'

/**
 * 自定义主题色：预设取自 Ant Design 色板
 * （https://ant-design.antgroup.com/docs/spec/colors-cn），每组 10 阶，第 6 阶为品牌色。
 *
 * 实现：项目全部强调色走 Tailwind 的 violet-* 工具类，v4 下这些类编译为
 * `var(--color-violet-*)` 引用——选中预设后在 <html> 上覆盖这组变量（Ant 第 6 阶
 * 对齐 violet-500 作为全局强调色），全部 violet-* 类（含 /透明度修饰符的 color-mix）
 * 随之换肤，无需改动任何类名；未选择时移除覆盖，回落 Tailwind 原生紫。
 * 同时同步 --accent（播放条滑块在非播放页的填充色；播放页会按封面主色内联覆盖）。
 */

export interface ThemeColorPreset {
  key: string
  /** i18n 键（settings.colorXxx），悬停提示显示色名 */
  nameKey: string
  /** Ant 1-10 阶色值 */
  scale: string[]
}

/** Tailwind violet 阶梯；与 Ant 阶的对应关系见 STEPS 用法（Ant-6 → violet-500） */
const STEPS = [50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 950] as const

export const THEME_COLOR_PRESETS: ThemeColorPreset[] = [
  {
    key: 'dust',
    nameKey: 'settings.colorDust',
    scale: ['#fff1f0', '#ffccc7', '#ffa39e', '#ff7875', '#ff4d4f', '#f5222d', '#cf1322', '#a8071a', '#820014', '#5c0011'],
  },
  {
    key: 'volcano',
    nameKey: 'settings.colorVolcano',
    scale: ['#fff2e8', '#ffd8bf', '#ffbb96', '#ff9c6e', '#ff7a45', '#fa541c', '#d4380d', '#ad2102', '#871400', '#610b00'],
  },
  {
    key: 'sunset',
    nameKey: 'settings.colorSunset',
    scale: ['#fff7e6', '#ffe7ba', '#ffd591', '#ffc069', '#ffa940', '#fa8c16', '#d46b08', '#ad4e00', '#873800', '#612500'],
  },
  {
    key: 'gold',
    nameKey: 'settings.colorGold',
    scale: ['#fffbe6', '#fff1b8', '#ffe58f', '#ffd666', '#ffc53d', '#faad14', '#d48806', '#ad6800', '#874d00', '#613400'],
  },
  {
    key: 'sunrise',
    nameKey: 'settings.colorSunrise',
    scale: ['#feffe6', '#ffffb8', '#fffb8f', '#fff566', '#ffec3d', '#fadb14', '#d4b106', '#ad8b00', '#876800', '#614700'],
  },
  {
    key: 'lime',
    nameKey: 'settings.colorLime',
    scale: ['#fcffe6', '#f4ffb8', '#eaff8f', '#d3f261', '#bae637', '#a0d911', '#7cb305', '#5b8c00', '#3f6600', '#254000'],
  },
  {
    key: 'green',
    nameKey: 'settings.colorGreen',
    scale: ['#f6ffed', '#d9f7be', '#b7eb8f', '#95de64', '#73d13d', '#52c41a', '#389e0d', '#237804', '#135200', '#092b00'],
  },
  {
    key: 'cyan',
    nameKey: 'settings.colorCyan',
    scale: ['#e6fffb', '#b5f5ec', '#87e8de', '#5cdbd3', '#36cfc9', '#13c2c2', '#08979c', '#006d75', '#00474f', '#002329'],
  },
  {
    key: 'blue',
    nameKey: 'settings.colorBlue',
    scale: ['#e6f4ff', '#bae0ff', '#91caff', '#69b1ff', '#4096ff', '#1677ff', '#0958d9', '#003a8c', '#002766', '#001d66'],
  },
  {
    key: 'geekblue',
    nameKey: 'settings.colorGeekblue',
    scale: ['#f0f5ff', '#d6e4ff', '#adc6ff', '#85a5ff', '#597ef7', '#2f54eb', '#1d39c4', '#10269e', '#061178', '#030852'],
  },
  {
    key: 'purple',
    nameKey: 'settings.colorPurple',
    scale: ['#f9f0ff', '#efdbff', '#d3adf7', '#b37feb', '#9254de', '#722ed1', '#531dab', '#391085', '#22075e', '#120338'],
  },
  {
    key: 'magenta',
    nameKey: 'settings.colorMagenta',
    scale: ['#fff0f6', '#ffd6e7', '#ffadd2', '#ff85c0', '#f759ab', '#eb2f96', '#c41d7f', '#780650', '#520339', '#2a0135'],
  },
]

const STORAGE_KEY = 'lm.themeColor'
/** 当前选中的预设 key；null = 默认紫（不覆盖任何变量） */
const selected = ref<string | null>(localStorage.getItem(STORAGE_KEY))

function apply(key: string | null) {
  const root = document.documentElement.style
  const preset = THEME_COLOR_PRESETS.find((p) => p.key === key)
  if (!preset) {
    for (const step of STEPS) root.removeProperty(`--color-violet-${step}`)
    root.removeProperty('--accent')
    return
  }
  // Ant 阶与 violet 阶错一位对齐：Ant-6（品牌色）落到 violet-500，Ant-5 → 400（悬停亮档），
  // Ant-7 → 600（文字深档）；violet-950 无对应阶，复用最深一档
  STEPS.forEach((step, i) => {
    root.setProperty(`--color-violet-${step}`, preset.scale[Math.min(i, preset.scale.length - 1)])
  })
  root.setProperty('--accent', preset.scale[5])
}

watchEffect(() => apply(selected.value))

/**
 * 当前主题色的环境色派生（useAmbient 的兜底：无封面/提取失败/切歌间隙时用）。
 * 未选主题色时精确复刻默认紫的旧兜底值；选了主题色则整体跟随：
 * 亮档（Ant-5）做强调、再亮一档（Ant-4）做第二色，辉光/深底用品牌色混黑近似原 hsl 亮度。
 */
export function themeAmbientPalette(): AmbientPalette {
  const preset = THEME_COLOR_PRESETS.find((p) => p.key === selected.value)
  if (!preset) {
    return {
      accent: '#a78bfa',
      accent2: '#c084fc',
      accentSoft: 'rgba(167, 139, 250, 0.45)',
      glow: '#2e1065',
      deep: '#09090b',
    }
  }
  const light = preset.scale[4]
  const brand = preset.scale[5]
  return {
    accent: light,
    accent2: preset.scale[3],
    accentSoft: hexToRgba(light, 0.45),
    glow: `color-mix(in srgb, ${brand} 32%, black)`,
    deep: `color-mix(in srgb, ${brand} 15%, black)`,
  }
}

export function useThemeColor() {
  function setThemeColor(key: string | null) {
    selected.value = key
    if (key) localStorage.setItem(STORAGE_KEY, key)
    else localStorage.removeItem(STORAGE_KEY)
  }
  return { themeColor: selected, presets: THEME_COLOR_PRESETS, setThemeColor }
}
