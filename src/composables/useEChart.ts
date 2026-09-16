/**
 * 手动管理单个 ECharts 实例（替代 vue-echarts 组件式挂载）。
 *
 * 为什么不用 vue-echarts：懒加载视图 + 视图 out-in 过渡的组合下，组件 mounted 的 init
 * 时机可能撞上容器宽度尚未稳定（clientWidth === 0），ECharts 告警且实例以 0 尺寸创建，
 * 之后依赖容器再发生尺寸变化才能恢复。这里改为**门控重试**：容器真实尺寸非 0 的那一帧
 * 才 init（rAF 循环，上限约 3.3 秒），从根上保证 init 一定发生在可布局时刻。
 *
 * - option 变化 → 增量 setOption（null 时挂起，等数据就绪后再 init）
 * - 容器尺寸变化（窗口 / 侧栏收展）→ ResizeObserver 自动 resize
 * - 卸载 → 断开观察并 dispose
 */
import * as echarts from 'echarts/core'
import { onMounted, onUnmounted, watch, type ComputedRef, type Ref } from 'vue'
import { ensureEcharts } from './useEcharts'

export function useEChart(
  elRef: Ref<HTMLElement | null>,
  option: ComputedRef<Record<string, unknown> | null | undefined>,
) {
  let chart: ReturnType<typeof echarts.init> | null = null
  let ro: ResizeObserver | null = null
  let disposed = false
  let tries = 0

  function tryInit() {
    if (disposed || chart) return
    const el = elRef.value
    if (!el) return
    // 尺寸未就绪：下一帧再试（上限约 3.3 秒，防异常布局下无限循环）
    if (el.clientWidth === 0 || el.clientHeight === 0) {
      if (++tries > 200) return
      requestAnimationFrame(tryInit)
      return
    }
    chart = echarts.init(el)
    ro = new ResizeObserver(() => chart?.resize())
    ro.observe(el)
    const o = option.value
    if (o) chart.setOption(o)
  }

  onMounted(() => {
    ensureEcharts()
    // flush: 'post' —— elRef 在 DOM patch 后才有值（v-if 数据驱动渲染的场景）
    watch([elRef, option], () => tryInit(), { immediate: true, flush: 'post' })
  })

  onUnmounted(() => {
    disposed = true
    ro?.disconnect()
    ro = null
    chart?.dispose()
    chart = null
  })
}
