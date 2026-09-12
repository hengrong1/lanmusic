import gsap from 'gsap'
import { watch, type Ref } from 'vue'

/**
 * 当 ready 变为真时，对容器内所有 [data-stagger] 元素做交错入场动画。
 * 视图切换会重新挂载页面组件，因此每次进入页面都会播放一次。
 * 用法：元素上加 data-stagger 属性，根元素绑定 ref。
 * 注意：只用于少量静态元素（页头/分区），大数据列表（虚拟行、大网格）不要加，避免卡顿。
 */
export function useStagger(root: Ref<HTMLElement | null>, ready: Ref<boolean>) {
  // 每个组件实例只播一次；ready 的语义是「数据已落定」（加载完成，空也算就绪）。
  let played = false
  watch(
    [root, ready],
    ([el, v]) => {
      if (played || !el) return
      const els = el.querySelectorAll('[data-stagger]')
      // 元素太多时直接跳过：大量并发 tween 会造成明显卡顿（保持原样可见）
      if (!els.length || els.length > 40) {
        played = true
        return
      }
      if (!v) {
        // 数据未就绪：绘制前先隐藏（flush: 'post' 时 DOM 已更新、浏览器尚未绘制），
        // 否则元素先以可见状态画出一帧、数据到达后再隐藏重播，表现为「放两遍动画」
        gsap.set(els, { opacity: 0, transition: 'none' })
        return
      }
      played = true
      // transition: 'none'：防止元素自身的 CSS transition（如 BaseButton 的 transition-all）
      // 逐帧追赶 GSAP 的样式写入造成拖影；clearProps 在动画结束后恢复原有样式
      gsap.fromTo(
        els,
        { opacity: 0, y: 14, transition: 'none' },
        {
          opacity: 1,
          y: 0,
          duration: 0.36,
          stagger: { amount: Math.min(0.4, els.length * 0.03) },
          ease: 'power2.out',
          clearProps: 'all',
        },
      )
    },
    { immediate: true, flush: 'post' },
  )
}
