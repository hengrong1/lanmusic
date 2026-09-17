<script setup lang="ts">
import { computed } from 'vue'
import { Rewind5SecondsBackIcon as RewindBack } from '@solar-icons/vue/linear/rewind-5-seconds-back'
import { Rewind5SecondsForwardIcon as RewindForward } from '@solar-icons/vue/linear/rewind-5-seconds-forward'
import { RestartIcon as RotateCcw } from '@solar-icons/vue/linear/restart'
import { usePlayerStore } from '@/stores/player'
import { useNav } from '@/composables/useNav'
import { useI18n } from 'vue-i18n'
import LyricsPanel from '@/components/LyricsPanel.vue'

withDefaults(defineProps<{ align?: 'center' | 'left' }>(), { align: 'center' })
const emit = defineEmits<{ navigate: [] }>()

const player = usePlayerStore()
const nav = useNav()
const { t: tr } = useI18n()

// ---- 音质信息：格式 / 采样率 / 位深；≥88.2kHz 或 ≥24bit 标记 Hi-Res ----
const quality = computed(() => {
  const t = player.current
  if (!t) return null
  const parts = [
    t.format?.toUpperCase(),
    t.sampleRate ? `${(t.sampleRate / 1000).toFixed(1).replace(/\.0$/, '')}kHz` : '',
    t.bitDepth ? `${t.bitDepth}bit` : '',
  ].filter(Boolean)
  if (!parts.length) return null
  return { text: parts.join(' · '), hires: (t.sampleRate ?? 0) >= 88200 || (t.bitDepth ?? 0) >= 24 }
})

function openAlbum() {
  const cur = player.current
  if (cur?.albumId == null) return
  nav.go({ view: 'tracks', albumId: cur.albumId, albumTitle: cur.album ?? tr('album.unknownAlbum') })
  emit('navigate')
}

/** 当前曲目的艺人列表：优先用后端拆分的多艺人，回退到合并字符串 */
const currentArtistLinks = computed<{ id: number | null; name: string }[]>(() => {
  const cur = player.current
  if (!cur) return []
  if (cur.artists?.length) return cur.artists.map((a) => ({ id: a.id, name: a.name }))
  if (cur.artist) return [{ id: cur.artistId, name: cur.artist }]
  return [{ id: null, name: tr('artist.unknownArtist') }]
})

function openArtist(artist: { id: number | null; name: string }) {
  if (artist.id == null) return
  nav.go({ view: 'tracks', artistId: artist.id, artistName: artist.name })
  emit('navigate')
}

/** 带参提示（模板 `$t` 无带参重载，统一在 setup 内生成） */
const artistTip = (name: string) => tr('artist.viewArtist', { name })
/** 已累计偏移的悬停提示后缀：如「，已累计提前 1.0s」；无偏移时为空串 */
const offsetTip = computed(() => {
  const v = player.lyricOffset
  if (!v) return ''
  return tr(v > 0 ? 'player.lyricOffsetLate' : 'player.lyricOffsetEarly', { value: Math.abs(v).toFixed(1) })
})
const lyricBackTip = computed(() => tr('player.lyricBackHint') + offsetTip.value)
const lyricForwardTip = computed(() => tr('player.lyricForwardHint') + offsetTip.value)
const lyricResetTip = computed(() =>
  player.lyricOffset ? tr('player.lyricResetHint') + offsetTip.value : tr('player.lyricCurrentHint'),
)

/** 校准按钮悬停态：底色与图标跟随播放页主题色（--np-accent 由 App 随环境色下发） */
const calibHover =
  'hover:bg-[color-mix(in_srgb,var(--np-accent,#a78bfa)_18%,transparent)] hover:text-[var(--np-accent,#a78bfa)]'

/** 副行开关（音 / 译）的选中态：强调色文字 + 同色半透明底，与校准按钮的悬停态同配方，
 *  开着与关着一眼可辨（关着只有很淡的白色描字） */
const subToggleOn =
  'bg-[color-mix(in_srgb,var(--np-accent,#a78bfa)_18%,transparent)] text-[var(--np-accent,#a78bfa)]'
</script>

