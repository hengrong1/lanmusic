<script setup lang="ts">
import { ref, computed, nextTick, onMounted, onBeforeUnmount } from 'vue'
import { AltArrowDownIcon as ChevronDown } from '@solar-icons/vue/linear/alt-arrow-down'
import { MagnifierIcon as Search } from '@solar-icons/vue/linear/magnifier'
import { useI18n } from 'vue-i18n'

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
    /** 面板顶部显示搜索框：选项多时按 label 过滤（艺人合并这类长列表用） */
    searchable?: boolean
  }>(),
  {
    placeholder: '',
    disabled: false,
    size: 'md',
    searchable: false,
  },
)

const { t } = useI18n()
/** 未传 placeholder 时用 i18n 默认文案（共享组件不硬编码中文） */
const placeholderText = computed(() => props.placeholder || t('common.pleaseSelect'))

const emit = defineEmits<{ 'update:modelValue': [value: string | number] }>()

const open = ref(false)
const wrap = ref<HTMLElement | null>(null)
const trigger = ref<HTMLButtonElement | null>(null)
/** 面板 Teleport 到 body：绝对定位会被 overflow-hidden 祖先裁切、也无法感知视口边缘 */
const panel = ref<HTMLElement | null>(null)
const searchInput = ref<HTMLInputElement | null>(null)
const filter = ref('')
const panelStyle = ref<{ left: string; top: string; width: string }>({ left: '0px', top: '0px', width: '0px' })

const sizeClasses = {
  sm: 'px-3 py-1.5 text-sm',
  md: 'px-3.5 py-2 text-sm',
  lg: 'px-4 py-2.5 text-base',
}

const selectedLabel = computed(() => {
  const found = props.options.find((o) => o.value === props.modelValue)
  return found?.label ?? placeholderText.value
})

/** 搜索过滤（大小写不敏感的包含匹配）；未开启搜索时原样返回 */
const filteredOptions = computed(() => {
  const f = filter.value.trim().toLowerCase()
  if (!f) return props.options
  return props.options.filter((o) => o.label.toLowerCase().includes(f))
})

/** 可聚焦的选项按钮（跳过 disabled），用于方向键在选项间移动焦点 */
function optionButtons(): HTMLButtonElement[] {
  const all = panel.value?.querySelectorAll<HTMLButtonElement>('[data-option]')
  return all ? Array.from(all).filter((b) => !b.disabled) : []
}

// ---------- 打开 / 关闭 / 定位 ----------

async function openPanel() {
  if (props.disabled || open.value) return
  open.value = true
  filter.value = ''
  // 先按触发器位置出第一帧（避免面板闪现在左上角），渲染后量实际高度再翻转/收拢
  const r = trigger.value?.getBoundingClientRect()
  if (r) {
    panelStyle.value = { left: `${r.left}px`, top: `${r.bottom + 6}px`, width: `${r.width}px` }
  }
  window.addEventListener('scroll', onOutsideScroll, true)
  window.addEventListener('resize', onWindowResize)
  await nextTick()
  placePanel()
  if (props.searchable) {
    searchInput.value?.focus()
  } else {
    focusCurrent()
  }
}

function close(focusTrigger = false) {
  if (!open.value) return
  open.value = false
  window.removeEventListener('scroll', onOutsideScroll, true)
  window.removeEventListener('resize', onWindowResize)
  if (focusTrigger) trigger.value?.focus()
}

/**
 * 视口内定位：宽度贴触发器；下方放不下且上方够时翻转到上方；
 * 两头都不够时 clamp 贴边收拢，保证面板完整可见。
 */
function placePanel() {
  const t = trigger.value
  const p = panel.value
  if (!t || !p) return
  const r = t.getBoundingClientRect()
  const gap = 6
  const margin = 8
  let left = r.left
  if (left + r.width > window.innerWidth - margin) {
    left = Math.max(margin, window.innerWidth - margin - r.width)
  }
  let top = r.bottom + gap
  const ph = p.offsetHeight
  const spaceBelow = window.innerHeight - r.bottom
  if (ph + gap > spaceBelow) {
    const spaceAbove = r.top - gap
    if (spaceAbove >= ph) {
      top = r.top - gap - ph
    } else {
      // 上下都不够：放在空间更多的一侧
      top = spaceAbove > spaceBelow ? r.top - gap - ph : r.bottom + gap
    }
  }
  panelStyle.value = {
    left: `${Math.round(left)}px`,
    top: `${Math.round(Math.max(margin, Math.min(top, window.innerHeight - margin - ph)))}px`,
    width: `${Math.round(r.width)}px`,
  }
}

