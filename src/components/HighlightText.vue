<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{ text: string; keyword: string; fieldMatched?: boolean }>()

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

/** 显示文本里是否有字面命中 */
const hasHit = computed(() => parts.value.some((p) => p.hit))
</script>

<template>
  <!-- 单根 span：外部传入的 class（truncate / 字号颜色等）才能通过 attrs 继承生效。
       片段根节点（文本 + mark 混排）会丢 fallthrough 并触发 Vue 警告 -->
  <span>
    <!-- 后端标记该字段命中、但显示文本字面不含关键词（按合并前旧名 / 拼音搜索）：
         整段用主题色文字显示，与 mark 高亮同色系 -->
    <span v-if="fieldMatched && !hasHit" class="text-violet-700 dark:text-violet-300">{{ props.text }}</span>
    <template v-else>
      <template v-for="(p, i) in parts" :key="i">
        <mark
          v-if="p.hit"
          class="rounded-sm bg-violet-500/20 px-0.5 text-violet-700 dark:bg-violet-400/25 dark:text-violet-300"
          >{{ p.text }}</mark
        >
        <template v-else>{{ p.text }}</template>
      </template>
    </template>
  </span>
</template>
