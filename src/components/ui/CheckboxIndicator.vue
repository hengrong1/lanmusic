<script setup lang="ts">
/**
 * 复选框指示器：BaseCheckbox 的方框视觉（选中填充 + 圆头对勾），纯展示、不拦截点击。
 * 用于「整行可点、复选框只是状态指示」的场景（TrackPicker / TrackTable 批量模式），
 * 避免在行内嵌套交互按钮；独立使用的复选框请直接用 BaseCheckbox。
 *
 * hover 反馈走 `group-hover:`：宿主（BaseCheckbox 的按钮、整行可点的表格行）挂 `.group`
 * 即可连带高亮方框；宿主没有 `.group` 时无副作用。
 */
const props = withDefaults(
  defineProps<{
    modelValue: boolean
    size?: 'sm' | 'md'
  }>(),
  { size: 'md' },
)

const sizeMap = {
  sm: { box: 'h-4 w-4', check: 'h-2.5 w-2.5' },
  md: { box: 'h-5 w-5', check: 'h-3 w-3' },
}
</script>

<template>
  <span
    class="relative flex shrink-0 items-center justify-center rounded border-2 transition-all duration-150"
    :class="[
      sizeMap[size].box,
      modelValue
        ? 'border-violet-500 bg-violet-500 group-hover:border-violet-600 group-hover:bg-violet-600'
        : 'border-zinc-300 bg-transparent group-hover:border-zinc-400 dark:border-zinc-600 dark:group-hover:border-zinc-500',
    ]"
  >
    <svg
      v-if="modelValue"
      :class="['text-white transition-transform duration-150', sizeMap[size].check]"
      viewBox="0 0 14 14"
      fill="none"
    >
      <path
        d="M3 7l3.5 3.5L11 4"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </svg>
  </span>
</template>
