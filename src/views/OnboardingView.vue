<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { BaseButton } from '@/components/ui'
import { api } from '@/api/commands'
import { setLocale } from '@/i18n'
import { useTheme } from '@/composables/useTheme'
import { useThemeColor } from '@/composables/useThemeColor'
import { useSpectrumMode } from '@/composables/useSkin'
import { toast } from '@/composables/useToast'
import { errorText } from '@/i18n/error'

/**
 * 首次启动引导（由 main.ts 按 DB 的 `app.onboarded` 决定渲染，完成/跳过时写标记）。
 *
 * 4 步：①语言 → ②外观预设（一键组合：主题模式 + 主题色 + 频谱）→ ③音乐来源 → ④完成。
 * 所有选择都写进常规偏好（与设置页同一套键），之后可随时在设置页细调。
 */
const { t } = useI18n()
const { setTheme } = useTheme()
const { setThemeColor } = useThemeColor()
const spectrumMode = useSpectrumMode()

const step = ref(0)
const totalSteps = 4

const locale = ref<'zh' | 'en'>(navigator.language?.startsWith('zh') ? 'zh' : 'en')
const preset = ref<'classic' | 'light' | 'deep'>('classic')
const scanRequested = ref(false)
const addingSource = ref(false)

onMounted(() => setLocale(locale.value))

/** 切换语言（立即生效，引导页文案随之变化） */
function pickLocale(next: 'zh' | 'en') {
  locale.value = next
  setLocale(next)
}

/** 应用外观预设：主题模式 + 主题色（null = 默认紫）+ 频谱一次性设定 */
function applyPreset(key: 'classic' | 'light' | 'deep') {
  preset.value = key
  if (key === 'classic') {
    setTheme('system') // 经典紫默认跟随系统亮暗（用户要求：主题默认跟随系统）
    setThemeColor(null)
    spectrumMode.value = 'particles'
  } else if (key === 'light') {
    setTheme('light')
    setThemeColor('blue')
    spectrumMode.value = 'none'
  } else {
    setTheme('dark')
    setThemeColor('cyan')
    spectrumMode.value = 'tree'
  }
}

/** 选择音乐文件夹并加入曲库（扫描在后台进行，进主界面即可见进度） */
async function pickMusicFolder() {
  try {
    const picked = await openDialog({ directory: true, title: t('ob.source.pickTitle') })
    if (typeof picked !== 'string' || !picked) return
    addingSource.value = true
    await api.addLocalSource(picked)
    scanRequested.value = true
    toast(t('ob.source.added'))
  } catch (e) {
    toast(errorText(e), 'error')
  } finally {
    addingSource.value = false
  }
}

/** 完成引导：写标记后重载，由 main.ts 改渲染主界面 */
async function finish() {
  try {
    await api.setSetting('app.onboarded', '1')
  } catch {
    // 写标记失败不阻塞进入应用（下次启动会再引导一次）
  }
  location.reload()
}

/** 进入下一步前先把当前步的选择落实（第 2 步离开时套用预设） */
function next() {
  if (step.value === 1) applyPreset(preset.value)
  step.value = Math.min(step.value + 1, totalSteps - 1)
}

function prev() {
  step.value = Math.max(step.value - 1, 0)
}
</script>

