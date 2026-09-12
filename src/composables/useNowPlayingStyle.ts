import { ref, watch } from 'vue'

/**
 * 播放页布局预设：
 * side = 左封面右歌词（经典/现状）、stacked = 上封面下歌词。
 */
export type NowPlayingStyleId = 'side' | 'stacked'

const LS_KEY = 'lm.npStyle'
const IDS: NowPlayingStyleId[] = ['side', 'stacked']

function load(): NowPlayingStyleId {
  const raw = localStorage.getItem(LS_KEY)
  return IDS.includes(raw as NowPlayingStyleId) ? (raw as NowPlayingStyleId) : 'side'
}

// 模块级单例：装扮面板写入，NowPlayingView 消费
const npStyle = ref<NowPlayingStyleId>(load())
watch(npStyle, (v) => localStorage.setItem(LS_KEY, v))

export function useNowPlayingStyle() {
  return npStyle
}
