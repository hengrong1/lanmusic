<script setup lang="ts">
const props = withDefaults(
  defineProps<{
    modelValue: string
    disabled?: boolean
    title?: string
    /** 预设色板（可选）：传入后显示一排可点选的小色块，点击即选中 */
    presets?: string[]
  }>(),
  {
    disabled: false,
    presets: () => [],
  },
)

const emit = defineEmits<{ 'update:modelValue': [value: string] }>()

/** 当前值与预设是否一致（忽略大小写；预设与值均为 6 位 hex） */
function isPreset(c: string) {
  return props.modelValue.toLowerCase() === c.toLowerCase()
}

function onInput(e: Event) {
  emit('update:modelValue', (e.target as HTMLInputElement).value)
}
</script>

<template>
  <div class="inline-flex flex-col items-end gap-1.5">
    <label
      class="relative inline-flex h-8 w-14 shrink-0 cursor-pointer items-center justify-center overflow-hidden rounded-lg border border-zinc-200 bg-white transition-all duration-150 hover:border-violet-400 hover:ring-2 hover:ring-violet-500/10 focus-within:border-violet-500 focus-within:ring-2 focus-within:ring-violet-500/10 dark:border-zinc-700 dark:bg-zinc-800"
      :class="{ 'cursor-not-allowed opacity-40': disabled }"
      :title="title"
    >
      <span class="pointer-events-none absolute inset-1 rounded-md shadow-inner" :style="{ background: modelValue }" />
      <input
        type="color"
        :value="modelValue"
        :disabled="disabled"
        class="absolute inset-0 h-full w-full cursor-pointer opacity-0 disabled:cursor-not-allowed"
        @input="onInput"
      />
    </label>
    <!-- 预设色板：当前色命中时显示 violet 环；悬停放大（与全局悬停动画口径一致） -->
    <div v-if="presets.length && !disabled" class="flex items-center gap-1.5">
      <button
        v-for="c in presets"
        :key="c"
        type="button"
        class="h-4 w-4 cursor-pointer rounded-full border border-black/10 transition-transform duration-150 hover:scale-125"
        :class="isPreset(c) ? 'ring-2 ring-violet-500 ring-offset-1 ring-offset-white dark:ring-offset-zinc-800' : ''"
        :style="{ background: c }"
        :title="c"
        @click="emit('update:modelValue', c)"
      />
    </div>
  </div>
</template>
