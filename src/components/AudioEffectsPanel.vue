<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { BaseSwitch, BaseButton } from '@/components/ui'
import {
  eqEnabled,
  normEnabled,
  getEqState,
  setEqEnabled,
  applyEqPreset,
  setEqBandGain,
  setNormalizeEnabled,
  setNormalizeGain,
  EQ_FREQS,
  EQ_PRESETS,
} from '@/composables/useAudioGraph'
import { useSleepTimer, SLEEP_PRESETS } from '@/composables/useSleepTimer'
import { useMediaControls } from '@/composables/useMediaControls'
import { useNowPlaying, setNowPlayingEnabled } from '@/composables/useNowPlaying'
import { analyzeLoudness } from '@/composables/useLoudness'
import { usePlayerStore } from '@/stores/player'
import { toast } from '@/composables/useToast'
import { t as tr } from '@/i18n/translate'
import { IS_WIN } from '@/utils/platform'

const { t } = useI18n()
const player = usePlayerStore()
const sleep = useSleepTimer()
const media = useMediaControls()

// ---------- 均衡器 ----------
const eqOn = ref(eqEnabled.value)
const eqState = getEqState()
const preset = ref(eqState.preset)
const gains = ref<number[]>([...eqState.gains])

const presetOptions = computed(() => [
  ...Object.keys(EQ_PRESETS).map((k) => ({ value: k, label: t(`effects.preset.${k}`) })),
  { value: 'custom', label: t('effects.preset.custom') },
])

function fmtFreq(f: number): string {
  return f >= 1000 ? `${f / 1000}k` : `${f}`
}

function onEqToggle(v: boolean) {
  setEqEnabled(v)
  eqOn.value = v
}
function onPreset(v: string | number) {
  const key = String(v)
  // 「自定义」是手动微调后的状态标记，不是可应用的预设：点击不改增益，仅保持当前状态
  if (key === 'custom') {
    preset.value = 'custom'
    return
  }
  preset.value = key
  applyEqPreset(key)
  gains.value = [...(EQ_PRESETS[key] ?? EQ_PRESETS.flat)]
}
function onBand(i: number, db: number) {
  gains.value[i] = db
  setEqBandGain(i, db)
  // 手动微调后脱离原预设，标记为「自定义」
  preset.value = 'custom'
}
/** ±按钮微调某频段增益（dB），与滑块共用 onBand（自动标为自定义） */
function bumpBand(i: number, delta: number) {
  const next = Math.max(-12, Math.min(12, (gains.value[i] ?? 0) + delta))
  onBand(i, next)
}

// ---------- 音量归一化 ----------
const normOn = ref(normEnabled.value)
const analyzing = ref(false)
function onNormToggle(v: boolean) {
  setNormalizeEnabled(v)
  normOn.value = v
}
async function analyzeCurrent() {
  const cur = player.current
  if (!cur) {
    toast(tr('effects.normalizeNoTrack'), 'info')
    return
  }
  analyzing.value = true
  try {
    const r = await analyzeLoudness(cur.id, cur.duration)
    // 分析耗时（最长 64MB 拉取 + 解码），期间用户可能已切歌：
    // 增益照常记到分析的那首（cur）上，但共享音频图的归一化增益属于
    // 当前曲目，不能把旧曲增益误写到新曲的播放上
    cur.rgTrackGain = r.gainDb
    if (player.current?.id === cur.id) setNormalizeGain(r.gainDb)
    toast(tr('effects.normalizeAnalyzed', { db: r.gainDb.toFixed(1) }), 'info')
  } catch (e) {
    toast(tr('effects.normalizeFailed', { msg: String(e) }), 'error')
  } finally {
    analyzing.value = false
  }
}

// ---------- 睡眠定时器 ----------
const sleepRemaining = computed(() => {
  const s = sleep.remaining.value
  if (s <= 0) return ''
  const m = Math.floor(s / 60)
  const sec = s % 60
  return m > 0 ? `${m}:${String(sec).padStart(2, '0')}` : `${sec}s`
})

