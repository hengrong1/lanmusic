<script setup lang="ts">
import { ref, useId } from 'vue'
import type { Component } from 'vue'

const props = withDefaults(
  defineProps<{
    modelValue: string
    placeholder?: string
    type?: string
    disabled?: boolean
    readonly?: boolean
    error?: string
    hint?: string
    icon?: Component
    iconRight?: Component
    size?: 'sm' | 'md' | 'lg'
    min?: number | string
    max?: number | string
    step?: number | string
    maxlength?: number
    required?: boolean
    autocomplete?: string
    name?: string
  }>(),
  {
    type: 'text',
    disabled: false,
    readonly: false,
    size: 'md',
  },
)

const emit = defineEmits<{ 'update:modelValue': [value: string] }>()

const id = useId()
const inputEl = ref<HTMLInputElement | null>(null)

/** 原生 input 的焦点/全选能力，供上层模板 ref 调用（如弹层打开时自动聚焦并全选） */
function focus() {
  inputEl.value?.focus()
}
function select() {
  inputEl.value?.select()
}

defineExpose({ focus, select, el: inputEl })

const sizeClasses = {
  sm: 'px-3 py-1.5 text-sm',
  md: 'px-3.5 py-2 text-sm',
  lg: 'px-4 py-2.5 text-base',
}

const iconSizeClasses = {
  sm: 'h-4 w-4',
  md: 'h-4 w-4',
  lg: 'h-5 w-5',
}

function onInput(e: Event) {
  emit('update:modelValue', (e.target as HTMLInputElement).value)
}
</script>

<template>
  <div class="w-full">
    <div
      class="relative flex items-center rounded-lg border transition-all duration-150"
      :class="[
        error
          ? 'border-red-400 bg-red-50/50 dark:border-red-500/50 dark:bg-red-500/5'
          : 'border-zinc-200 bg-white hover:border-zinc-300 focus-within:border-violet-500 focus-within:ring-2 focus-within:ring-violet-500/10 dark:border-zinc-700 dark:bg-zinc-800 dark:hover:border-zinc-600 dark:focus-within:border-violet-500',
        disabled ? 'opacity-50 cursor-not-allowed' : '',
      ]"
    >
      <component
        v-if="icon"
        :is="icon"
        :class="['ml-3 shrink-0 text-zinc-400', iconSizeClasses[size]]"
      />
      <input
        :id="id"
        ref="inputEl"
        :type="type"
        :value="modelValue"
        :placeholder="placeholder"
        :disabled="disabled"
        :readonly="readonly"
        :min="min"
        :max="max"
        :step="step"
        :maxlength="maxlength"
        :required="required"
        :autocomplete="autocomplete"
        :name="name"
        class="w-full bg-transparent text-zinc-900 outline-none placeholder:text-zinc-400 dark:text-zinc-100"
        :class="[sizeClasses[size], icon ? 'pl-2' : '', iconRight ? 'pr-2' : '']"
        @input="onInput"
      />
      <component
        v-if="iconRight"
        :is="iconRight"
        :class="['mr-3 shrink-0 text-zinc-400', iconSizeClasses[size]]"
      />
    </div>
    <p v-if="error" class="mt-1 text-xs text-red-500">{{ error }}</p>
    <p v-else-if="hint" class="mt-1 text-xs text-zinc-400">{{ hint }}</p>
  </div>
</template>