<template>
  <div class="np-fade relative flex min-w-0 flex-1 flex-col">
    <!-- 曲目信息：align 控制居中（经典/上下）或靠左（黑胶） -->
    <div
      class="flex shrink-0 flex-col pb-4 pt-6"
      :class="align === 'left' ? 'items-start text-left' : 'items-center text-center'"
    >
      <h1 class="max-w-full truncate text-2xl font-bold text-white">{{ player.current?.title ?? $t('player.notPlaying') }}</h1>
      <!-- 多艺人：每个名字独立可点击（区分每一个艺人）。
           合唱曲目动辄十几位，整行 truncate 会把后面的合唱者直接吃掉 → 这里按 flex-wrap 换行；
           每个「名字 + 后面的分隔符」绑成一组，避免「/」被甩到行首单独占位 -->
      <div
        class="mt-1 flex max-w-full flex-wrap gap-x-1.5 gap-y-0.5 text-sm leading-snug text-white/60"
        :class="align === 'left' ? 'justify-start' : 'justify-center'"
      >
        <span
          v-for="(a, i) in currentArtistLinks"
          :key="a.id ?? `na-${i}`"
          class="inline-flex max-w-full min-w-0 items-baseline gap-1.5"
        >
          <button
            v-if="a.id != null"
            class="max-w-full cursor-pointer truncate transition hover:text-white hover:underline"
            v-tooltip="artistTip(a.name)"
            @click="openArtist(a)"
          >{{ a.name }}</button>
          <span v-else class="max-w-full truncate">{{ a.name }}</span>
          <span v-if="i < currentArtistLinks.length - 1" class="shrink-0 opacity-40">/</span>
        </span>
      </div>
      <button
        v-if="player.current?.albumId != null"
        class="mt-0.5 max-w-full cursor-pointer truncate text-xs text-white/40 transition hover:text-white/80"
        v-tooltip="$t('album.goToAlbum')"
        @click="openAlbum"
      >
        {{ player.current?.album }}
      </button>
      <!-- 音质徽标：格式/采样率/位深，Hi-Res（高解析度）金色标识 -->
      <p
        v-if="quality"
        class="mt-2 flex items-center gap-1.5 text-[11px] text-white/40"
        :class="align === 'left' ? 'justify-start' : 'justify-center'"
      >
        <span class="font-mono">{{ quality.text }}</span>
        <span
          v-if="quality.hires"
          class="rounded border border-amber-300/50 bg-amber-300/10 px-1.5 py-px font-bold text-amber-200"
          v-tooltip="$t('player.hiResHint')"
        >Hi-Res</span>
      </p>
    </div>
    <!-- 歌词校准：固定在右下角（绝对定位不占布局），三个按钮竖排：快退(延后)/还原/快进(提前)，
         同一首歌内点击累计（按曲目记忆持久化）；已累计量在按钮悬停提示中显示 -->
    <div
      v-if="player.lyricsLines?.length"
      class="absolute right-0 bottom-2 z-10 flex flex-col items-center gap-1 rounded-2xl bg-black/40 px-1.5 py-2 backdrop-blur-sm"
    >
      <button
        class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-lg text-white/60 transition"
        :class="calibHover"
        v-tooltip="lyricBackTip"
        @click="player.setLyricOffset(0.5)"
      >
        <RewindBack class="h-5 w-5" />
      </button>
      <button
        class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-lg transition"
        :class="player.lyricOffset ? `text-white/60 ${calibHover}` : 'text-white/25'"
        v-tooltip="lyricResetTip"
        @click="player.setLyricOffset(-player.lyricOffset)"
      >
        <RotateCcw class="h-4.5 w-4.5" />
      </button>
      <button
        class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-lg text-white/60 transition"
        :class="calibHover"
        v-tooltip="lyricForwardTip"
        @click="player.setLyricOffset(-0.5)"
      >
        <RewindForward class="h-5 w-5" />
      </button>
      <!-- 歌词副行显隐：音译（音）与译文（译）两个文字按钮，仅当前歌词确实带该副行时才出现
           （没检测到就整颗隐藏，不留空位）；与上面的时间校准按钮之间用细分隔线分组，
           **默认关闭**、按曲目记忆，点开后用强调色底 + 强调色字表示开着（见 subToggleOn） -->
      <template v-if="player.hasLyricTransliteration || player.hasLyricTranslation">
        <span class="my-0.5 h-px w-5 shrink-0 bg-white/15"></span>
        <button
          v-if="player.hasLyricTransliteration"
          class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-lg text-[13px] font-medium transition"
          :class="player.lyricTransliteration ? subToggleOn : 'text-white/25 hover:text-white/60'"
          v-tooltip="
            player.lyricTransliteration ? $t('lyrics.transliterationHide') : $t('lyrics.transliterationShow')
          "
          @click="player.toggleLyricTransliteration()"
        >
          音
        </button>
        <button
          v-if="player.hasLyricTranslation"
          class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-lg text-[13px] font-medium transition"
          :class="player.lyricTranslation ? subToggleOn : 'text-white/25 hover:text-white/60'"
          v-tooltip="player.lyricTranslation ? $t('lyrics.translationHide') : $t('lyrics.translationShow')"
          @click="player.toggleLyricTranslation()"
        >
          译
        </button>
      </template>
    </div>
    <div class="min-h-0 flex-1">
      <LyricsPanel :align="align" />
    </div>
  </div>
</template>
