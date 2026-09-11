<script setup lang="ts">
import { ref, computed, nextTick, onMounted, onBeforeUnmount, watch } from 'vue'
import { AltArrowDownIcon as ChevronDown } from '@solar-icons/vue/linear/alt-arrow-down'

export interface SelectOption {
  value: string | number
  label: string
  disabled?: boolean
}

const props = withDefaults(
  defineProps<{
    modelValue: string | number
    options: SelectOption[]
    placeholder?: string
    disabled?: boolean
    size?: 'sm' | 'md' | 'lg'
  }>(),
  {
    placeholder: '请选择...',
    disabled: false,
    size: 'md',
  },
)

const emit = defineEmits<{ 'update:modelValue': [value: string | number] }>()

const open = ref(false)
const wrap = ref<HTMLElement | null>(null)
const trigger = ref<HTMLButtonElement | null>(null)

const sizeClasses = {
  sm: 'px-3 py-1.5 text-sm',
  md: 'px-3.5 py-2 text-sm',
  lg: 'px-4 py-2.5 text-base',
}

const selectedLabel = computed(() => {
  const found = props.options.find((o) => o.value === props.modelValue)
  return found?.label ?? props.placeholder
})

/** 可聚焦的选项按钮（跳过 disabled），用于方向键在选项间移动焦点 */
function optionButtons(): HTMLButtonElement[] {
  const all = wrap.value?.querySelectorAll<HTMLButtonElement>('[data-option]')
  return all ? Array.from(all).filter((b) => !b.disabled) : []
}

// 展开后把焦点移到当前选中项（无选中则第一项），方向键/回车即可直接操作
watch(open, async (v) => {
  if (!v) return
  await nextTick()
  const list = optionButtons()
  if (!list.length) return
  const si = props.options.findIndex((o) => o.value === props.modelValue)
  const current = si >= 0 ? wrap.value?.querySelector<HTMLButtonElement>(`[data-option="${si}"]`) : null
  ;(current && !current.disabled ? current : list[0]).focus()
})

function close(focusTrigger = false) {
  open.value = false
  if (focusTrigger) trigger.value?.focus()
}

function select(option: SelectOption) {
  if (option.disabled) return
  emit('update:modelValue', option.value)
  close(true)
}

/**
 * 键盘交互：触发器的开合交给原生 button 的 click（避免与 keydown 重复切换），
 * 这里只处理展开态的 Escape / 方向键 / Tab，以及关闭态的方向键展开。
 */
function onKeydown(e: KeyboardEvent) {
  if (props.disabled) return
  if (e.key === 'Escape' && open.value) {
    e.preventDefault()
    e.stopPropagation()
    close(true)
    return
  }
  if (!open.value) {
    // 方向键不会触发 button 的 click，故在此展开；Enter/Space 交给原生 click
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault()
      open.value = true
    }
    return
  }
  const list = optionButtons()
  if (!list.length) return
  const cur = list.indexOf(document.activeElement as HTMLButtonElement)
  switch (e.key) {
    case 'ArrowDown':
      e.preventDefault()
      list[(cur + 1) % list.length]?.focus()
      break
    case 'ArrowUp':
      e.preventDefault()
      list[(cur - 1 + list.length) % list.length]?.focus()
      break
    case 'Home':
      e.preventDefault()
      list[0]?.focus()
      break
    case 'End':
      e.preventDefault()
      list[list.length - 1]?.focus()
      break
    case 'Tab':
      close(false)
      break
  }
}

function onDocClick(e: MouseEvent) {
  if (wrap.value && !wrap.value.contains(e.target as Node)) {
    open.value = false
  }
}

onMounted(() => document.addEventListener('click', onDocClick))
onBeforeUnmount(() => document.removeEventListener('click', onDocClick))
</script>

<template>
  <div ref="wrap" class="relative w-full" @keydown="onKeydown">
    <button
      ref="trigger"
      type="button"
      aria-haspopup="listbox"
      :aria-expanded="open"
      class="flex w-full cursor-pointer items-center justify-between gap-2 rounded-lg border text-left transition-all duration-150 focus:outline-none focus-visible:ring-2 focus-visible:ring-violet-500/40"
      :class="[
        sizeClasses[size],
        open ? 'border-violet-500 ring-2 ring-violet-500/10' : 'border-zinc-200 dark:border-zinc-700',
        'bg-white dark:bg-zinc-800',
        disabled ? 'opacity-50 cursor-not-allowed' : '',
      ]"
      :disabled="disabled"
      @click="open = !open"
    >
      <!--
        宽度稳定器：原生 <select> 的宽度由最长选项决定，这里用叠格复刻该行为——
        当前值与所有选项（含 placeholder）放在同一 grid 格里，不可见的 sizer 照常占位，
        于是组件在「宽度由内容决定」的容器里（工具栏、justify-between 行）不会随选中项跳宽；
        容器宽度给定时（w-28 / flex-1 等）由 min-w-0 + truncate 收敛并省略。
      -->
      <span class="grid min-w-0">
        <span
          class="col-start-1 row-start-1 min-w-0 truncate"
          :class="modelValue ? 'text-zinc-700 dark:text-zinc-200' : 'text-zinc-400'"
        >
          {{ selectedLabel }}
        </span>
        <span aria-hidden="true" class="invisible col-start-1 row-start-1 min-w-0 truncate">{{ placeholder }}</span>
        <span
          v-for="(opt, i) in options"
          :key="`sizer-${i}`"
          aria-hidden="true"
          class="invisible col-start-1 row-start-1 min-w-0 truncate"
        >
          {{ opt.label }}
        </span>
      </span>
      <ChevronDown
        class="h-4 w-4 shrink-0 text-zinc-400 transition-transform duration-200"
        :class="{ 'rotate-180': open }"
      />
    </button>

    <Transition
      enter-active-class="transition duration-150 ease-out"
      enter-from-class="opacity-0 -translate-y-1"
      enter-to-class="opacity-100 translate-y-0"
      leave-active-class="transition duration-100 ease-in"
      leave-from-class="opacity-100"
      leave-to-class="opacity-0"
    >
      <div
        v-if="open"
        role="listbox"
        class="absolute z-50 mt-1.5 w-full rounded-lg border border-zinc-200 bg-white py-1 shadow-xl dark:border-zinc-700 dark:bg-zinc-800 max-h-60 overflow-y-auto"
      >
        <button
          v-for="(opt, i) in options"
          :key="String(opt.value)"
          type="button"
          role="option"
          :data-option="i"
          :aria-selected="opt.value === modelValue"
          class="flex w-full cursor-pointer items-center px-3 py-2 text-left text-sm transition-colors focus:outline-none focus-visible:bg-zinc-100 dark:focus-visible:bg-zinc-700/60"
          :class="[
            opt.value === modelValue
              ? 'bg-violet-50 text-violet-600 font-medium dark:bg-violet-500/10 dark:text-violet-400'
              : 'text-zinc-700 hover:bg-zinc-50 dark:text-zinc-200 dark:hover:bg-zinc-700/50',
            opt.disabled ? 'opacity-40 cursor-not-allowed' : '',
          ]"
          :disabled="opt.disabled"
          @click.stop="select(opt)"
        >
          {{ opt.label }}
        </button>
      </div>
    </Transition>
  </div>
</template>
