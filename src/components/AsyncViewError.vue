<script setup lang="ts">
import { BaseButton } from '@/components/ui'

/**
 * 懒加载视图的失败兜底：chunk 加载失败（包损坏 / 升级中断 / dev server 重启后旧模块失效）时展示。
 *
 * 「重试」按钮 = 整窗刷新。原因：Vue 3.5 的 defineAsyncComponent 只给 errorComponent 传
 * `error`、**不传 retry**（runtime-core 源码里是 createVNode(errorComponent, { error })，
 * retry 回调只存在于 loader 的 onError 选项）——旧实现的必需 prop `retry` 永远是 undefined，
 * 按钮点了没反应。刷新会重新拉取入口与全部模块，是最可靠的恢复路径；
 * dev 下 dev server 恢复后 Vite 的 HMR 客户端还会自动刷新一次。
 */
defineProps<{ error?: Error }>()

function onRetry() {
  location.reload()
}
</script>

<template>
  <div class="flex h-full flex-col items-center justify-center gap-3 p-10 text-center">
    <p class="text-sm text-zinc-500">{{ $t('toast.viewLoadFailed') }}</p>
    <BaseButton size="sm" @click="onRetry">{{ $t('common.retry') }}</BaseButton>
  </div>
</template>
