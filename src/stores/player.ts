import { computed, markRaw, ref, watch } from 'vue'
import { defineStore } from 'pinia'
import { listen } from '@tauri-apps/api/event'
import type { Track } from '@/types'
import { trackStreamUrl } from '@/api/scheme'
import { api } from '@/api/commands'
import { toast } from '@/composables/useToast'
import { activeLineIndex, parseLrc, plainLines, type LrcLine } from '@/utils/lrc'

export type PlayMode = 'order' | 'loop' | 'one' | 'shuffle'

const LS = {
  volume: 'lm.volume',
  mode: 'lm.mode',
  muted: 'lm.muted',
  lastTrack: 'lm.lastTrack',
  lastPos: 'lm.lastPos',
  queue: 'lm.queue',
}

/** 队列快照：任何队列/索引变化时自动保存 */
function saveQueueSnapshot(queue: Track[], index: number) {
  try {
    localStorage.setItem('lm.queue', JSON.stringify({ ids: queue.map((t) => t.id), index }))
  } catch {
    /* ignore */
  }
}

export const usePlayerStore = defineStore('player', () => {
  // 单例 audio 元素（非响应式）
  const audio = markRaw(new Audio())
  audio.preload = 'auto'
  // 频谱用 captureStream 复制音频流，需要 CORS 干净的媒体源（流协议响应带 ACAO:*）
  audio.crossOrigin = 'anonymous'

  // ---------- 状态 ----------
  const queue = ref<Track[]>([])
  const index = ref(-1)
  const playing = ref(false)
  const buffering = ref(false)
  const position = ref(0)
  const duration = ref(0)
  const mode = ref<PlayMode>((localStorage.getItem(LS.mode) as PlayMode) || 'order')
  const volume = ref(Number(localStorage.getItem(LS.volume) ?? 1))
  const muted = ref(localStorage.getItem(LS.muted) === '1')

  // ---------- 歌词 ----------
  const lyricsLines = ref<LrcLine[] | null>(null)
  const lyricsPlain = ref<string[] | null>(null)
  const lyricsLoading = ref(false)
  const activeLyricIndex = computed(() =>
    lyricsLines.value ? activeLineIndex(lyricsLines.value, position.value) : -1,
  )

  async function loadLyrics(t: Track) {
    lyricsLines.value = null
    lyricsPlain.value = null
    lyricsLoading.value = true
    try {
      // 不以 hasLyrics 标志为前置条件：旧库的标志可能过期（快速导入/旧版本扫描）
      const raw = await api.getLyrics(t.id)
      if (!raw) return
      const { lines, synced } = parseLrc(raw)
      if (synced) {
        lyricsLines.value = lines
      } else {
        lyricsPlain.value = plainLines(raw)
      }
    } catch {
      /* 歌词获取失败静默忽略 */
    } finally {
      lyricsLoading.value = false
    }
  }

  const current = computed<Track | null>(() => queue.value[index.value] ?? null)

  // ---------- audio 事件收口 ----------
  audio.volume = muted.value ? 0 : volume.value
  audio.muted = muted.value

  audio.addEventListener('timeupdate', () => {
    position.value = audio.currentTime
    if (audio.currentTime > 0) localStorage.setItem(LS.lastPos, String(audio.currentTime))
  })
  audio.addEventListener('durationchange', () => {
    if (Number.isFinite(audio.duration)) duration.value = audio.duration
  })
  audio.addEventListener('playing', () => {
    playing.value = true
    buffering.value = false
  })
  audio.addEventListener('pause', () => {
    playing.value = false
  })
  audio.addEventListener('waiting', () => {
    buffering.value = true
  })
  audio.addEventListener('canplay', () => {
    buffering.value = false
  })
  audio.addEventListener('ended', () => {
    if (mode.value === 'one') {
      audio.currentTime = 0
      void audio.play().catch(() => {})
    } else {
      next()
    }
  })
  audio.addEventListener('error', () => {
    if (!current.value) return
    buffering.value = false
    playing.value = false
    toast(`播放失败：${current.value.title}`, 'error')
    // 连续失败保护：整轮队列都失败则停止，避免死循环
    errorStreak++
    if (errorStreak < queue.value.length) {
      setTimeout(() => next(true), 400)
    } else {
      errorStreak = 0
    }
  })

  let errorStreak = 0

  // ---------- 控制 ----------
  function load(t: Track, autoplay = true) {
    audio.src = trackStreamUrl(t.id)
    position.value = 0
    duration.value = t.duration ?? 0
    // 切换新歌立即进入加载态，直到 canplay/playing 事件清除
    buffering.value = true
    localStorage.setItem(LS.lastPos, '0')
    void loadLyrics(t)
    if (autoplay) {
      void audio.play().catch(() => {})
      // 播放统计（静默上报）
      api.reportPlay(t.id).catch(() => {})
    }
  }

  function playList(list: Track[], startIndex = 0) {
    if (!list.length) return
    queue.value = [...list]
    errorStreak = 0
    playAt(Math.max(0, Math.min(startIndex, list.length - 1)))
  }

  function playAt(i: number) {
    if (i < 0 || i >= queue.value.length) return
    index.value = i
    errorStreak = 0
    load(queue.value[i])
  }

  function toggle() {
    if (!current.value) return
    if (audio.paused) {
      void audio.play().catch(() => {})
    } else {
      audio.pause()
    }
  }

  function next(fromError = false) {
    const n = queue.value.length
    if (!n) return
    if (mode.value === 'shuffle' && n > 1) {
      let r = index.value
      while (r === index.value) r = Math.floor(Math.random() * n)
      playAt(r)
      return
    }
    const i = index.value + 1
    if (i >= n) {
      if (mode.value === 'loop') {
        playAt(0)
      } else if (!fromError) {
        audio.pause()
        position.value = 0
      }
    } else {
      playAt(i)
    }
  }

  function prev() {
    if (!queue.value.length) return
    if (position.value > 3) {
      audio.currentTime = 0
      return
    }
    const i = index.value - 1
    if (i < 0) {
      if (mode.value === 'loop') playAt(queue.value.length - 1)
      else audio.currentTime = 0
    } else {
      playAt(i)
    }
  }

  function seek(t: number) {
    if (!Number.isFinite(t)) return
    audio.currentTime = t
    position.value = t
  }

  function setVolume(v: number) {
    volume.value = Math.min(1, Math.max(0, v))
    if (volume.value > 0 && muted.value) muted.value = false
  }

  function toggleMute() {
    muted.value = !muted.value
  }

  // ---------- 队列操作 ----------
  function playNextInQueue(t: Track) {
    if (index.value === -1) {
      playList([t], 0)
      return
    }
    queue.value.splice(index.value + 1, 0, t)
    toast(`将在「${current.value?.title ?? ''}」后播放`)
  }

  function enqueue(t: Track) {
    if (index.value === -1) {
      playList([t], 0)
      return
    }
    queue.value.push(t)
    toast('已加入队列')
  }

  function removeFromQueue(i: number) {
    if (i < 0 || i >= queue.value.length) return
    queue.value.splice(i, 1)
    if (i < index.value) {
      index.value--
    } else if (i === index.value) {
      // 移除的是当前曲目：停在原地不自动播，指针指向下一首
      if (index.value >= queue.value.length) index.value = queue.value.length - 1
      playing.value = false
    }
  }

  function clearQueue() {
    queue.value = []
    index.value = -1
    audio.pause()
    audio.removeAttribute('src')
    playing.value = false
    position.value = 0
    duration.value = 0
  }

  // ---------- 持久化 ----------
  watch(volume, (v) => {
    audio.volume = v
    localStorage.setItem(LS.volume, String(v))
  })
  watch(muted, (m) => {
    audio.muted = m
    localStorage.setItem(LS.muted, m ? '1' : '0')
  })
  watch(mode, (m) => localStorage.setItem(LS.mode, m))
  watch(current, (t) => {
    if (t) localStorage.setItem(LS.lastTrack, String(t.id))
  })
  // 队列快照：内容或当前索引变化即保存
  watch(
    () => `${queue.value.map((t) => t.id).join(',')}#${index.value}`,
    () => saveQueueSnapshot(queue.value, index.value),
  )

  // 系统托盘控制（M2）
  void listen<string>('tray', (e) => {
    if (e.payload === 'toggle') toggle()
    else if (e.payload === 'next') next()
    else if (e.payload === 'prev') prev()
  })

  /** 切换当前歌曲的喜欢状态 */
  function toggleFav() {
    const t = current.value
    if (!t) return
    api
      .favoriteToggle(t.id, !t.fav)
      .then(() => {
        t.fav = !t.fav
      })
      .catch((e) => toast(String(e), 'error'))
  }

  /** 启动时恢复完整播放队列（快照中的歌曲已删除则跳过），并恢复歌词与进度 */
  async function restore() {
    let tracks: Track[] = []
    let startIndex = 0

    // 1. 队列快照
    const raw = localStorage.getItem(LS.queue)
    if (raw) {
      try {
        const snap = JSON.parse(raw) as { ids?: number[]; index?: number }
        if (Array.isArray(snap.ids) && snap.ids.length) {
          const fetched = await api.getTracksByIds(snap.ids).catch(() => [] as Track[])
          const byId = new Map(fetched.map((t) => [t.id, t]))
          tracks = snap.ids.map((id) => byId.get(id)).filter((t): t is Track => !!t)
          startIndex = Math.min(Math.max(0, snap.index ?? 0), Math.max(0, tracks.length - 1))
        }
      } catch {
        /* 快照损坏，走兜底 */
      }
    }

    // 2. 兜底：无快照时恢复最后一首
    if (!tracks.length) {
      const id = Number(localStorage.getItem(LS.lastTrack))
      if (!id) return
      const t = await api.getTrack(id).catch(() => null)
      if (!t) return
      tracks = [t]
      startIndex = 0
    }

    queue.value = tracks
    index.value = startIndex
    const first = tracks[startIndex]
    audio.src = trackStreamUrl(first.id)
    duration.value = first.duration ?? 0
    void loadLyrics(first)
    const pos = Number(localStorage.getItem(LS.lastPos) ?? 0)
    if (pos > 0) {
      const apply = () => {
        if (pos < (audio.duration || Infinity)) audio.currentTime = pos
      }
      audio.addEventListener('loadedmetadata', apply, { once: true })
    }
  }

  return {
    audio,
    queue,
    index,
    playing,
    buffering,
    position,
    duration,
    mode,
    volume,
    muted,
    current,
    lyricsLines,
    lyricsPlain,
    lyricsLoading,
    activeLyricIndex,
    playList,
    playAt,
    toggle,
    next,
    prev,
    seek,
    setVolume,
    toggleMute,
    playNextInQueue,
    enqueue,
    removeFromQueue,
    clearQueue,
    toggleFav,
    restore,
  }
})
