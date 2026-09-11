<script setup lang="ts">
import { computed } from 'vue'

const props = withDefaults(
  defineProps<{
    modelValue: number
    min?: number
    max?: number
    step?: number
    disabled?: boolean
  }>(),
  {
    min: 0,
    max: 100,
    step: 1,
    disabled: false,
  },
)

const emit = defineEmits<{ 'update:modelValue': [value: number] }>()

const percent = computed(() => {
  const range = props.max - props.min
  return range > 0 ? ((props.modelValue - props.min) / range) * 100 : 0
})

function onInput(e: Event) {
  emit('update:modelValue', Number((e.target as HTMLInputElement).value))
}
</script>

<template>
  <input
    type="range"
    class="slider w-full min-w-0 flex-1"
    :class="[{ 'pointer-events-none opacity-40': disabled }]"
    :min="min"
    :max="max"
    :step="step"
    :value="modelValue"
    :disabled="disabled"
    :style="{ '--fill': percent + '%' }"
    @input="onInput"
  />
</template>
