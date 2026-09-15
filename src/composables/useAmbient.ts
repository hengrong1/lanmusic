import { computed, ref } from 'vue'
import { coverUrl } from '@/api/scheme'
import { extractAmbient, type AmbientPalette } from '@/utils/color'
import { themeAmbientPalette } from './useThemeColor'
import { npAccentMode } from './useNowPlayingStyle'

// 模块级单例：播放页负责提取，播放条等组件共享读取
const extracted = ref<AmbientPalette | null>(null)
let lastAlbumId: number | null | undefined

/** 跟随当前歌曲专辑更新环境色（幂等，切歌时自动失效旧结果） */
async function setAlbum(id?: number | null) {
  if (id === lastAlbumId) return
  lastAlbumId = id
  extracted.value = null
  if (id == null) return
  const url = coverUrl(id)
  if (!url) return
  const p = await extractAmbient(url)
  if (lastAlbumId === id && p) extracted.value = p
}

/**
 * 对外暴露的环境色：
 * - auto（装扮 → 主题色 → 自动）：封面主色不可用（无封面/提取失败/切歌间隙）时，
 *   回落到当前主题色派生的环境色（见 useThemeColor.themeAmbientPalette）。
 * - theme（装扮 → 主题色 → 跟随软件主题色）：无视封面，整体用主题色派生，
 *   播放页环境渐变/歌词强调/粒子/装扮面板与播放条强调色全部跟随设置里的主题色。
 */
const palette = computed<AmbientPalette>(() =>
  npAccentMode.value === 'theme'
    ? themeAmbientPalette()
    : (extracted.value ?? themeAmbientPalette()),
)

export function useAmbient() {
  return { palette, setAlbum }
}
