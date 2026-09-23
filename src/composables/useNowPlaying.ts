import { ref, watch } from 'vue'
import { usePlayerStore } from '@/stores/player'
import { api } from '@/api/commands'

// 系统级「正在播放」（Windows SMTC / macOS Now Playing / Linux MPRIS）：
// 前端把当前曲目元数据与播放状态推给 Rust（souvlaki 转成系统调用，见 now_playing.rs）。
// - 切歌 → 元数据（+紧跟一次进度，因 set_metadata 会重置系统侧时间轴）
// - 播放/暂停 → 状态
// - 1Hz 心跳 → 进度（让系统浮层的进度条与实际播放保持一致；后台节流时系统会按倍率外推，无害）
// - 暂停期间每 30s 重发一次纯状态（对账）：纠正可能丢失的暂停推送（挂机场景下
//   前端 timer 被强节流，状态翻转的推送与暂停动作之间存在竞争窗口）。
//   Rust 侧对「暂停+位置未变」的重发只调 SetPlaybackStatus（幂等、不碰时间轴），
//   对 Win11 塌缩怪癖安全（那条禁的是完整推送/时间轴）。
// 反方向（系统按钮/拖动）由 Rust 广播 `media-control` 事件，useMediaControls.ts 统一消费。
//
// 开关默认开启（'0' 才算关，项目约定）；关闭时启动对账即通知 Rust 解除注册。
// 与「全局媒体键」（useMediaControls）互斥，互斥动作在设置面板（AudioEffectsPanel）里做。

const LS = 'lm.nowPlaying'
let started = false

/** 默认开启：键不存在或非 '0' 都视为开（默认开开关存 '0' 才算关） */
export const nowPlayingEnabled = ref(localStorage.getItem(LS) !== '0')

export function useNowPlaying() {
  if (started) return { enabled: nowPlayingEnabled, setEnabled: setNowPlayingEnabled }
  started = true

  const player = usePlayerStore()

  function pushMeta() {
    const t = player.current
    void api
      .nowPlayingSet(
        t
          ? {
              title: t.title,
              artist: t.artist ?? null,
              album: t.album ?? null,
              durationMs: t.duration ? Math.round(t.duration * 1000) : null,
              albumId: t.albumId ?? null,
            }
          : null,
      )
      .catch(() => {})
    // 刚换曲进度已归零：立即同步一次，别等心跳（避免系统浮层短暂显示上一首的进度）
    pushState()
  }

  function pushState() {
    if (!nowPlayingEnabled.value) return
    void api
      .nowPlayingState(!player.audio.paused, Math.round(player.position * 1000))
      .catch(() => {})
  }

  watch(
    () => player.current,
    () => {
      if (nowPlayingEnabled.value) pushMeta()
    },
  )
  watch(
    () => player.playing,
    (v) => {
      if (!nowPlayingEnabled.value) return
      // 诊断留痕：系统侧状态与本地不一致（如「暂停了 SMTC 还显示播放」）时，
      // 用此日志对照推送是否发生/是否报错
      void api
        .frontendLog('info', `now-playing 状态推送: ${v ? 'Playing' : 'Paused'} @${Math.round(player.position * 1000)}ms`)
        .catch(() => {})
      pushState()
    },
  )
  let lastPausedReassert = 0
  window.setInterval(() => {
    if (!nowPlayingEnabled.value || !player.current) return
    if (!player.audio.paused) {
      lastPausedReassert = 0
      pushState()
      return
    }
    // 暂停对账：间隔 ≥30s 重发一次纯状态（后台节流下 tick 变稀，最多 ~1min 一次，够用）
    const now = Date.now()
    if (now - lastPausedReassert >= 30_000) {
      lastPausedReassert = now
      pushState()
    }
  }, 1000)

  // 启动对账：开启的用户恢复显示（restore 之后 current 变化会再推一次）；
  // 关闭的用户让 Rust 侧解除注册（Rust 默认已启用）
  if (nowPlayingEnabled.value) {
    pushMeta()
  } else {
    void api.nowPlayingEnable(false).catch(() => {})
  }

  return { enabled: nowPlayingEnabled, setEnabled: setNowPlayingEnabled }
}

/** 设置系统「正在播放」开关（设置面板调用） */
export async function setNowPlayingEnabled(v: boolean) {
  nowPlayingEnabled.value = v
  try {
    localStorage.setItem(LS, v ? '1' : '0')
  } catch {
    /* ignore */
  }
  try {
    await api.nowPlayingEnable(v)
  } catch {
    /* ignore */
  }
}
