<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { BaseButton, BaseCheckbox, BaseModal } from '@/components/ui'
import { api } from '@/api/commands'

/**
 * 首次点击关闭按钮时的行为询问（见 src-tauri/src/lib.rs）：
 * Rust 侧读不到 `lm.closeAction`（首次）时不擅自决定，而是 prevent_close 并
 * emit `close-confirm-needed`，由本组件弹窗收集用户选择。
 *
 * 选择「记住」→ 写入 lm.closeAction（DB + localStorage 双写：设置页读后者、
 * Rust 读前者）；随后执行最小化到托盘或退出应用。
 * 取消 / Esc / 点遮罩 = 不关闭窗口（留在应用内）。
 */
const { t } = useI18n()

const open = ref(false)
/** 默认最小化到托盘：音乐播放器的常见使用场景（后台继续播放） */
const action = ref<'tray' | 'quit'>('tray')
const remember = ref(true)

void listen('close-confirm-needed', () => {
  action.value = 'tray'
  remember.value = true
  open.value = true
})

async function confirm() {
  open.value = false
  if (remember.value) {
    // 双写保持一致：设置页读 localStorage，Rust 侧读 DB
    localStorage.setItem('lm.closeAction', action.value)
    try {
      await api.setSetting('lm.closeAction', action.value)
    } catch {
      // 写入失败仅影响「下次是否再问」，不阻塞关闭动作
    }
  }
  if (action.value === 'quit') {
    await api.exitApp()
  } else {
    await getCurrentWindow().hide()
  }
}
</script>

<template>
  <!-- layer=prompt：窗口关闭确认必须能压在别的一切弹窗之上（例如设置里开着「已移除歌曲」时点关闭） -->
  <BaseModal :open="open" :title="t('closeDialog.title')" size="sm" layer="prompt" @close="open = false">
    <p class="text-sm leading-relaxed text-zinc-500 dark:text-zinc-300">{{ t('closeDialog.desc') }}</p>

    <!-- 行为选项：点卡片即选中，选中态 violet 描边 -->
    <div class="mt-4 space-y-2">
      <button
        type="button"
        class="w-full cursor-pointer rounded-xl border p-3 text-left transition-colors"
        :class="
          action === 'tray'
            ? 'border-violet-400 bg-violet-50 dark:border-violet-500/60 dark:bg-violet-500/10'
            : 'hover-accent-border border-zinc-200 hover:bg-zinc-50 dark:border-zinc-700 dark:hover:bg-zinc-800/60'
        "
        @click="action = 'tray'"
      >
        <span class="block text-sm font-medium text-zinc-800 dark:text-zinc-100">{{ t('closeDialog.tray') }}</span>
        <span class="mt-0.5 block text-xs text-zinc-500 dark:text-zinc-400">{{ t('closeDialog.trayDesc') }}</span>
      </button>
      <button
        type="button"
        class="w-full cursor-pointer rounded-xl border p-3 text-left transition-colors"
        :class="
          action === 'quit'
            ? 'border-violet-400 bg-violet-50 dark:border-violet-500/60 dark:bg-violet-500/10'
            : 'hover-accent-border border-zinc-200 hover:bg-zinc-50 dark:border-zinc-700 dark:hover:bg-zinc-800/60'
        "
        @click="action = 'quit'"
      >
        <span class="block text-sm font-medium text-zinc-800 dark:text-zinc-100">{{ t('closeDialog.quit') }}</span>
        <span class="mt-0.5 block text-xs text-zinc-500 dark:text-zinc-400">{{ t('closeDialog.quitDesc') }}</span>
      </button>
    </div>

    <BaseCheckbox v-model="remember" :label="t('closeDialog.remember')" class="mt-4" />
    <p class="mt-2 text-xs text-zinc-400">{{ t('closeDialog.changeHint') }}</p>

    <div class="mt-5 flex justify-end gap-2">
      <BaseButton variant="ghost" size="sm" @click="open = false">{{ t('common.cancel') }}</BaseButton>
      <BaseButton size="sm" @click="confirm">{{ t('common.ok') }}</BaseButton>
    </div>
  </BaseModal>
</template>
