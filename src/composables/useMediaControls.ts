import { ref } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { usePlayerStore } from '@/stores/player'
import { api } from '@/api/commands'
import { toast } from '@/composables/useToast'
import { t as tr } from '@/i18n/translate'

// 系统媒体控制事件的统一消费端。两个来源共用 `media-control` 通道（payload 一致）：
// - 系统级「正在播放」（SMTC / Now Playing / MPRIS，见 now_playing.rs，默认开启）
// - 全局媒体键热键（见 media_controls.rs，设置开关；与上面互斥，面板里切换）
//
// 全局媒体键的开与关：开关存 localStorage；非 Windows 平台注册为空操作。

const LS = 'lm.mediaControls'
let started = false

export const mediaControlsEnabled = ref(localStorage.getItem(LS) === '1')

/** 事件 payload：action 为播放控制；seekto 携带绝对位置（毫秒，来自系统侧拖动/快进快退） */
interface MediaControlPayload {
  action: 'play' | 'pause' | 'playpause' | 'stop' | 'next' | 'prev' | 'seekto'
  positionMs?: number
}

/** 是否已初始化监听（模块单例，多次调用只初始化一次） */
export function useMediaControls() {
  if (!started) {
    started = true
    void init()
  }
  return { enabled: mediaControlsEnabled, setEnabled }
}

function init() {
  // 监听器与 App 同生命周期，无需注销
  void listen<MediaControlPayload>('media-control', (e) => handleButton(e.payload)).catch(() => {})
  // 启动时若之前开启过，重新注册系统媒体键
  if (mediaControlsEnabled.value) {
    void api.mediaControlsEnable(true).catch(() => {})
  }
}

function handleButton(p: MediaControlPayload) {
  const player = usePlayerStore()
  switch (p.action) {
    case 'playpause':
      player.toggle()
      break
    case 'play':
      if (player.audio.paused) player.toggle()
      break
    case 'pause':
    case 'stop':
      if (!player.audio.paused) player.toggle()
      break
    case 'next':
      player.next()
      break
    case 'prev':
      player.prev()
      break
    case 'seekto':
      if (typeof p.positionMs === 'number') player.seek(p.positionMs / 1000)
      break
  }
}

async function setEnabled(v: boolean) {
  mediaControlsEnabled.value = v
  try {
    localStorage.setItem(LS, v ? '1' : '0')
  } catch {
    /* ignore */
  }
  // 注册失败（个别键被其他播放器占用）时提示具体是哪些键，避免「开了却没反应」的困惑
  const failed = await api.mediaControlsEnable(v).catch(() => [] as string[])
  if (failed.length) toast(tr('toast.mediaKeysPartial', { keys: failed.join(' / ') }), 'error')
}
