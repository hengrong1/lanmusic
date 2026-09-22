<script setup lang="ts">
import { ref } from 'vue'
import { CloseIcon as X } from '@solar-icons/vue/linear/close'
import { useI18n } from 'vue-i18n'

/**
 * 标签输入：输入 + 回车（或逗号）生成标签，Backspace 在空输入时删除最后一个，
 * 每个标签自带删除按钮。值是标签数组；重复（不区分大小写）与空值自动忽略。
 * 聚焦态样式与 BaseInput 一致（点击容器任意位置聚焦内层输入框）。
 */
const props = withDefaults(
  defineProps<{
    modelValue: string[]
    placeholder?: string
    disabled?: boolean
  }>(),
  {
    placeholder: '',
    disabled: false,
  },
)

const emit = defineEmits<{ 'update:modelValue': [value: string[]] }>()

const { t } = useI18n()
const draft = ref('')
const inputEl = ref<HTMLInputElement | null>(null)

function focusInput() {
  if (!props.disabled) inputEl.value?.focus()
}

/** 把输入框里的草稿收成一个标签（去空白、大小写不敏感去重） */
function commit() {
  const v = draft.value.trim()
  draft.value = ''
  if (!v) return
  if (props.modelValue.some((tag) => tag.toLowerCase() === v.toLowerCase())) return
  emit('update:modelValue', [...props.modelValue, v])
}

function removeAt(index: number) {
  emit(
    'update:modelValue',
    props.modelValue.filter((_, i) => i !== index),
  )
}

function onKeydown(e: KeyboardEvent) {
  if (props.disabled) return
  // 回车 / 逗号（半角全角）都提交标签
  if (e.key === 'Enter' || e.key === ',' || e.key === '，') {
    e.preventDefault()
    commit()
  } else if (e.key === 'Backspace' && !draft.value && props.modelValue.length) {
    // 空输入按退格：删除最后一个标签
    emit('update:modelValue', props.modelValue.slice(0, -1))
  }
}

function onBlur() {
  // 失焦时草稿还在就收掉，避免内容默默留在输入框里不生效
  if (draft.value.trim()) commit()
}
</script>

<template>
  <div
    class="flex w-full flex-wrap items-center gap-1.5 rounded-lg border border-zinc-200 bg-white px-2.5 py-1.5 text-sm focus-within:border-violet-500 focus-within:ring-2 focus-within:ring-violet-500/10 dark:border-zinc-700 dark:bg-zinc-800"
    :class="[
      disabled ? 'cursor-not-allowed opacity-50' : 'cursor-text',
      disabled ? '' : 'hover-accent-border',
    ]"
    @click="focusInput"
  >
    <span
      v-for="(tag, i) in modelValue"
      :key="`${tag}-${i}`"
      class="flex items-center gap-1 rounded-md bg-violet-50 py-0.5 pr-1 pl-2 text-xs font-medium text-violet-600 dark:bg-violet-500/15 dark:text-violet-300"
    >
      {{ tag }}
      <button
        type="button"
        class="flex h-4 w-4 cursor-pointer items-center justify-center rounded transition-colors hover:bg-violet-100 hover:text-violet-700 dark:hover:bg-violet-500/25 dark:hover:text-violet-200"
        :aria-label="t('common.remove')"
        @click.stop="removeAt(i)"
      >
        <X class="h-3 w-3" />
      </button>
    </span>
    <input
      ref="inputEl"
      v-model="draft"
      type="text"
      class="min-w-20 flex-1 border-none bg-transparent text-sm text-zinc-800 outline-none placeholder:text-zinc-400 dark:text-zinc-100"
      :placeholder="modelValue.length ? '' : placeholder"
      :disabled="disabled"
      maxlength="100"
      @keydown="onKeydown"
      @blur="onBlur"
    />
  </div>
</template>