// ---------- 系统媒体键 ----------
const mediaOn = ref(media.enabled.value)
function onMediaToggle(v: boolean) {
  void media.setEnabled(v)
  mediaOn.value = v
  // 与「系统正在播放」互斥：两者都会响应硬件媒体键，只留一个持有者
  if (v && npOn.value) {
    npOn.value = false
    void setNowPlayingEnabled(false)
    toast(tr('toast.nowPlayingTookOver'), 'info')
  }
}

// ---------- 系统级「正在播放」（SMTC / Now Playing / MPRIS） ----------
const np = useNowPlaying()
const npOn = ref(np.enabled.value)
function onNpToggle(v: boolean) {
  void setNowPlayingEnabled(v)
  npOn.value = v
  if (v && mediaOn.value) {
    void media.setEnabled(false)
    mediaOn.value = false
    toast(tr('toast.mediaKeysTookOver'), 'info')
  }
}
</script>

<template>
  <div class="space-y-6 select-none">
    <!-- 均衡器 -->
    <section>
      <div class="mb-2.5 flex items-center justify-between gap-3">
        <h3 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ t('effects.eq') }}</h3>
        <div class="flex shrink-0 items-center gap-1.5 text-xs text-zinc-500">
          {{ t('effects.eqEnable') }}
          <BaseSwitch :model-value="eqOn" size="sm" @update:model-value="onEqToggle" />
        </div>
      </div>
      <div
        class="rounded-2xl shadow-sm hover-accent-border border border-zinc-200 bg-white p-4 text-sm dark:border-zinc-800 dark:bg-zinc-900"
        :class="eqOn ? '' : 'opacity-60'"
      >
        <div class="mb-3 flex flex-wrap gap-1.5">
          <button
            v-for="opt in presetOptions"
            :key="opt.value"
            type="button"
            class="cursor-pointer rounded-lg border px-2.5 py-1 text-xs transition-colors"
            :class="preset === opt.value
              ? 'border-violet-500 bg-violet-50 text-violet-600 dark:bg-violet-500/10 dark:text-violet-400'
              : 'hover-accent-border border-zinc-200 bg-zinc-50 text-zinc-600 hover:text-violet-600 dark:border-zinc-700 dark:bg-zinc-800/60 dark:text-zinc-300 dark:hover:text-violet-300'"
            :disabled="!eqOn"
            @click="onPreset(opt.value)"
          >
            {{ opt.label }}
          </button>
        </div>
        <div class="flex items-end justify-between gap-1 overflow-x-auto pb-1">
          <div v-for="(f, i) in EQ_FREQS" :key="f" class="flex w-12 shrink-0 flex-col items-center gap-1.5">
            <span class="tabular-nums text-[10px] text-zinc-400">{{ gains[i] > 0 ? '+' : '' }}{{ gains[i] }}</span>
            <button
              type="button"
              class="flex h-5 w-5 cursor-pointer items-center justify-center rounded-md border border-zinc-200 bg-zinc-50 text-sm leading-none text-zinc-500 transition-colors hover-accent-border hover:bg-violet-100 hover:text-violet-700 disabled:cursor-not-allowed disabled:opacity-40 dark:border-zinc-700 dark:bg-zinc-800/60 dark:text-zinc-400 dark:hover:bg-violet-500/20 dark:hover:text-violet-300"
              :disabled="!eqOn"
              @click="bumpBand(i, -1)"
            >−</button>
            <input
              type="range"
              min="-12"
              max="12"
              step="0.5"
              :value="gains[i]"
              :disabled="!eqOn"
              class="eq-slider"
              :style="{ '--v': ((gains[i] + 12) / 24) * 100 + '%' }"
              @input="(e) => onBand(i, Number((e.target as HTMLInputElement).value))"
            />
            <button
              type="button"
              class="flex h-5 w-5 cursor-pointer items-center justify-center rounded-md border border-zinc-200 bg-zinc-50 text-sm leading-none text-zinc-500 transition-colors hover-accent-border hover:bg-violet-100 hover:text-violet-700 disabled:cursor-not-allowed disabled:opacity-40 dark:border-zinc-700 dark:bg-zinc-800/60 dark:text-zinc-400 dark:hover:bg-violet-500/20 dark:hover:text-violet-300"
              :disabled="!eqOn"
              @click="bumpBand(i, 1)"
            >+</button>
            <span class="text-[10px] text-zinc-500">{{ fmtFreq(f) }}</span>
          </div>
        </div>
        <p class="mt-2 text-xs text-zinc-400">{{ t('effects.eqDesc') }}</p>
      </div>
    </section>

    <!-- 音量归一化 -->
    <section>
      <div class="mb-2.5 flex items-center justify-between gap-3">
        <h3 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ t('effects.normalize') }}</h3>
        <div class="flex shrink-0 items-center gap-1.5 text-xs text-zinc-500">
          {{ t('effects.normalizeEnable') }}
          <BaseSwitch :model-value="normOn" size="sm" @update:model-value="onNormToggle" />
        </div>
      </div>
      <div class="rounded-2xl shadow-sm hover-accent-border border border-zinc-200 bg-white p-4 text-sm dark:border-zinc-800 dark:bg-zinc-900">
        <p class="mb-3 text-xs leading-relaxed text-zinc-500">{{ t('effects.normalizeDesc') }}</p>
        <BaseButton size="sm" :loading="analyzing" :disabled="analyzing || !player.current" @click="analyzeCurrent">
          {{ t('effects.normalizeAnalyze') }}
        </BaseButton>
      </div>
    </section>

    <!-- 睡眠定时器 -->
    <section>
      <div class="mb-2.5 flex items-center justify-between gap-3">
        <h3 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ t('effects.sleep') }}</h3>
        <span v-if="sleepRemaining" class="tabular-nums text-xs text-violet-500">
          {{ t('effects.sleepRemaining', { time: sleepRemaining }) }}
        </span>
      </div>
      <div class="rounded-2xl shadow-sm hover-accent-border border border-zinc-200 bg-white p-4 text-sm dark:border-zinc-800 dark:bg-zinc-900">
        <div class="flex flex-wrap gap-2">
          <button
            v-for="m in SLEEP_PRESETS"
            :key="m"
            type="button"
            class="cursor-pointer rounded-lg border px-3 py-1.5 text-xs transition-colors"
            :class="
              sleep.mode.value === m
                ? 'border-violet-500 bg-violet-50 text-violet-600 dark:bg-violet-500/10 dark:text-violet-400'
                : 'hover-accent-border border-zinc-200 bg-zinc-50 text-zinc-600 hover:text-violet-600 dark:border-zinc-700 dark:bg-zinc-800/60 dark:text-zinc-300 dark:hover:text-violet-300'
            "
            @click="sleep.startSleepTimer(m)"
          >
            {{ t('effects.sleepMinutes', { minutes: m }) }}
          </button>
          <button
            type="button"
            class="cursor-pointer rounded-lg border px-3 py-1.5 text-xs transition-colors"
            :class="
              sleep.mode.value === 'endOfTrack'
                ? 'border-violet-500 bg-violet-50 text-violet-600 dark:bg-violet-500/10 dark:text-violet-400'
                : 'hover-accent-border border-zinc-200 bg-zinc-50 text-zinc-600 hover:text-violet-600 dark:border-zinc-700 dark:bg-zinc-800/60 dark:text-zinc-300 dark:hover:text-violet-300'
            "
            @click="sleep.startSleepTimerEndOfTrack()"
          >
            {{ t('effects.sleepEndOfTrack') }}
          </button>
          <button
            v-if="sleep.mode.value"
            type="button"
            class="hover-accent-border cursor-pointer rounded-lg border border-zinc-200 bg-zinc-50 px-3 py-1.5 text-xs text-zinc-500 transition-colors hover:text-violet-600 dark:border-zinc-700 dark:bg-zinc-800/60 dark:hover:text-violet-300"
            @click="sleep.cancelSleepTimer()"
          >
            {{ t('effects.sleepCancel') }}
          </button>
        </div>
        <p class="mt-3 text-xs text-zinc-400">{{ t('effects.sleepDesc') }}</p>
      </div>
    </section>

    <!-- 系统集成：系统级「正在播放」+ 全局媒体键热键（两者互斥，都响应硬件媒体键）。
         两项都是「开关+一句说明」的单行设置，合并一张列表组卡，减少零碎空卡 -->
    <section>
      <div class="rounded-2xl shadow-sm hover-accent-border border border-zinc-200 bg-white p-1.5 text-sm dark:border-zinc-800 dark:bg-zinc-900">
        <div class="flex items-center justify-between gap-3 rounded-lg px-3 py-2.5 transition-colors hover:bg-zinc-50 dark:hover:bg-zinc-800/60">
          <div class="min-w-0">
            <p class="text-zinc-700 dark:text-zinc-200">{{ t('effects.nowPlaying') }}</p>
            <p class="mt-0.5 text-xs text-zinc-400">{{ t('effects.nowPlayingDesc') }}</p>
          </div>
          <BaseSwitch :model-value="npOn" size="sm" @update:model-value="onNpToggle" />
        </div>
        <div
          v-if="IS_WIN"
          class="mt-0.5 flex items-center justify-between gap-3 rounded-lg px-3 py-2.5 transition-colors hover:bg-zinc-50 dark:hover:bg-zinc-800/60"
        >
          <div class="min-w-0">
            <p class="text-zinc-700 dark:text-zinc-200">{{ t('effects.mediaKeys') }}</p>
            <p class="mt-0.5 text-xs text-zinc-400">{{ t('effects.mediaKeysDesc') }}</p>
          </div>
          <BaseSwitch :model-value="mediaOn" size="sm" @update:model-value="onMediaToggle" />
        </div>
      </div>
    </section>
  </div>
