<script setup lang="ts">
import { BaseButton } from '@/components/ui'

/**
 * 懒加载视图的失败兜底：chunk 加载失败（包损坏 / 升级中断）时展示，
 * 提供「重试」入口（defineAsyncComponent 的 errorComponent 会收到 retry 回调）
 */
defineProps<{ retry: () => void }>()
</script>

<template>
  <div class="flex h-full flex-col items-center justify-center gap-3 p-10 text-center">
    <p class="text-sm text-zinc-500">{{ $t('toast.viewLoadFailed') }}</p>
    <BaseButton size="sm" @click="retry">{{ $t('common.retry') }}</BaseButton>
  </div>
</template>
