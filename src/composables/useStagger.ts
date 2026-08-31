import gsap from 'gsap'
import { watch, type Ref } from 'vue'

/**
 * 当 ready 变为真时，对容器内所有 [data-stagger] 元素做交错入场动画。
 * 视图切换会重新挂载页面组件，因此每次进入页面都会播放一次。
 * 用法：元素上加 data-stagger 属性，根元素绑定 ref。
 * 注意：只用于少量静态元素（页头/分区），大数据列表（虚拟行、大网格）不要加，避免卡顿。
 */
export function useStagger(root: Ref<HTMLElement | null>, ready: Ref<boolean>) {
  watch(
    ready,
    (v) => {
      if (!v) return
      requestAnimationFrame(() => {
        if (!root.value) return
        const els = root.value.querySelectorAll('[data-stagger]')
        // 元素太多时直接跳过：大量并发 tween 会造成明显卡顿
        if (!els.length || els.length > 40) return
        gsap.from(els, {
          opacity: 0,
          y: 14,
          duration: 0.36,
          stagger: { amount: Math.min(0.4, els.length * 0.03) },
          ease: 'power2.out',
          clearProps: 'all',
        })
      })
    },
    { immediate: true },
  )
}