</template>

<style scoped>
/* 均衡器竖滑杆：WebAudio 风格的细窄滑块，轨道按当前增益值上色（紫） */
.eq-slider {
  -webkit-appearance: none;
  appearance: none;
  writing-mode: vertical-lr;
  direction: rtl;
  width: 6px;
  height: 120px;
  border-radius: 9999px;
  /* 进度（已增益部分）跟随主题强调色；未增益部分为中性 zinc */
  background: linear-gradient(
    to top,
    var(--color-violet-500) 0%,
    var(--color-violet-500) var(--v, 50%),
    var(--color-zinc-300) var(--v, 50%),
    var(--color-zinc-300) 100%
  );
  outline: none;
  cursor: pointer;
}
.eq-slider:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.eq-slider::-webkit-slider-thumb {
  -webkit-appearance: none;
  appearance: none;
  width: 14px;
  height: 14px;
  border-radius: 9999px;
  background: #fff;
  border: 2px solid var(--color-violet-500);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
}
.eq-slider::-moz-range-thumb {
  width: 14px;
  height: 14px;
  border-radius: 9999px;
  background: #fff;
  border: 2px solid var(--color-violet-500);
}
.dark .eq-slider {
  /* 深色下强调色用 violet-400（更亮，匹配暗背景），未增益部分为 zinc-700 */
  background: linear-gradient(
    to top,
    var(--color-violet-400) 0%,
    var(--color-violet-400) var(--v, 50%),
    var(--color-zinc-700) var(--v, 50%),
    var(--color-zinc-700) 100%
  );
}
.dark .eq-slider::-webkit-slider-thumb {
  border-color: var(--color-violet-400);
}
.dark .eq-slider::-moz-range-thumb {
  border-color: var(--color-violet-400);
}
</style>
