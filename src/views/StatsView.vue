<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { HeadphonesRoundIcon as HeadphonesBold } from '@solar-icons/vue/bold/headphones-round'
import { DatabaseIcon as Database } from '@solar-icons/vue/linear/database'
import { StethoscopeIcon as Stethoscope } from '@solar-icons/vue/linear/stethoscope'
import { useStatsEntry } from '@/composables/useStatsEntry'
import { useStagger } from '@/composables/useStagger'
import ListenPane from '@/components/stats/ListenPane.vue'
import HealthPane from '@/components/stats/HealthPane.vue'
import DiagnosticsPane from '@/components/stats/DiagnosticsPane.vue'
import { BaseTabs } from '@/components/ui'

const { t } = useI18n()
const { statsEnabled } = useStatsEntry()
const root = ref<HTMLElement | null>(null)
useStagger(root, computed(() => statsEnabled.value))

const tab = ref<'listen' | 'health' | 'diag'>('listen')
</script>

<template>
  <!-- 布局：标题与 Tab 固定在顶部，仅内容区滚动（滚动到页尾仍有底部留白） -->
  <div ref="root" class="flex h-full min-h-0 flex-col px-6">
    <div class="shrink-0 pt-5 pb-3">
      <h1 data-stagger class="mt-0.5 text-2xl font-bold text-zinc-900 dark:text-zinc-50">
        {{ $t('stats.title') }}
      </h1>
      <p data-stagger class="mt-1 text-xs text-zinc-400">{{ $t('stats.subtitle') }}</p>
    </div>

    <!-- 入口被隐藏时（用户从统计页内开启后关闭开关被弹回这里之外的路径不会到达；兜底提示） -->
    <template v-if="statsEnabled">
      <div class="shrink-0">
        <BaseTabs
          v-model="tab"
          :items="[
            { value: 'listen', label: t('stats.tabListen'), icon: HeadphonesBold },
            { value: 'health', label: t('stats.tabHealth'), icon: Database },
            { value: 'diag', label: t('stats.tabDiag'), icon: Stethoscope },
          ]"
        />
      </div>

      <!-- 滚动发生在内容区；pb-10 保证滚到页尾不贴边 -->
      <div class="min-h-0 flex-1 overflow-y-auto pt-5 pb-10">
        <ListenPane v-if="tab === 'listen'" />
        <HealthPane v-else-if="tab === 'health'" />
        <DiagnosticsPane v-else-if="tab === 'diag'" />
      </div>
    </template>
    <div v-else class="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 text-center">
      <p class="text-sm text-zinc-500 dark:text-zinc-400">{{ $t('stats.entryHidden') }}</p>
      <p class="text-xs text-zinc-400">{{ $t('stats.entryHiddenHint') }}</p>
    </div>
  </div>
</template>
