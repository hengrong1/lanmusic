import { computed, markRaw, ref, watch } from 'vue'
import { defineStore } from 'pinia'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import type { Track } from '@/types'
import { trackStreamUrl } from '@/api/scheme'
import { api } from '@/api/commands'
import { useLibraryStore } from '@/stores/library'
import { toast } from '@/composables/useToast'
import { t as tr } from '@/i18n/translate'
import {
  activeLineIndex,
  foldQrcSubLines,
  parseLrc,
  parseWordLrc,
  plainLines,
  type LrcLine,
} from '@/utils/lrc'
import { looksBinaryish, looksLikeHexQrc, qrcToLrcLines } from '@/utils/qrc'
import { insertNextAfter } from '@/utils/queue'
import type { QrcLine } from '@/types'
import { applyPowerGuard } from '@/composables/usePowerGuard'
import { consumeSleepStop } from '@/composables/useSleepTimer'
import { setNormalizeGain, normEnabled } from '@/composables/useAudioGraph'
import { errorText } from '@/i18n/error'

export type PlayMode = 'order' | 'loop' | 'one' | 'shuffle'

/** 播放模式白名单：存档被外部改坏时回退到 order，避免 UI 图标与实际行为对不上 */
const PLAY_MODES: PlayMode[] = ['order', 'loop', 'one', 'shuffle']
function normalizeMode(v: unknown): PlayMode {
  return PLAY_MODES.includes(v as PlayMode) ? (v as PlayMode) : 'order'
}

/** 音量默认值：50%（仅首次启动/存档损坏时生效，已存档用户保持自己调过的音量） */
const DEFAULT_VOLUME = 0.5

/** 音量归一化：存档损坏（NaN/越界）时回退默认值，避免 audio.volume = NaN 抛 TypeError */
function normalizeVolume(v: unknown): number {
  const n = Number(v)
  if (!Number.isFinite(n)) return DEFAULT_VOLUME
  return Math.min(1, Math.max(0, n))
}

const LS = {
  volume: 'lm.volume',
  mode: 'lm.mode',
  muted: 'lm.muted',
  rate: 'lm.rate',
  lastTrack: 'lm.lastTrack',
  lastPos: 'lm.lastPos',
  queue: 'lm.queue',
}

/** 可选播放倍速（PlayerBar 点击循环切换） */
export const PLAYBACK_RATES = [0.5, 0.75, 1, 1.25, 1.5, 2]

