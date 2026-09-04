import { computed, ref, watch } from 'vue'
import { emit, listen } from '@tauri-apps/api/event'
import { api } from '@/api/commands'
import { usePlayerStore } from '@/stores/player'
import { getAppFont } from '@/composables/useAppFont'
import { toast } from '@/composables/useToast'

/** 桌面歌词配置 */
export interface DeskLyricsConfig {
  /** 显示行数：1=仅当前行，2=当前行+下一行 */
  lines: 1 | 2
  /** 对齐方式：split=左右分离（仅双行：当前行左对齐、下一行右对齐） */
  align: 'left' | 'center' | 'right' | 'split'
  /** 播放行文字颜色 */
  color: string
  /** 未播放行文字颜色 */
  pendingColor: string
  /** 字号（px，两行相同） */
  fontSize: number
  /** 歌词背景色（配合 bgOpacity 使用） */
  bgColor: string
  /** 背景不透明度 0~0.85（0 = 无背景） */
  bgOpacity: number
  /** 是否显示文字描边 */
  outline: boolean
  /** 描边颜色 */
  outlineColor: string
  /** 是否加粗 */
  bold: boolean
}

/**
 * 空歌词占位符：三个中点（U+00B7）'···'。
 * 与播放页 / 播放条 / 歌词面板保持一致——桌面歌词原先显示「暂无歌词」，
 * 与其它界面不统一，这里统一用省略点。
 */
export const EMPTY_LYRIC = '\u00B7\u00B7\u00B7'

const LS_KEY = 'lm.deskLyrics'
const DEFAULT_CONFIG: DeskLyricsConfig = {
  lines: 2,
  align: 'center',
  color: '#ffffff',
  pendingColor: '#a1a1aa',
  fontSize: 34,
  bgColor: '#000000',
  bgOpacity: 0.35,
  outline: true,
  outlineColor: '#000000',
  bold: true,
}

function loadState(): { enabled: boolean; config: DeskLyricsConfig } {
  try {
    const raw = localStorage.getItem(LS_KEY)
    if (raw) {
      const s = JSON.parse(raw) as { enabled?: boolean; config?: Partial<DeskLyricsConfig> }
      return { enabled: !!s.enabled, config: { ...DEFAULT_CONFIG, ...s.config } }
    }
  } catch {
    /* 损坏则用默认值 */
  }
  return { enabled: false, config: { ...DEFAULT_CONFIG } }
}

// 模块级单例：主窗口内各处（播放条开关 / 设置页）共享同一状态
const saved = loadState()
const enabled = ref(saved.enabled)
const config = ref<DeskLyricsConfig>(saved.config)

/** 歌词窗口挂载完成后会广播 lyrics:ready，主窗口立即推送一次当前行与配置 */
function push(lines: string[], active: 0 | 1) {
  if (!enabled.value) return
  const player = usePlayerStore()
  // font：全局字体随事件同步给浮窗（与设置页修改即时联动）
  void emit('lyrics:sync', {
    lines,
    active,
    config: config.value,
    playing: player.playing,
    font: getAppFont(),
  }).catch(() => {})
}

/**
 * 桌面歌词（主窗口侧）：管理开关与配置，向歌词浮窗同步当前歌词行。
 * 在主窗口 App.vue 调用一次完成初始化；播放条 / 设置页调用读取共享状态。
 */
export function useDesktopLyrics() {
  const player = usePlayerStore()

  /**
   * 双行交替滚动：播放行在第一行/第二行之间交替（播完一句换到另一行），
   * 另一行显示下一句歌词。按行号奇偶决定播放行所在位置：
   * - 偶数行播放：[当前行(播放), 下一行]
   * - 奇数行播放：[下一行, 当前行(播放)]
   * 无同步歌词时降级为纯文本前两行（无交替）。
   */
  const deskLines = computed<{ lines: [string, string]; active: 0 | 1 }>(() => {
    if (player.lyricsLines && player.lyricsLines.length) {
      const i = Math.max(0, player.activeLyricIndex)
      const cur = player.lyricsLines[i]?.text ?? ''
      const next = player.lyricsLines[i + 1]?.text ?? ''
      return i % 2 === 0 ? { lines: [cur, next], active: 0 } : { lines: [next, cur], active: 1 }
    }
    if (player.lyricsPlain && player.lyricsPlain.length) {
      return { lines: [player.lyricsPlain[0] ?? '', player.lyricsPlain[1] ?? ''], active: 0 }
    }
    return { lines: [EMPTY_LYRIC, ''], active: 0 }
  })

  function persist() {
    try {
      localStorage.setItem(LS_KEY, JSON.stringify({ enabled: enabled.value, config: config.value }))
    } catch {
      /* ignore */
    }
  }

  /** 开关桌面歌词浮窗，返回最终状态（Rust 侧确认） */
  async function toggle(): Promise<boolean> {
    const next = !enabled.value
    try {
      const ok = await api.desktopLyricsSet(next)
      enabled.value = ok === true
      // 仅开启失败才提示（关闭成功时后端返回 false，属正常结果）
      if (next && !ok) toast('桌面歌词开启失败', 'error')
    } catch (e) {
      toast(`桌面歌词操作失败：${e}`, 'error')
      enabled.value = false
    }
    persist()
    return enabled.value
  }

  if (!started) {
    started = true
    // 歌词行/配置变化：持久化 + 推送到歌词浮窗
    watch(
      [deskLines, config],
      () => {
        persist()
        push(deskLines.value.lines, deskLines.value.active)
      },
      { deep: true },
    )
    // 播放状态变化：同步浮窗控制条的播放/暂停图标
    watch(
      () => player.playing,
      () => push(deskLines.value.lines, deskLines.value.active),
    )
    // 歌词浮窗就绪后立即补推一次（覆盖窗口刚创建/无新歌词行变化的场景）
    void listen('lyrics:ready', () => push(deskLines.value.lines, deskLines.value.active))
    // 全局字体变更（设置页）：立即同步给歌词浮窗
    void listen<string>('font:changed', () => push(deskLines.value.lines, deskLines.value.active))
    // 歌词浮窗控制条指令：转发给播放器（校准与播放页 [ ] / 还原语义一致）
    void listen<DeskControl>('lyrics:control', (e) => {
      switch (e.payload) {
        case 'prev':
          player.prev()
          break
        case 'toggle':
          player.toggle()
          break
        case 'next':
          player.next()
          break
        case 'close':
          api
            .desktopLyricsSet(false)
            .then(() => {
              enabled.value = false
              persist()
            })
            .catch(() => {})
          break
        case 'calib-back': // 歌词后退 0.5s（延后显示）
          player.setLyricOffset(0.5)
          break
        case 'calib-forward': // 歌词前进 0.5s（提前显示）
          player.setLyricOffset(-0.5)
          break
        case 'calib-reset': // 还原为默认时间轴
          player.setLyricOffset(-player.lyricOffset)
          break
      }
    })
    // 恢复上次开启状态
    if (enabled.value) {
      api
        .desktopLyricsSet(true)
        .then((ok) => {
          enabled.value = ok === true
          persist()
        })
        .catch(() => {
          enabled.value = false
          persist()
        })
    }
  }

  return { enabled, config, toggle }
}

let started = false

/** 歌词浮窗控制条 → 主窗口的指令 */
export type DeskControl = 'prev' | 'toggle' | 'next' | 'close' | 'calib-back' | 'calib-forward' | 'calib-reset'