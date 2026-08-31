import { ref } from 'vue'
import { coverUrl } from '@/api/scheme'
import { extractAmbient, type AmbientPalette } from '@/utils/color'

// 模块级单例：播放页负责提取，播放条等组件共享读取
const palette = ref<AmbientPalette | null>(null)
let lastAlbumId: number | null | undefined

/** 跟随当前歌曲专辑更新环境色（幂等，切歌时自动失效旧结果） */
async function setAlbum(id?: number | null) {
  if (id === lastAlbumId) return
  lastAlbumId = id
  palette.value = null
  if (id == null) return
  const url = coverUrl(id)
  if (!url) return
  const p = await extractAmbient(url)
  if (lastAlbumId === id && p) palette.value = p
}

export function useAmbient() {
  return { palette, setAlbum }
}