/** 校验倍速值：不在可选列表内则回退到 1x */
function normalizeRate(v: number): number {
  return PLAYBACK_RATES.includes(v) ? v : 1
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
  const mode = ref<PlayMode>(normalizeMode(localStorage.getItem(LS.mode)))
  const volume = ref(normalizeVolume(localStorage.getItem(LS.volume) ?? DEFAULT_VOLUME))
  const muted = ref(localStorage.getItem(LS.muted) === '1')
  /** 播放倍速：持久化；换歌加载新 src 时由 defaultPlaybackRate 延续 */
  const rate = ref(normalizeRate(Number(localStorage.getItem(LS.rate) ?? 1)))

  // ---------- 歌词 ----------
  const lyricsLines = ref<LrcLine[] | null>(null)
  const lyricsPlain = ref<string[] | null>(null)
  /** QRC 逐字歌词（Rust 侧解密+解析，毫秒时间轴）；非逐字歌词为 null */
  const lyricsWordLines = ref<QrcLine[] | null>(null)
  /** 歌词是加密/二进制内容，无法解析显示 */
  const lyricsUnsupported = ref(false)
  const lyricsLoading = ref(false)
  /** 歌词偏移（秒）：>0 歌词延后显示，<0 提前。按曲目持久化，用于校准 LRC 时间轴与音频不同步 */
  const lyricOffset = ref(0)
  /**
   * 双语歌词两条副行的显示开关：**默认都关闭**（键不存在即关，只有显式存 '1' 才算开），
   * 按曲目记忆，切歌时随曲目重读。界面只在当前歌词确实带该副行时显示对应按钮
   * （见 hasLyricTransliteration / hasLyricTranslation），点开后才在原文上下显示。
   * - lyricTransliteration：音译 / 罗马字（显示在原文上方）
   * - lyricTranslation：译文（显示在原文下方）
   */
  const lyricTransliteration = ref(false)
  const lyricTranslation = ref(false)
  /** 当前歌词是否带音译 / 译文：决定播放页歌词区那两颗开关图标显不显示（没有的整颗隐藏） */
  const hasLyricTransliteration = computed(() => !!lyricsLines.value?.some((l) => !!l.transliteration))
  const hasLyricTranslation = computed(() => !!lyricsLines.value?.some((l) => !!l.translation))
  const activeLyricIndex = computed(() =>
    lyricsLines.value ? activeLineIndex(lyricsLines.value, position.value - lyricOffset.value) : -1,
  )

  /** 歌词请求序号：连续切歌时旧请求的慢回包直接丢弃，防止 A 的歌词写到 B 身上 */
  let lyricsSeq = 0
  async function loadLyrics(t: Track) {
    const my = ++lyricsSeq
    const trackId = t.id
    lyricsLines.value = null
    lyricsPlain.value = null
    lyricsWordLines.value = null
    lyricsUnsupported.value = false
    lyricOffset.value = readLrcOffset(trackId)
    lyricTransliteration.value = readLyricTransliteration(trackId)
    lyricTranslation.value = readLyricTrans(trackId)
    lyricsLoading.value = true
    try {
      // 不以 hasLyrics 标志为前置条件：旧库的标志可能过期（快速导入/旧版本扫描）
      const raw = await api.getLyrics(trackId)
      // 切歌后旧回包直接丢弃（歌词与偏移都是按曲目记的，不能落到新歌上）
      if (my !== lyricsSeq || current.value?.id !== trackId) return
      if (!raw) return
      // QRC 逐字歌词优先（Rust 侧解密+解析，兼容新旧加密/明文/XML/行式）：
      // 行级时间轴复用现有滚动/跳转链路（偏移由 activeLyricIndex 统一处理），单词级另存供逐字高亮
      const qrc = await api.parseQrc(raw).catch(() => null)
      // 切歌后旧回包直接丢弃
      if (my !== lyricsSeq || current.value?.id !== trackId) return
      if (qrc && qrc.length) {
        // 双语/音译歌词：同起点的副行并进上一行（与 parseWordLrc 同口径，两条链路行数口径一致）
        const folded = foldQrcSubLines(qrc)
        lyricsWordLines.value = folded
        lyricsLines.value = qrcToLrcLines(folded)
        return
      }
      // 字级歌词（A2 `<mm:ss.xx>` 写法 与 多标签逐字 `[00:00.000]身[00:00.582]骑` 两种结构）：
      // 产出与 QRC 同构的逐字数据，复用同一套渲染链路；
      // 须放在 parseLrc 之前，否则字级时间标签会被当普通文本显示
      const wordLrc = parseWordLrc(raw)
      if (my !== lyricsSeq || current.value?.id !== trackId) return
      if (wordLrc && wordLrc.length) {
        lyricsWordLines.value = wordLrc
        lyricsLines.value = qrcToLrcLines(wordLrc)
        return
      }
      const { lines, synced } = parseLrc(raw)
      if (synced) {
        lyricsLines.value = lines
        return
      }
      // 加密/二进制内容（如未知方案的加密 QRC）：明确提示，不展示乱码
      if (looksLikeHexQrc(raw) || looksBinaryish(raw)) {
        lyricsUnsupported.value = true
        return
      }
      lyricsPlain.value = plainLines(raw)
    } catch {
      /* 歌词获取失败静默忽略 */
    } finally {
      // 只有最新请求能关 loading：旧请求的 finally 不能灭掉新歌的加载态
      if (my === lyricsSeq) lyricsLoading.value = false
    }
  }

  /** 当前曲目歌词按最新设置重新加载（歌词来源优先级等设置变更后调用） */
  function reloadLyrics() {
    const t = current.value
    if (t) void loadLyrics(t)
  }

  /** 读取某曲目的持久化歌词偏移 */
  function readLrcOffset(trackId: number): number {
    const v = Number(localStorage.getItem(`lm.lrcOffset.${trackId}`) ?? 0)
    return Number.isFinite(v) ? v : 0
  }

  /** 调整歌词偏移（delta 秒，UI 步进 ±0.5s），clamp 到 ±10s 并按曲目持久化；每次生效后 toast 反馈累计量 */
  function setLyricOffset(delta: number) {
    const t = current.value
    if (!t) return
    const prev = lyricOffset.value
    const v = Math.round(Math.min(10, Math.max(-10, prev + delta)) * 10) / 10
    if (v === prev) return // 无变化（如已还原后再点还原）不反馈
    lyricOffset.value = v
    try {
      localStorage.setItem(`lm.lrcOffset.${t.id}`, String(v))
    } catch {
      /* ignore */
    }
    // 固定 key：连续校准时提示原地更新，不叠加多个提示框
    toast(
      v === 0
        ? tr('toast.lyricOffsetReset')
        : tr(v > 0 ? 'toast.lyricOffsetDelay' : 'toast.lyricOffsetAdvance', {
            value: Math.abs(v).toFixed(1),
          }),
      'info',
      'lyric-offset',
    )
  }

  /** 读取某曲目的音译（罗马字）开关（**默认关闭**：键不存在即关，只有显式存 '1' 才算开） */
  function readLyricTransliteration(trackId: number): boolean {
    return localStorage.getItem(`lm.lrcTranslit.${trackId}`) === '1'
  }

  /** 读取某曲目的译文开关（同上口径，默认关闭） */
  function readLyricTrans(trackId: number): boolean {
    return localStorage.getItem(`lm.lrcTrans.${trackId}`) === '1'
  }

  /** 写回某曲目的副行开关（音译 / 译文）；写失败不抛错，本次会话内的显隐照样生效 */
  function writeLyricSubFlag(key: string, trackId: number, on: boolean) {
    try {
      localStorage.setItem(`${key}.${trackId}`, on ? '1' : '0')
    } catch {
      /* ignore */
    }
  }

  /** 切换音译（罗马字）显示：按曲目持久化，纯前端显隐，不需要重载歌词 */
  function toggleLyricTransliteration() {
    const t = current.value
    if (!t) return
    lyricTransliteration.value = !lyricTransliteration.value
    writeLyricSubFlag('lm.lrcTranslit', t.id, lyricTransliteration.value)
  }

  /** 切换译文显示：同上口径 */
  function toggleLyricTranslation() {
    const t = current.value
    if (!t) return
    lyricTranslation.value = !lyricTranslation.value
    writeLyricSubFlag('lm.lrcTrans', t.id, lyricTranslation.value)
  }

  const current = computed<Track | null>(() => queue.value[index.value] ?? null)

  // ---------- audio 事件收口 ----------
  audio.volume = muted.value ? 0 : volume.value
  audio.muted = muted.value
  // 倍速：换源加载新 src 时 playbackRate 会重置，defaultPlaybackRate 保证新歌延续倍速
  audio.playbackRate = rate.value
  audio.defaultPlaybackRate = rate.value

  // 连续播放失败计数：连续失败达到上限就停止跳歌（见 error 监听器），成功播放即归零。
  // 上限固定而非「整轮队列长度」：远端限流（如 OpenList 的 WebDAV 登录锁定）期间，
  // 顺着队列一路跳会持续发请求，反而把封锁窗口不断续期，越跳越恢复不了。
  const MAX_AUTO_SKIP = 5
  let errorStreak = 0

  // ---------- 听歌统计：实际收听心跳 ----------
  // 只计「真实在听」的秒数：暂停/缓冲不派发 timeupdate 天然不计，快进跳过的段落不在
  // 推进时刻内也不计。累计满 30s 落一条流水（play_history），暂停/切歌时把不足 30s 的
  // 尾巴结算掉——一次连续收听可能对应多条流水，统计口径是 SUM(seconds)，行数无意义。
  const LISTEN_FLUSH_THRESHOLD = 30
  let listenTrackId = 0 // 当前累计所属曲目 id（0 = 无）
  let listenSeconds = 0 // 未结算的累计收听秒数（浮点累加，结算时取整）
  let lastListenTick = 0 // 上次心跳时刻（performance.now()）
  let playStartTick = 0 // 播放启动延迟统计起点（performance.now()；0 = 无待结算）
  /** 结算并上报未落库的收听片段；force=false 时不足阈值就不写（防碎片行） */
  function flushListen(force = false) {
    if (!listenTrackId) return
    const secs = Math.round(listenSeconds)
    if (secs < (force ? 1 : LISTEN_FLUSH_THRESHOLD)) return
    api.reportListen(listenTrackId, secs, mode.value).catch(() => {})
    listenSeconds = 0
  }

  // ---------- 淡入淡出 ----------
  // 开关存 localStorage lm.fade（**默认开启**：只有显式存 '0' 才算关闭，键不存在时按开启）。
  // 淡入：每曲开头 800ms 从 0 升至目标音量；淡出：暂停/切歌前 600ms 平滑降至 0，避免突兀截断。
  const FADE_IN_MS = 800
  const FADE_OUT_MS = 600
  function fadeEnabled(): boolean {
    return localStorage.getItem('lm.fade') !== '0'
  }
  let fadeRaf = 0
  let fadeSeq = 0
  /** 淡入淡出期间禁止 watch(volume) 直接写 audio.volume（避免与逐帧渐变打架） */
  let suppressVolSync = false
  function cancelFade() {
    if (fadeRaf) clearInterval(fadeRaf)
    fadeRaf = 0
    fadeSeq++
    suppressVolSync = false
  }
  /** 平滑淡入：从当前音量升到用户设定音量 */
  function fadeIn() {
    cancelFade()
    // 后台窗口（document.hidden）：timer 被强节流（挂机数分钟后每分钟才 tick 一次），
    // 800ms 的淡入会拖到几十秒，期间音量贴 0 ≈ 无声——用户会以为「播放没反应」。
    // 后台听不见过渡，直接到位（前台行为不变）。
    if (!fadeEnabled() || document.hidden) {
      audio.volume = volume.value
      return
    }
    const seq = ++fadeSeq
    const from = audio.volume
    const to = volume.value
    const start = performance.now()
    suppressVolSync = true
    // 用定时器而非 rAF 驱动：窗口被遮挡/最小化时 rAF 停摆，音量会永远卡在起点（无声播放）
    const step = () => {
      if (fadeSeq !== seq) return
      const t = Math.min(1, (performance.now() - start) / FADE_IN_MS)
      audio.volume = from + (to - from) * t
      if (t < 1) return
      clearInterval(fadeRaf)
      fadeRaf = 0
      suppressVolSync = false
      audio.volume = volume.value // 结束校正：若过程中用户调了音量则落在最新值
    }
    step()
    fadeRaf = setInterval(step, 16)
  }
  /** 平滑淡出：降到 0 后执行回调（暂停/换源） */
  function fadeOut(cb?: () => void) {
    // 后台窗口直达（原因同 fadeIn）：睡眠定时器挂机触发时尤其重要，
    // 别让 600ms 的动画在强节流下把暂停拖到几十秒之后
    if (!fadeEnabled() || audio.paused || document.hidden) {
      audio.volume = 0
      cb?.()
      return
    }
    cancelFade()
    const seq = ++fadeSeq
    const from = audio.volume
    const start = performance.now()
    suppressVolSync = true
    const step = () => {
      if (fadeSeq !== seq) return
      const t = Math.min(1, (performance.now() - start) / FADE_OUT_MS)
      audio.volume = from * (1 - t)
      if (t < 1) return
      clearInterval(fadeRaf)
      fadeRaf = 0
      suppressVolSync = false
      audio.volume = 0
      cb?.()
    }
    step()
    fadeRaf = setInterval(step, 16)
  }
  /** 设置淡入淡出开关（设置页调用）；关闭时立即恢复当前音量 */
  function setFadeEnabled(v: boolean) {
    localStorage.setItem('lm.fade', v ? '1' : '0')
    if (!v) {
      cancelFade()
      audio.volume = volume.value
    }
  }

  audio.addEventListener('timeupdate', () => {
    position.value = audio.currentTime
    if (audio.currentTime > 0) localStorage.setItem(LS.lastPos, String(audio.currentTime))
    // 收听心跳：timeupdate 仅在播放推进时派发（暂停/缓冲不触发，天然不计）；
    // delta 用墙钟差并钳制 5s，防后台节流/睡眠唤醒后一次补算出巨大时长
    if (listenTrackId && current.value?.id === listenTrackId) {
      const delta = Math.min((performance.now() - lastListenTick) / 1000, 5)
      if (delta > 0) listenSeconds += delta
      flushListen()
    }
    lastListenTick = performance.now()
  })
  audio.addEventListener('durationchange', () => {
    if (Number.isFinite(audio.duration)) duration.value = audio.duration
  })
  audio.addEventListener('playing', () => {
    playing.value = true
    buffering.value = false
    errorStreak = 0
    // 播放启动延迟：从 requestPlay 到真正出声（每次加载只报一次，恢复播放不报）
    if (playStartTick > 0) {
      const ms = Math.round(performance.now() - playStartTick)
      playStartTick = 0
      if (ms > 0 && ms < 60_000) api.reportPlayLatency(ms).catch(() => {})
    }
    lastListenTick = performance.now() // 恢复播放：心跳基准重置（暂停期间不计秒）
  })
  audio.addEventListener('pause', () => {
    playing.value = false
    flushListen(true) // 暂停即结算本片段（恢复后另起一条流水）
  })
  audio.addEventListener('waiting', () => {
    buffering.value = true
  })
  audio.addEventListener('canplay', () => {
    buffering.value = false
  })
  audio.addEventListener('ended', () => {
    // 手动切歌的淡出正在进行（fadeOut 的 600ms 窗口内旧曲自然播完）：
    // 以用户的手动选择为准，自动推进作废，否则会取消掉用户刚点的歌
    if (fadeRaf !== 0) return
    // 睡眠定时器「播完当前曲」：到此为止，不自动切下一首
    if (consumeSleepStop()) {
      audio.pause()
      return
    }
    if (mode.value === 'one') {
      audio.currentTime = 0
      void audio.play().catch(() => {})
    } else {
      next()
    }
  })
  audio.addEventListener('error', () => {
    if (!current.value) return
    resetPlaybackState()
    toast(tr('toast.playFailed', { title: current.value.title }), 'error')
    // 连续失败保护：连续失败达上限就停下，不再顺着队列一路请求
    // （远端限流期间那样做会把封锁窗口不断续期，越跳越恢复不了）
    errorStreak++
    if (errorStreak < MAX_AUTO_SKIP) {
      setTimeout(() => next(true), 400)
    } else {
      toast(tr('toast.playFailedTooMany', { count: errorStreak }), 'error')
      errorStreak = 0
    }
  })

  // 窗口关闭/刷新：尽力结算未落库的收听尾巴（丢失上限 = 一个心跳周期 30s）
  window.addEventListener('pagehide', () => flushListen(true))

  // ---------- 控制 ----------
  /**
   * 发起播放:在调用方（用户手势）内立即调用 play()，数据未就绪时浏览器会挂起直到可播。
   * 不能等 loadedmetadata 再播——该事件每次加载只触发一次，若已被消费（如启动恢复进度的
   * seek 场景，readyState 因 seek 未完成停在 HAVE_METADATA），监听永远不会触发，播放死等。
   * play() 拒绝时若非换源打断（AbortError），等 canplay 后重试一次。
   */
  function requestPlay() {
    buffering.value = true
    playStartTick = performance.now() // 播放启动延迟统计起点
    const attempt = () => {
      audio.play()?.catch((e: unknown) => {
        if ((e as DOMException | undefined)?.name === 'AbortError') return
        if (audio.readyState >= 2) {
          void audio.play().catch(() => (buffering.value = false))
        } else {
          audio.addEventListener(
            'canplay',
            () => {
              void audio.play().catch(() => (buffering.value = false))
            },
            { once: true },
          )
        }
      })
    }
    attempt()
  }

  function load(t: Track, autoplay = true) {
    const doLoad = () => {
      // 换曲：结算上一首未落库的收听尾巴，并切换心跳归属
      flushListen(true)
      listenTrackId = t.id
      listenSeconds = 0
      lastListenTick = performance.now()
      audio.src = trackStreamUrl(t.id)
      position.value = 0
      duration.value = t.duration ?? 0
      // 切换新歌立即进入加载态，直到 canplay/playing 事件清除
      buffering.value = true
      localStorage.setItem(LS.lastPos, '0')
      errorStreak = 0
      void loadLyrics(t)
      if (autoplay) {
        audio.volume = 0 // 淡入起点
        requestPlay()
        api.reportPlay(t.id).catch(() => {})
        fadeIn()
      }
    }
    if (audio.paused) {
      cancelFade()
      doLoad()
    } else {
      // 正在播放：先淡出旧曲再换源（自然衔接）
      fadeOut(doLoad)
    }
  }

  /** 判断当前队列与传入列表是否为同一份（同长度、同顺序） */
  function sameQueueAs(list: Track[]): boolean {
    return queue.value.length === list.length && queue.value.every((t, i) => t.id === list[i].id)
  }

  function playList(list: Track[], startIndex = 0) {
    if (!list.length) return
    const idx = Math.max(0, Math.min(startIndex, list.length - 1))
    // 同一播放列表内点击正在播放的歌曲：暂停/继续，而不是重新开始
    if (sameQueueAs(list) && list[idx].id === current.value?.id) {
      toggle()
      return
    }
    queue.value = [...list]
    errorStreak = 0
    playAt(idx)
  }

  function playAt(i: number) {
    if (i < 0 || i >= queue.value.length) return
    index.value = i
    load(queue.value[i])
    snapshotQueue()
  }

  function toggle() {
    if (!current.value) return
    if (audio.paused) {
      cancelFade()
      audio.volume = 0 // 淡入起点
      requestPlay()
      fadeIn()
    } else {
      // 暂停前先淡出，避免音量突变
      fadeOut(() => audio.pause())
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
        fadeOut(() => {
          audio.pause()
          position.value = 0
        })
      }
    } else {
      playAt(i)
    }
  }

  /**
   * 上一曲：**永远切到队列里的上一首**。
   * 刻意不做「播放超过 3 秒就先回到本曲开头」那套（iTunes / Spotify 的老习惯）：
   * 那会让按钮时灵时不灵——同一颗按钮点下去去哪，取决于当前播到第几秒，用户没法预期。
   * 想回到本曲开头用进度条拖回去即可。
   * 队列第一首时：循环模式回到末尾，否则原地回到开头（没有上一首可切）。
   */
  function prev() {
    if (!queue.value.length) return
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
    // 元数据未就绪时 duration 为 0：直接透传 currentTime，不能被钳死在 0
    const target = duration.value > 0 ? Math.min(Math.max(0, t), duration.value) : Math.max(0, t)
    try {
      audio.currentTime = target
    } catch {
      /* 元数据未就绪时浏览器可能拒绝 seek，忽略 */
    }
    position.value = target
  }

  function setVolume(v: number) {
    volume.value = Math.min(1, Math.max(0, v))
    if (volume.value > 0 && muted.value) muted.value = false
  }

  function toggleMute() {
    muted.value = !muted.value
  }

  /** 播放失败兜底：清掉坏歌残留的时长/进度（queue/index 已指向坏歌，不能留上一首的值误导 UI） */
  function resetPlaybackState() {
    position.value = 0
    duration.value = 0
    buffering.value = false
    playing.value = false
  }

  /** 设置播放倍速（不在可选列表内的值回退到 1x） */
  function setRate(v: number) {
    rate.value = normalizeRate(v)
  }

  // ---------- 队列操作 ----------
  /** 下一首播放：队列里已有这首时不再新增条目（挪到下一首 / 本来就是下一首则只提示） */
  function playNextInQueue(t: Track) {
    if (index.value === -1) {
      playList([t], 0)
      return
    }
    const r = insertNextAfter(queue.value, index.value, t)
    if (r.action === 'noop') return
    if (r.action === 'alreadyNext') {
      toast(tr('toast.alreadyPlayNext', { title: t.title }))
      return
    }
    queue.value = r.queue
    index.value = r.index
    snapshotQueue()
    toast(
      r.action === 'moved'
        ? tr('toast.movedToPlayNext', { title: t.title })
        : tr('toast.playNextAfter', { title: current.value?.title ?? '' }),
    )
  }

  function enqueue(t: Track) {
    if (index.value === -1) {
      playList([t], 0)
      return
    }
    queue.value.push(t)
    snapshotQueue()
    toast(tr('toast.addedToQueue'))
  }

  function removeFromQueue(i: number) {
    if (i < 0 || i >= queue.value.length) return
    const removingCurrent = i === index.value
    queue.value.splice(i, 1)
    if (i < index.value) {
      index.value--
    } else if (removingCurrent) {
      // 移除的是当前曲目：指针指向下一首，播放状态延续——
      // 之前在放就接着播下一首（走完整 load：换源 + 淡入 + 播放记录），
      // 之前暂停则保持暂停（预载 src，播放键可直接续播）。
      if (index.value >= queue.value.length) index.value = queue.value.length - 1
      const wasPlaying = !audio.paused
      cancelFade()
      audio.pause()
      resetPlaybackState()
      lyricsLines.value = null
      lyricsPlain.value = null
      lyricsWordLines.value = null
      lyricsUnsupported.value = false
      const next = current.value
      if (next) {
        if (wasPlaying) {
          load(next)
        } else {
          audio.src = trackStreamUrl(next.id)
          duration.value = next.duration ?? 0
          void loadLyrics(next)
        }
      } else {
        audio.removeAttribute('src')
      }
    }
    snapshotQueue()
  }

  function clearQueue() {
    queue.value = []
    index.value = -1
    audio.pause()
    audio.removeAttribute('src')
    playing.value = false
    position.value = 0
    duration.value = 0
    snapshotQueue()
  }

  // ---------- 持久化 ----------
  watch(volume, (v) => {
    if (!suppressVolSync) audio.volume = v
    localStorage.setItem(LS.volume, String(v))
  })
  watch(muted, (m) => {
    audio.muted = m
    localStorage.setItem(LS.muted, m ? '1' : '0')
  })
  watch(mode, (m) => localStorage.setItem(LS.mode, m))
  watch(rate, (r) => {
    audio.playbackRate = r
    audio.defaultPlaybackRate = r
    localStorage.setItem(LS.rate, String(r))
  })
  watch(current, (t) => {
    if (t) localStorage.setItem(LS.lastTrack, String(t.id))
  })
  // 队列快照：在各变更点显式保存（避免 watch 里对大队列反复 map/join 的开销）
  function snapshotQueue() {
    saveQueueSnapshot(queue.value, index.value)
  }

  /**
   * 音量归一化（ReplayGain）：把当前曲目的增益写入共享音频图的归一化节点。
   * 无增益数据时写 0（增益=1，不改变音量）；开启归一化后切歌/切换开关即时生效。
   */
  function applyCurrentNormalize() {
    const db = current.value?.rgTrackGain ?? 0
    setNormalizeGain(db)
  }
  watch([current, normEnabled], () => applyCurrentNormalize())

  // 系统托盘控制（M2）
  void listen<string>('tray', (e) => {
    if (e.payload === 'toggle') toggle()
    else if (e.payload === 'next') next()
    else if (e.payload === 'prev') prev()
    else if (e.payload === 'fav') toggleFav()
  })

  // Windows 任务栏缩略图工具栏：播放状态变化时同步中间按钮的播放/暂停图标
  watch(playing, (p) => {
    api.setThumbbarPlaying(p).catch(() => {})
    // 播放时阻止系统休眠/锁屏（开关见设置，默认启用）
    applyPowerGuard(p)
  })

  // 窗口标题跟随当前歌曲：任务栏悬停预览 / Alt+Tab 顶部显示歌名（类似 QQ 音乐）
  const appWindow = getCurrentWindow()
  watch(current, (t) => {
    const title = t ? `${t.title} - ${t.artist ?? tr('artist.unknownArtist')}` : 'LanMusic'
    void appWindow.setTitle(title).catch(() => {})
    // 任务栏悬停预览整块显示当前歌曲的专辑封面（无曲目/无专辑时传 null 关闭封面预览）
    api.setThumbbarAlbum(t?.albumId ?? null).catch(() => {})
  })

  /** 切换当前歌曲的喜欢状态 */
  function toggleFav() {
    const t = current.value
    if (!t) return
    api
      .favoriteToggle(t.id, !t.fav)
      .then(() => {
        t.fav = !t.fav
        // 刷新侧边栏「我的喜欢」计数
        void useLibraryStore().loadStats()
      })
      .catch((e) => toast(errorText(e), 'error'))
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
      // 先把进度同步到 UI：媒体是分块流式加载，seek 校准可能要等数秒，
      // 期间用户不应看到进度归零
      position.value = pos
      const restoreSrc = audio.src
      const apply = () => {
        // 触发即摘除（非 once 监听）：若此刻 src 已被切歌替换，说明用户在恢复曲目
        // 元数据加载完成前就点了新歌——进度绝不能应用到新歌上（否则新歌从旧进度开播）
        audio.removeEventListener('loadedmetadata', apply)
        if (audio.src !== restoreSrc) return
        if (pos < (audio.duration || Infinity)) audio.currentTime = pos
      }
      audio.addEventListener('loadedmetadata', apply)
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
    rate,
    setRate,
    current,
    lyricsLines,
    lyricsPlain,
    lyricsWordLines,
    lyricsUnsupported,
    lyricsLoading,
    lyricOffset,
    setLyricOffset,
    lyricTransliteration,
    hasLyricTransliteration,
    toggleLyricTransliteration,
    lyricTranslation,
    hasLyricTranslation,
    toggleLyricTranslation,
    reloadLyrics,
    activeLyricIndex,
    playList,
    playAt,
    toggle,
    next,
    prev,
    seek,
    setVolume,
    toggleMute,
    setFadeEnabled,
    // 与 fadeEnabled() 同源（存 '0' 才算关，默认开启）：否则全新安装时设置页开关显示
    // 「已关闭」而实际行为是开启，两者对不上
    isFadeOn: fadeEnabled,
    playNextInQueue,
    enqueue,
    removeFromQueue,
    clearQueue,
    toggleFav,
    restore,
  }
})
