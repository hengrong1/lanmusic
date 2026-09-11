<script setup lang="ts">
import { computed } from 'vue'
import type { Component } from 'vue'

export type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'outline'
export type ButtonTone = 'default' | 'danger'
export type ButtonSize = 'xs' | 'sm' | 'md' | 'lg'

const props = withDefaults(
  defineProps<{
    variant?: ButtonVariant
    /** 语义色调：danger = 危险操作（红色系），与 variant 的强调程度正交 */
    tone?: ButtonTone
    /** 原生 type，默认 button（避免放进 <form> 时意外触发提交） */
    type?: 'button' | 'submit' | 'reset'
    size?: ButtonSize
    icon?: Component
    iconRight?: Component
    loading?: boolean
    disabled?: boolean
    block?: boolean
    rounded?: boolean
  }>(),
  {
    variant: 'primary',
    tone: 'default',
    type: 'button',
    size: 'md',
    loading: false,
    disabled: false,
    block: false,
    rounded: false,
  },
)

const emit = defineEmits<{ click: [e: MouseEvent] }>()

const variantClasses: Record<ButtonVariant, string> = {
  primary:
    'bg-violet-500 text-white shadow-sm hover:bg-violet-400 active:bg-violet-600',
  secondary:
    'bg-zinc-100 text-zinc-700 hover:bg-zinc-200 active:bg-zinc-300 dark:bg-zinc-800 dark:text-zinc-200 dark:hover:bg-zinc-700 dark:active:bg-zinc-600',
  ghost:
    'text-zinc-600 hover:bg-zinc-100 active:bg-zinc-200 dark:text-zinc-300 dark:hover:bg-zinc-800 dark:active:bg-zinc-700',
  outline:
    'border border-zinc-300 text-zinc-700 hover:bg-zinc-50 active:bg-zinc-100 dark:border-zinc-600 dark:text-zinc-200 dark:hover:bg-zinc-800 dark:active:bg-zinc-700',
}

/** danger 色调下各强调级别改用红色系 */
const dangerVariantClasses: Record<ButtonVariant, string> = {
  primary: 'bg-red-500 text-white shadow-sm hover:bg-red-400 active:bg-red-600',
  secondary:
    'bg-red-50 text-red-600 hover:bg-red-100 active:bg-red-200 dark:bg-red-500/10 dark:text-red-400 dark:hover:bg-red-500/20 dark:active:bg-red-500/30',
  ghost:
    'text-red-600 hover:bg-red-50 active:bg-red-100 dark:text-red-400 dark:hover:bg-red-500/10 dark:active:bg-red-500/20',
  outline:
    'border border-red-300 text-red-600 hover:bg-red-50 active:bg-red-100 dark:border-red-500/40 dark:text-red-400 dark:hover:bg-red-500/10 dark:active:bg-red-500/20',
}

/**
 * 禁用态配色：仅在 disabled=true 时叠加。
 * loading 不使用这套样式——"进行中"是正常外观 + 轻微透明，而不是"被禁用"的灰化。
 */
const disabledClasses: Record<ButtonVariant, string> = {
  primary: 'disabled:bg-violet-300 dark:disabled:bg-violet-800',
  secondary: 'disabled:opacity-50',
  ghost: 'disabled:opacity-50',
  outline: 'disabled:opacity-50',
}

const sizeClasses: Record<ButtonSize, string> = {
  xs: 'px-2.5 py-1 text-xs gap-1',
  sm: 'px-3 py-1.5 text-sm gap-1.5',
  md: 'px-4 py-2 text-sm gap-2',
  lg: 'px-5 py-2.5 text-base gap-2',
}

const iconSizeClasses: Record<ButtonSize, string> = {
  xs: 'h-3.5 w-3.5',
  sm: 'h-4 w-4',
  md: 'h-4 w-4',
  lg: 'h-5 w-5',
}

const classes = computed(() => [
  'inline-flex cursor-pointer items-center justify-center font-medium transition-all duration-150 ease-out focus:outline-none focus-visible:ring-2 focus-visible:ring-violet-500/40 disabled:cursor-not-allowed',
  props.rounded ? 'rounded-full' : 'rounded-lg',
  props.tone === 'danger' ? dangerVariantClasses[props.variant] : variantClasses[props.variant],
  props.disabled ? disabledClasses[props.variant] : '',
  props.loading ? 'pointer-events-none opacity-80' : '',
  sizeClasses[props.size],
  props.block ? 'w-full' : '',
])

function onClick(e: MouseEvent) {
  if (!props.disabled && !props.loading) emit('click', e)
}
</script>

<template>
  <button :type="type" :class="classes" :disabled="disabled || loading" @click="onClick">
    <svg
      v-if="loading"
      :class="['animate-spin', iconSizeClasses[size]]"
      xmlns="http://www.w3.org/2000/svg"
      fill="none"
      viewBox="0 0 24 24"
    >
      <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
      <path
        class="opacity-75"
        fill="currentColor"
        d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
      />
    </svg>
    <component :is="icon" v-else-if="icon" :class="iconSizeClasses[size]" />
    <span v-if="$slots.default" :class="[icon ? '-ml-0.5' : '', iconRight ? '-mr-0.5' : '']">
      <slot />
    </span>
    <component :is="iconRight" v-if="iconRight" :class="iconSizeClasses[size]" />
  </button>
</template>
