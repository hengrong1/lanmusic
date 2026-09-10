<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{ text: string; keyword: string }>()

/** 按关键词把文本切成 [普通段, 命中段, ...]，大小写不敏感 */
const parts = computed(() => {
  const kw = props.keyword?.trim()
  if (!kw) return [{ hit: false, text: props.text }]
  const k = kw.toLowerCase()
  const out: { hit: boolean; text: string }[] = []
  let rest = props.text
  let idx = rest.toLowerCase().indexOf(k)
  let guard = 0
  while (idx >= 0 && guard++ < 40) {
    if (idx > 0) out.push({ hit: false, text: rest.slice(0, idx) })
    out.push({ hit: true, text: rest.slice(idx, idx + kw.length) })
    rest = rest.slice(idx + kw.length)
    idx = rest.toLowerCase().indexOf(k)
  }
  if (rest) out.push({ hit: false, text: rest })
  return out
})
</script>

<template>
  <template v-for="(p, i) in parts" :key="i">
    <mark
      v-if="p.hit"
      class="rounded-sm bg-violet-500/20 px-0.5 text-violet-700 dark:bg-violet-400/25 dark:text-violet-300"
      >{{ p.text }}</mark
    >
    <template v-else>{{ p.text }}</template>
  </template>
</template>