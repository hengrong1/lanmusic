import { invoke } from '@tauri-apps/api/core'
import { getSearchSettings } from '@/composables/useSearchSettings'
import type {
  AlbumItem,
  ArtistAlias,
  ArtistItem,
  ArtistUnmergeResult,
  ArtistNormalizeChange,
  ArtistSplitChange,
  LibraryHealth,
  LibraryStats,
  DiagnosticsSnapshot,
  ListenDailyPoint,
  ListenHeatCell,
  ListenHourPoint,
  ListenStreak,
  ListenSummary,
  ListenTopItem,
  FolderItem,
  NowPlayingMeta,
  Page,
  Playlist,
  QrcLine,
  ReleaseInfo,
  RemovedRestoreResult,
  RemovedTrack,
  ScanHistoryItem,
  Source,
  Track,
  TrackQuery,
} from '@/types'

export const api = {
  // 来源管理
  addLocalSource: (path: string) => invoke<Source>('add_local_source', { path }),
  listSources: () => invoke<Source[]>('list_sources'),
  removeSource: (id: number) => invoke<void>('remove_source', { id }),
  rescanSource: (id: number, mode: 'auto' | 'full' = 'auto') =>
    invoke<void>('rescan_source', { id, mode }),
  setSourceFastImport: (id: number, enabled: boolean) =>
    invoke<void>('set_source_fast_import', { id, enabled }),
  // 子目录扫描开关：关闭后仅扫描来源根目录下的文件（本地与 WebDAV 通用）
  setSourceScanSubdirs: (id: number, enabled: boolean) =>
    invoke<void>('set_source_scan_subdirs', { id, enabled }),

  // 库查询
  queryTracks: (q: TrackQuery) => {
    // 搜索时自动附加搜索设置（范围/拼音/排序偏好），调用方无需关心
    let merged = q
    if (q.search) {
      const s = getSearchSettings()
      merged = { ...q, fields: s.fields, pinyin: s.pinyin, sort: s.sort }
    }
    return invoke<Page<Track>>('query_tracks', { q: merged })
  },
  queryAlbums: (search?: string, page = 0, pageSize = 120) =>
    invoke<Page<AlbumItem>>('query_albums', { search, page, pageSize }),
  queryArtists: (search?: string, page = 0, pageSize = 300) =>
    invoke<Page<ArtistItem>>('query_artists', { search, page, pageSize }),
  getTrack: (id: number) => invoke<Track | null>('get_track', { id }),
  getTracksByIds: (ids: number[]) => invoke<Track[]>('get_tracks_by_ids', { ids }),
  getStreamUrl: (id: number) => invoke<string>('get_stream_url', { id }),
  libraryStats: () => invoke<LibraryStats>('library_stats'),
  // 文件夹视图：列出 parent 目录（null = 根）下的直接子目录；列出某目录下直接存放的曲目
  queryFolders: (parent: string | null) => invoke<FolderItem[]>('query_folders', { parent }),
  queryTracksByFolder: (folder: string | null) =>
    invoke<Track[]>('query_tracks_by_folder', { folder }),

  // 其他
  revealTrack: (id: number) => invoke<void>('reveal_track', { id }),
  /** 前端错误转发到后端日志文件（release 版无控制台，日志是唯一排查出口） */
  frontendLog: (level: 'error' | 'warn' | 'info', message: string) =>
    invoke<void>('frontend_log', { level, message }),
  /** 检查 GitHub 最新 Release（Rust 侧比较版本，见 src-tauri/src/updater.rs）；无更新返回 null */
  checkGithubUpdate: () => invoke<ReleaseInfo | null>('check_github_update'),
  /** 下载更新安装包（SHA-256 校验；进度见 update:download-progress 事件），返回落地路径 */
  downloadUpdateInstaller: (url: string, sha256Url: string | null, size: number | null) =>
    invoke<string>('download_update_installer', { url, sha256Url, size }),
  /** 听歌统计：上报一段实际收听（秒）与当时的播放模式。暂停/快进跳过的不计。 */
  reportListen: (trackId: number, seconds: number, mode: string) =>
    invoke<void>('report_listen', { trackId, seconds, mode }),
  listenStatsSummary: () => invoke<ListenSummary>('listen_stats_summary'),
  listenTopTracks: (kind: 'track' | 'artist' | 'album' | 'genre', range: 'week' | 'month' | 'all', limit?: number) =>
    invoke<ListenTopItem[]>('listen_top_tracks', { kind, range, limit: limit ?? null }),
  listenDaily: (days?: number, granularity?: 'day' | 'week' | 'month') =>
    invoke<ListenDailyPoint[]>('listen_daily', { days: days ?? null, granularity: granularity ?? null }),
  listenHourly: () => invoke<ListenHourPoint[]>('listen_hourly'),
  listenHeatmap: () => invoke<ListenHeatCell[]>('listen_heatmap'),
  listenStreak: () => invoke<ListenStreak>('listen_streak'),
  libraryHealth: () => invoke<LibraryHealth>('library_health'),
  scanHistoryList: () => invoke<ScanHistoryItem[]>('scan_history_list'),
  reportPlayLatency: (ms: number) => invoke<void>('report_play_latency', { ms }),
  diagnosticsSnapshot: () => invoke<DiagnosticsSnapshot>('diagnostics_snapshot'),
  /** 运行已下载的安装包（静默安装 + 装完自动重启），随后应用退出 */
  installUpdateAndRestart: (path: string) =>
    invoke<void>('install_update_and_restart', { path }),

  // 歌单（M2）
  playlistList: () => invoke<Playlist[]>('playlist_list'),
  playlistCreate: (name: string) => invoke<Playlist>('playlist_create', { name }),
  playlistRename: (id: number, name: string) => invoke<void>('playlist_rename', { id, name }),
  playlistSetDescription: (id: number, description: string) =>
    invoke<void>('playlist_set_description', { id, description }),
  playlistDelete: (id: number) => invoke<void>('playlist_delete', { id }),
  /** sort 为空 = 加入时间倒序；title/-title/album/-album/artist/-artist/duration/-duration 走后端拼音分组排序 */
  playlistGetItems: (id: number, sort?: string) =>
    invoke<Track[]>('playlist_get_items', { id, sort }),
  /** 返回实际新增数量（同歌单内已存在的曲目会跳过） */
  playlistAddTracks: (id: number, trackIds: number[]) =>
    invoke<number>('playlist_add_tracks', { id, trackIds }),
  playlistRemoveTrack: (id: number, trackId: number) =>
    invoke<void>('playlist_remove_track', { id, trackId }),
  playlistRemoveTracks: (id: number, trackIds: number[]) =>
    invoke<void>('playlist_remove_tracks', { id, trackIds }),
  playlistReorder: (id: number, trackIds: number[]) =>
    invoke<void>('playlist_reorder', { id, trackIds }),
  /** 歌单封面：最新加入歌曲的专辑 id */
  playlistCover: (id: number) => invoke<number | null>('playlist_cover', { id }),

  // 播放统计与歌词（M2）
  reportPlay: (id: number) => invoke<void>('report_play', { id }),
  getLyrics: (id: number) => invoke<string | null>('get_lyrics', { id }),
  /** QRC 逐字歌词解析（Rust 侧解密 + 词级毫秒时间轴，见 src-tauri/src/qrc.rs）；非 QRC 返回 null */
  parseQrc: (raw: string) => invoke<QrcLine[] | null>('parse_qrc', { raw }),
  favoriteToggle: (id: number, fav: boolean) => invoke<void>('favorite_toggle', { id, fav }),
  // Windows 任务栏缩略图按钮：同步播放/暂停图标（其他平台为空操作）
  setThumbbarPlaying: (playing: boolean) => invoke<void>('set_thumbbar_playing', { playing }),
  // Windows 任务栏悬停预览：整块显示当前歌曲所在专辑的封面（其他平台为空操作）
  setThumbbarAlbum: (albumId: number | null) => invoke<void>('set_thumbbar_album', { albumId }),
  // 桌面歌词浮窗开关，返回最终状态
  desktopLyricsSet: (enabled: boolean) => invoke<boolean>('desktop_lyrics_set', { enabled }),
  // 系统已安装字体列表（DirectWrite 枚举字族名；其他平台为空数组）
  listSystemFonts: () => invoke<string[]>('list_system_fonts'),
  // 退出应用（托盘菜单「退出」）
  exitApp: () => invoke<void>('exit_app'),
  // 播放时阻止系统休眠/锁屏（Windows SetThreadExecutionState；其他平台空操作）
  setPreventSleep: (prevent: boolean) => invoke<void>('set_prevent_sleep', { prevent }),
  // 获取 MV 视频流 URL（同名视频文件不存在时返回 null）
  getMvUrl: (trackId: number) => invoke<string | null>('get_mv_url', { trackId }),
  // 写入曲目响度分析结果（ReplayGain 增益 dB + 峰值）；分析在前端 Web Audio 完成
  saveLoudness: (id: number, gainDb: number, peak: number) =>
    invoke<void>('save_loudness', { id, gainDb, peak }),
  // 启用 / 禁用系统媒体键（注册为全局快捷键；非 Windows 为空操作）。
  // 返回值：注册失败的加速键名列表（如被其他播放器占用），通常为空数组。
  mediaControlsEnable: (enabled: boolean) => invoke<string[]>('media_controls_enable', { enabled }),
  // 系统级「正在播放」（SMTC / Now Playing / MPRIS，见 now_playing.rs）：
  // 推送当前曲目元数据（null = 清空）；播放状态与进度；启用/禁用注册
  nowPlayingSet: (meta: NowPlayingMeta | null) => invoke<void>('now_playing_set', { meta }),
  nowPlayingState: (playing: boolean, positionMs: number) =>
    invoke<void>('now_playing_state', { playing, positionMs }),
  nowPlayingEnable: (enabled: boolean) => invoke<void>('now_playing_enable', { enabled }),

  // 全局快捷键（Rust 侧注册系统热键；触发后经 global-shortcut 事件分发，见 useShortcuts.ts）
  /** 整体替换注册（先注销旧注册）；任一组合被其他程序占用则整体失败并返回错误 */
  globalShortcutsApply: (bindings: { action: string; shortcut: string }[]) =>
    invoke<void>('global_shortcut_apply', { bindings }),
  /** 注销本应用注册的全部全局快捷键 */
  globalShortcutsClear: () => invoke<void>('global_shortcut_clear'),
  /** 检测快捷键是否已被本应用注册（被其他程序占用只能在注册时报错） */
  globalShortcutIsRegistered: (shortcut: string) =>
    invoke<boolean>('global_shortcut_is_registered', { shortcut }),

  // 设置（M2/M3）
  getSetting: (key: string) => invoke<string | null>('get_setting', { key }),
  setSetting: (key: string, value: string) => invoke<void>('set_setting', { key, value }),
  /** 内置跳过目录名（扫描时始终跳过；设置页用于标出这些关键字） */
  getBuiltinSkipDirs: () => invoke<string[]>('get_builtin_skip_dirs'),
  setBackgroundImage: (path: string) => invoke<string>('set_background_image', { path }),

  // 艺人分隔符（设置后立即按新分隔符重拆曲库，返回受影响曲目的变更列表）
  getArtistSeparators: () => invoke<string>('get_artist_separators'),
  setArtistSeparators: (value: string) => invoke<ArtistSplitChange[]>('set_artist_separators', { value }),
  // 艺人名规整：剥离尾部括号注释（如「陈奕迅（Eason Chan）」→「陈奕迅」），合并同义艺人并迁移曲目/专辑关联
  normalizeArtistNames: () => invoke<ArtistNormalizeChange[]>('normalize_artist_names'),
  // 已合并名单（历次规整与自定义合并的别名记录）
  listArtistAliases: () => invoke<ArtistAlias[]>('list_artist_aliases'),
  // 自定义合并：把 source 艺人并入 target（视为同一人），source 名字记为 target 的别名
  mergeArtist: (sourceId: number, targetId: number) =>
    invoke<ArtistNormalizeChange>('merge_artist', { sourceId, targetId }),
  // 取消合并：把旧名恢复为独立艺人，按合并历史拆回曲目/专辑归属
  unmergeArtist: (alias: string) => invoke<ArtistUnmergeResult>('unmerge_artist', { alias }),

  // 曲库移除与记录
  /** 从曲库移除曲目（不删磁盘文件），返回实际移除数量 */
  removeTracks: (ids: number[]) => invoke<number>('remove_tracks', { ids }),
  listRemovedTracks: () => invoke<RemovedTrack[]>('list_removed_tracks'),
  clearRemovedTracks: () => invoke<void>('clear_removed_tracks'),
  /** 还原已移除歌曲（确认文件仍在后触发来源增量扫描重新入库） */
  restoreRemovedTracks: (ids: number[]) =>
    invoke<RemovedRestoreResult>('restore_removed_tracks', { ids }),

  // WebDAV（M3）
  webdavAddSource: (url: string, username: string, password: string, name?: string) =>
    invoke<Source>('webdav_add_source', { url, username, password, name }),
}
