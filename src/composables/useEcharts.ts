/**
 * ECharts 按需注册与统计页主题适配。
 *
 * - 只注册用到的图表/组件，控制产物体积（CanvasRenderer + Bar/Heatmap/Pie + Grid/Tooltip/VisualMap/Legend）
 * - 统计页各图共用这里的颜色 helper：跟随应用明暗主题（useTheme.resolved），zinc 系轴色
 * - 强调色固定 violet 系（主题色换肤约定：新强调色一律 violet），不随换肤联动
 */
import { use } from 'echarts/core'
import { CanvasRenderer } from 'echarts/renderers'
import { BarChart, HeatmapChart, PieChart } from 'echarts/charts'
import {
  GridComponent,
  LegendComponent,
  TooltipComponent,
  VisualMapComponent,
} from 'echarts/components'
import { LegacyGridContainLabel } from 'echarts/features'
import { computed, type ComputedRef } from 'vue'
import { useTheme } from '@/composables/useTheme'

let registered = false
export function ensureEcharts(): void {
  if (registered) return
  use([
    CanvasRenderer,
    BarChart,
    HeatmapChart,
    PieChart,
    GridComponent,
    TooltipComponent,
    VisualMapComponent,
    LegendComponent,
    // echarts 6 起 grid.containLabel 移入 legacy 特性，显式注册以保持旧写法
    LegacyGridContainLabel,
  ])
  registered = true
}

/** 统计页图表的明暗主题色包：组件 option 内解构使用 */
export function useChartTheme(): {
  isDark: ComputedRef<boolean>
  axisLabel: ComputedRef<string>
  axisLine: ComputedRef<string>
  splitLine: ComputedRef<string>
  emptyText: ComputedRef<string>
  cardBg: ComputedRef<string>
  tooltipBase: ComputedRef<Record<string, unknown>>
} {
  const { resolved } = useTheme()
  const isDark = computed(() => resolved.value === 'dark')
  return {
    isDark,
    axisLabel: computed(() => (isDark.value ? '#a1a1aa' : '#71717a')),
    axisLine: computed(() => (isDark.value ? '#3f3f46' : '#e4e4e7')),
    splitLine: computed(() => (isDark.value ? '#27272a' : '#f4f4f5')),
    emptyText: computed(() => (isDark.value ? '#52525b' : '#a1a1aa')),
    cardBg: computed(() => (isDark.value ? '#18181b' : '#ffffff')),
    tooltipBase: computed(() => ({
      backgroundColor: isDark.value ? '#18181b' : '#ffffff',
      borderColor: isDark.value ? '#3f3f46' : '#e4e4e7',
      borderWidth: 1,
      padding: [6, 10],
      textStyle: { color: isDark.value ? '#e4e4e7' : '#3f3f46', fontSize: 12 },
      extraCssText: 'box-shadow: 0 4px 14px rgba(0,0,0,.18); border-radius: 8px;',
    })),
  }
}

/** 占比环形图 / 榜单的固定调色板（violet 打头，语义可分） */
export const CHART_COLORS = ['#8b5cf6', '#0ea5e9', '#f59e0b', '#10b981', '#f43f5e', '#a1a1aa']
