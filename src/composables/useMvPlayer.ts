import { computed, ref } from 'vue'
import type { Track } from '@/types'
import { api } from '@/api/commands'
import { toast } from '@/composables/useToast'
import { usePlayerStore } from '@/stores/player'
import { i18n } from '@/i18n'

/** 正在播放 MV 的曲目；null = 未打开 */
const track = ref<Track | null>(null)
/** 视频流地址（打开前经 get_mv_url 校验同名视频确实存在） */
const url = ref('')
const loading = ref(false)

/**
 * 页面内 MV 播放（Plyr 遮罩层，见 MvPlayer.vue）。
 * 全局单例状态：任意位置调用 open() 即可在当前页面弹出播放。
 */
export function useMvPlayer() {
  /** 打开某曲目的 MV；先校验同名视频文件存在（曲库 hasMv 标记可能过期） */
  async function open(t: Track) {
    if (track.value?.id === t.id || loading.value) return
    loading.value = true
    try {
      const mvUrl = await api.getMvUrl(t.id)
      if (!mvUrl) {
        toast(i18n.global.t('mv.notFound'), 'error')
        return
      }
      // 打开 MV 前暂停音乐，避免人声/伴奏两个声音同时播放
      usePlayerStore().audio.pause()
      url.value = mvUrl
      track.value = t
    } catch {
      toast(i18n.global.t('mv.loadFailed'), 'error')
    } finally {
      loading.value = false
    }
  }

  /** 关闭 MV 播放层（视频元素随 v-if 卸载自动停止） */
  function close() {
    track.value = null
    url.value = ''
  }

  return {
    track,
    url,
    loading,
    opened: computed(() => track.value !== null),
    open,
    close,
  }
}