<template>
  <div class="flex h-screen w-screen flex-col bg-zinc-100 text-zinc-900 dark:bg-zinc-950 dark:text-zinc-100">
    <!-- 无边框窗口：顶部拖拽区 + 跳过 -->
    <div data-tauri-drag-region class="flex h-11 shrink-0 items-center justify-between px-4">
      <div data-tauri-drag-region class="text-xs font-medium text-zinc-400">LanMusic</div>
      <button
        class="cursor-pointer rounded-md px-2 py-1 text-xs text-zinc-400 transition-colors hover:bg-zinc-500/10 hover:text-zinc-600 dark:hover:text-zinc-200"
        @click="finish"
      >
        {{ t('ob.skip') }}
      </button>
    </div>

    <div class="flex flex-1 items-center justify-center p-6">
      <div class="w-[560px] rounded-2xl border border-zinc-200/70 bg-white p-7 dark:border-zinc-800 dark:bg-zinc-900">
        <!-- 步骤指示 -->
        <div class="mb-6 flex items-center gap-1.5">
          <span
            v-for="i in totalSteps"
            :key="i"
            class="h-1 rounded-full transition-all"
            :class="i - 1 <= step ? 'w-8 bg-violet-500' : 'w-4 bg-zinc-200 dark:bg-zinc-700'"
          ></span>
        </div>

        <!-- ① 欢迎与语言 -->
        <template v-if="step === 0">
          <h1 class="text-lg font-semibold">{{ t('ob.lang.title') }}</h1>
          <p class="mt-1.5 text-sm leading-relaxed text-zinc-500 dark:text-zinc-400">{{ t('ob.lang.desc') }}</p>
          <div class="mt-5 flex gap-2">
            <button
              v-for="opt in [
                { key: 'zh' as const, label: '简体中文' },
                { key: 'en' as const, label: 'English' },
              ]"
              :key="opt.key"
              type="button"
              class="flex-1 cursor-pointer rounded-xl border p-3 text-center text-sm transition-colors"
              :class="
                locale === opt.key
                  ? 'border-violet-400 bg-violet-50 font-medium dark:border-violet-500/60 dark:bg-violet-500/10'
                  : 'border-zinc-200 hover:bg-zinc-50 dark:border-zinc-700 dark:hover:bg-zinc-800/60'
              "
              @click="pickLocale(opt.key)"
            >
              {{ opt.label }}
            </button>
          </div>
        </template>

        <!-- ② 外观预设 -->
        <template v-else-if="step === 1">
          <h1 class="text-lg font-semibold">{{ t('ob.appearance.title') }}</h1>
          <p class="mt-1.5 text-sm leading-relaxed text-zinc-500 dark:text-zinc-400">{{ t('ob.appearance.desc') }}</p>
          <div class="mt-5 space-y-2">
            <button
              v-for="opt in [
                { key: 'classic' as const, name: t('ob.preset.classic'), desc: t('ob.preset.classicDesc'), color: '#8b5cf6', dark: true },
                { key: 'light' as const, name: t('ob.preset.light'), desc: t('ob.preset.lightDesc'), color: '#3b82f6', dark: false },
                { key: 'deep' as const, name: t('ob.preset.deep'), desc: t('ob.preset.deepDesc'), color: '#06b6d4', dark: true },
              ]"
              :key="opt.key"
              type="button"
              class="flex w-full cursor-pointer items-center gap-3 rounded-xl border p-3 text-left transition-colors"
              :class="
                preset === opt.key
                  ? 'border-violet-400 bg-violet-50 dark:border-violet-500/60 dark:bg-violet-500/10'
                  : 'border-zinc-200 hover:bg-zinc-50 dark:border-zinc-700 dark:hover:bg-zinc-800/60'
              "
              @click="applyPreset(opt.key)"
            >
              <!-- 迷你预览：预设底色 + 强调色画一个均衡器示意 -->
              <span
                class="flex h-10 w-14 shrink-0 items-end gap-1 rounded-lg border p-1.5"
                :class="opt.dark ? 'border-zinc-700 bg-zinc-800' : 'border-zinc-200 bg-zinc-100'"
              >
                <span class="h-3 w-1 rounded-sm" :style="{ backgroundColor: opt.color }"></span>
                <span class="h-5 w-1 rounded-sm" :style="{ backgroundColor: opt.color }"></span>
                <span class="h-7 w-1 rounded-sm" :style="{ backgroundColor: opt.color }"></span>
              </span>
              <span class="min-w-0">
                <span class="block text-sm font-medium text-zinc-800 dark:text-zinc-100">{{ opt.name }}</span>
                <span class="mt-0.5 block text-xs text-zinc-500 dark:text-zinc-400">{{ opt.desc }}</span>
              </span>
            </button>
          </div>
        </template>

        <!-- ③ 音乐来源 -->
        <template v-else-if="step === 2">
          <h1 class="text-lg font-semibold">{{ t('ob.source.title') }}</h1>
          <p class="mt-1.5 text-sm leading-relaxed text-zinc-500 dark:text-zinc-400">{{ t('ob.source.desc') }}</p>
          <div class="mt-5 flex items-center gap-3">
            <BaseButton size="sm" :loading="addingSource" @click="pickMusicFolder">
              {{ scanRequested ? t('ob.source.change') : t('ob.source.pick') }}
            </BaseButton>
            <span v-if="scanRequested" class="text-xs text-emerald-500">{{ t('ob.source.added') }}</span>
          </div>
          <p class="mt-3 text-xs leading-relaxed text-zinc-400">{{ t('ob.source.webdavHint') }}</p>
        </template>

        <!-- ④ 完成 -->
        <template v-else>
          <h1 class="text-lg font-semibold">{{ t('ob.done.title') }}</h1>
          <p class="mt-1.5 text-sm leading-relaxed text-zinc-500 dark:text-zinc-400">{{ t('ob.done.desc') }}</p>
          <ul class="mt-4 space-y-2 text-xs leading-relaxed text-zinc-500 dark:text-zinc-400">
            <li>· {{ t('ob.done.tipTray') }}</li>
            <li>· {{ t('ob.done.tipShortcuts') }}</li>
            <li>· {{ t('ob.done.tipUpdate') }}</li>
          </ul>
        </template>

        <!-- 操作区 -->
        <div class="mt-7 flex items-center justify-between">
          <BaseButton v-if="step > 0" variant="ghost" size="sm" @click="prev">{{ t('ob.prev') }}</BaseButton>
          <span v-else></span>
          <BaseButton v-if="step < totalSteps - 1" size="sm" @click="next">{{ t('ob.next') }}</BaseButton>
          <BaseButton v-else size="sm" @click="finish">{{ t('ob.start') }}</BaseButton>
        </div>
      </div>
    </div>
  </div>
</template>
