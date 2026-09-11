<script setup lang="ts">
import { ref } from 'vue'
import {
  BaseButton,
  BaseButtonGroup,
  BaseCheckbox,
  BaseSwitch,
  BaseRadio,
  BaseInput,
  BaseTextarea,
  BaseSelect,
  BaseDropdown,
  BaseSlider,
  BaseCard,
  BaseModal,
  BaseTabs,
} from '@/components/ui'
import type { SelectOption, DropdownItem, TabItem } from '@/components/ui'
import { HeartIcon as Heart } from '@solar-icons/vue/linear/heart'
import { TrashBin2Icon as Trash2 } from '@solar-icons/vue/linear/trash-bin-2'
import { SettingsIcon as Settings } from '@solar-icons/vue/linear/settings'
import { PlayIcon as Play } from '@solar-icons/vue/bold/play'

const btnGroupValue = ref('all')
const btnGroupItems = [
  { value: 'all', label: '全部' },
  { value: 'local', label: '本地' },
  { value: 'cloud', label: '云端', icon: Settings },
]
const checkbox1 = ref(true)
const checkbox2 = ref(false)
const switch1 = ref(true)
const switch2 = ref(false)
const radioValue = ref('option1')
const inputValue = ref('')
const textareaValue = ref('')
const selectValue = ref('zh')
const selectOptions: SelectOption[] = [
  { value: 'zh', label: '简体中文' },
  { value: 'en', label: 'English' },
]
const dropdownItems: DropdownItem[] = [
  { label: '播放', icon: Play, action: () => {} },
  { label: '收藏', icon: Heart, action: () => {} },
  { label: '删除', icon: Trash2, danger: true, action: () => {} },
]
const sliderValue = ref(60)
const modalOpen = ref(false)
const tabValue = ref('general')
const tabItems: TabItem[] = [
  { value: 'general', label: '通用' },
  { value: 'playback', label: '播放' },
]
</script>

<template>
  <div class="mx-auto max-w-3xl space-y-8 p-6">
    <h1 class="text-2xl font-bold text-zinc-900 dark:text-zinc-50">组件库演示</h1>
    <BaseCard>
      <h2 class="mb-4 text-lg font-semibold text-zinc-800 dark:text-zinc-100">按钮 Button</h2>
      <div class="flex flex-wrap items-center gap-3">
        <BaseButton variant="primary">主要</BaseButton>
        <BaseButton variant="secondary">次要</BaseButton>
        <BaseButton variant="primary" tone="danger">危险</BaseButton>
        <BaseButton variant="ghost">幽灵</BaseButton>
        <BaseButton variant="outline">边框</BaseButton>
      </div>
      <div class="mt-4 flex flex-wrap items-center gap-3">
        <BaseButton variant="ghost" tone="danger">幽灵危险</BaseButton>
        <BaseButton variant="outline" tone="danger">边框危险</BaseButton>
      </div>
      <div class="mt-4 flex flex-wrap items-center gap-3">
        <BaseButton size="xs">超小</BaseButton>
        <BaseButton size="sm">小型</BaseButton>
        <BaseButton size="md">中等</BaseButton>
        <BaseButton size="lg">大型</BaseButton>
      </div>
      <div class="mt-4 flex flex-wrap items-center gap-3">
        <BaseButton loading>加载中</BaseButton>
        <BaseButton disabled>禁用</BaseButton>
        <BaseButton rounded>圆形</BaseButton>
        <BaseButton block>块级按钮</BaseButton>
      </div>
    </BaseCard>
    <BaseCard>
      <h2 class="mb-4 text-lg font-semibold text-zinc-800 dark:text-zinc-100">按钮组</h2>
      <BaseButtonGroup v-model="btnGroupValue" :items="btnGroupItems" />
    </BaseCard>
    <BaseCard>
      <h2 class="mb-4 text-lg font-semibold text-zinc-800 dark:text-zinc-100">复选框</h2>
      <div class="flex flex-wrap items-center gap-6">
        <BaseCheckbox v-model="checkbox1" label="已选中" />
        <BaseCheckbox v-model="checkbox2" label="未选中" />
        <BaseCheckbox v-model="checkbox1" label="禁用" disabled />
      </div>
    </BaseCard>
    <BaseCard>
      <h2 class="mb-4 text-lg font-semibold text-zinc-800 dark:text-zinc-100">开关</h2>
      <div class="flex flex-wrap items-center gap-6">
        <BaseSwitch v-model="switch1" label="开启" />
        <BaseSwitch v-model="switch2" label="关闭" />
        <BaseSwitch v-model="switch1" label="禁用" disabled />
      </div>
    </BaseCard>
    <BaseCard>
      <h2 class="mb-4 text-lg font-semibold text-zinc-800 dark:text-zinc-100">单选框</h2>
      <div class="flex flex-wrap items-center gap-6">
        <BaseRadio v-model="radioValue" value="option1" label="选项一" />
        <BaseRadio v-model="radioValue" value="option2" label="选项二" />
        <BaseRadio v-model="radioValue" value="option3" label="禁用" disabled />
      </div>
    </BaseCard>
    <BaseCard>
      <h2 class="mb-4 text-lg font-semibold text-zinc-800 dark:text-zinc-100">输入框</h2>
      <div class="space-y-4">
        <BaseInput v-model="inputValue" placeholder="请输入内容..." />
        <BaseInput v-model="inputValue" placeholder="错误状态" error="请输入有效内容" />
        <BaseInput v-model="inputValue" placeholder="提示信息" hint="这是提示" />
        <BaseInput v-model="inputValue" placeholder="禁用" disabled />
      </div>
    </BaseCard>
    <BaseCard>
      <h2 class="mb-4 text-lg font-semibold text-zinc-800 dark:text-zinc-100">文本域</h2>
      <BaseTextarea v-model="textareaValue" placeholder="请输入..." :rows="3" />
    </BaseCard>
    <BaseCard>
      <h2 class="mb-4 text-lg font-semibold text-zinc-800 dark:text-zinc-100">下拉选择</h2>
      <div class="max-w-xs">
        <BaseSelect v-model="selectValue" :options="selectOptions" />
      </div>
    </BaseCard>
    <BaseCard>
      <h2 class="mb-4 text-lg font-semibold text-zinc-800 dark:text-zinc-100">下拉菜单</h2>
      <BaseDropdown :items="dropdownItems">
        <BaseButton variant="secondary">操作</BaseButton>
      </BaseDropdown>
    </BaseCard>
    <BaseCard>
      <h2 class="mb-4 text-lg font-semibold text-zinc-800 dark:text-zinc-100">滑块</h2>
      <BaseSlider v-model="sliderValue" :min="0" :max="100" />
    </BaseCard>
    <BaseCard padding="none">
      <BaseTabs v-model="tabValue" :items="tabItems" />
      <div class="p-4">
        <p class="text-sm text-zinc-500">{{ tabValue }} 内容</p>
      </div>
    </BaseCard>
    <BaseCard>
      <h2 class="mb-4 text-lg font-semibold text-zinc-800 dark:text-zinc-100">弹窗</h2>
      <BaseButton @click="modalOpen = true">打开弹窗</BaseButton>
      <BaseModal v-model:open="modalOpen" title="示例弹窗">
        <p class="text-sm text-zinc-500">弹窗内容</p>
        <template #footer>
          <BaseButton variant="ghost" @click="modalOpen = false">取消</BaseButton>
          <BaseButton @click="modalOpen = false">确认</BaseButton>
        </template>
      </BaseModal>
    </BaseCard>
  </div>
</template>