function onOutsideScroll(e: Event) {
  // 面板内部滚动（选项列表）不关闭；其余滚动时固定定位的面板会脱离触发器，直接收起
  if (panel.value && e.target instanceof Node && panel.value.contains(e.target)) return
  close(false)
}
function onWindowResize() {
  if (open.value) placePanel()
}

/** 把焦点移到当前选中项（无选中则第一项） */
function focusCurrent() {
  const list = optionButtons()
  if (!list.length) return
  const si = filteredOptions.value.findIndex((o) => o.value === props.modelValue)
  const current = si >= 0 ? panel.value?.querySelector<HTMLButtonElement>(`[data-option="${si}"]`) : null
  ;(current && !current.disabled ? current : list[0]).focus()
}

function select(option: SelectOption) {
  if (option.disabled) return
  emit('update:modelValue', option.value)
  close(true)
}

// ---------- 键盘交互 ----------

/** 触发器上：展开态 Escape 收起；关闭态方向键展开（Enter/Space 交给原生 click） */
function onTriggerKeydown(e: KeyboardEvent) {
  if (props.disabled) return
  if (e.key === 'Escape' && open.value) {
    e.preventDefault()
    e.stopPropagation()
    close(true)
    return
  }
  if (!open.value && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
    e.preventDefault()
    void openPanel()
  }
}

/** 面板上：Escape / Tab 关闭，方向键在（过滤后的）选项间移动，搜索框回车选中首个命中 */
function onPanelKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    close(true)
    return
  }
  if (e.key === 'Tab') {
    close(false)
    return
  }
  const list = optionButtons()
  const active = document.activeElement as HTMLButtonElement | null
  const activeIdx = active ? list.indexOf(active) : -1
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault()
    if (!list.length) return
    if (activeIdx < 0) {
      // 焦点还在搜索框：Down 进列表头，Up 进列表尾
      ;(e.key === 'ArrowDown' ? list[0] : list[list.length - 1])?.focus()
      return
    }
    const next = e.key === 'ArrowDown' ? (activeIdx + 1) % list.length : (activeIdx - 1 + list.length) % list.length
    list[next]?.focus()
    return
  }
  if (e.key === 'Home' && activeIdx >= 0) {
    e.preventDefault()
    list[0]?.focus()
    return
  }
  if (e.key === 'End' && activeIdx >= 0) {
    e.preventDefault()
    list[list.length - 1]?.focus()
    return
  }
  if (e.key === 'Enter' && activeIdx < 0) {
    // 焦点在搜索框：回车选中首个（或唯一）过滤命中项
    const first = list[0]
    if (first) {
      e.preventDefault()
      first.click()
    }
  }
}

function onDocClick(e: MouseEvent) {
  const target = e.target as Node
  if (wrap.value?.contains(target)) return
  if (panel.value?.contains(target)) return // 面板已 Teleport 到 body，需单独排除
  open.value = false
}

onMounted(() => document.addEventListener('click', onDocClick))
onBeforeUnmount(() => {
  document.removeEventListener('click', onDocClick)
  window.removeEventListener('scroll', onOutsideScroll, true)
  window.removeEventListener('resize', onWindowResize)
})
</script>

<template>
  <div ref="wrap" class="relative w-full" @keydown="onTriggerKeydown">
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
      @click="open ? close(false) : openPanel()"
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
        <span aria-hidden="true" class="invisible col-start-1 row-start-1 min-w-0 truncate">{{ placeholderText }}</span>
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

    <!-- 面板挂到 body：fixed 定位按触发器实时计算，下方放不下自动翻到上方并贴边收拢 -->
    <Teleport to="body">
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
          ref="panel"
          role="listbox"
          class="fixed z-[80] rounded-lg border border-zinc-200 bg-white py-1 shadow-xl dark:border-zinc-700 dark:bg-zinc-800"
          :style="panelStyle"
          @keydown="onPanelKeydown"
        >
          <div
            v-if="searchable"
            class="flex items-center gap-1.5 border-b border-zinc-100 px-3 py-2 dark:border-zinc-700"
          >
            <Search class="h-3.5 w-3.5 shrink-0 text-zinc-400" />
            <input
              ref="searchInput"
              v-model="filter"
              type="text"
              class="w-full bg-transparent text-sm text-zinc-800 outline-none placeholder:text-zinc-400 dark:text-zinc-100"
              :placeholder="t('common.search')"
            />
          </div>
          <div class="max-h-60 overflow-y-auto">
            <button
              v-for="(opt, i) in filteredOptions"
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
            <p v-if="!filteredOptions.length" class="px-3 py-2 text-xs text-zinc-400">
              {{ t('common.noData') }}
            </p>
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>
