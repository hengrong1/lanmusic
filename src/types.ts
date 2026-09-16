export interface Source {
  id: number
  kind: 'local' | 'webdav'
  name: string
  basePath: string | null
  baseUrl: string | null
  enabled: boolean
  lastScanAt: number | null
  trackCount: number
  fastImport: boolean
  /** 是否扫描来源内子目录（false = 仅扫描根目录下的文件） */
  scanSubdirs: boolean
}

export interface Track {
  id: number
  title: string
  artist: string | null
  artistId: number | null
  album: string | null
  albumId: number | null
  trackNo: number | null
  discNo: number | null
  duration: number | null
  bitrate: number | null
  sampleRate: number | null
  bitDepth: number | null
  format: string | null
  path: string
  hasLyrics: boolean
  hasMv: boolean
  fav: boolean
  /** 完整艺人列表（合作曲目按标签顺序拆分为独立艺人） */
  artists?: TrackArtistRef[]
  /** 命中的搜索字段（title/artist/album/lyrics/filename），仅搜索结果非空 */
  matchedFields?: string[]
}

/** 曲目关联艺人 */
export interface TrackArtistRef {
  id: number
  name: string
}

/** 调整艺人分隔符后单首曲目的拆分变更 */
export interface ArtistSplitChange {
  trackId: number
  title: string
  oldArtists: string[]
  newArtists: string[]
}

/** 艺人名单规整变更：旧名 → 规整名及其影响的曲目数 */
export interface ArtistNormalizeChange {
  oldName: string
  newName: string
  trackCount: number
}

/** 艺人别名（合并记忆）：旧名/别名 → 主艺人 */
export interface ArtistAlias {
  alias: string
  artistId: number
  artistName: string
}

/** 已移除歌曲记录 */
export interface RemovedTrack {
  id: number
  title: string
  artist: string | null
  album: string | null
  path: string
  /** 'manual' = 手动从曲库移除；'scan' = 扫描发现文件消失 */
  reason: 'manual' | 'scan'
  removedAt: number
}

/** 还原结果：restored = 已接受（文件仍在，来源扫描会重新入库）；missing = 文件已不在 */
export interface RemovedRestoreResult {
  restored: number
  missing: number
}

export interface Page<T> {
  total: number
  items: T[]
}

export interface AlbumItem {
  id: number
  title: string
  artist: string | null
  year: number | null
  hasCover: boolean
  trackCount: number
}

export interface ArtistItem {
  id: number
  name: string
  trackCount: number
}

export interface LibraryStats {
  tracks: number
  albums: number
  artists: number
  favorites: number
}

/** 听歌统计：汇总（seconds 均为实际收听秒数，play_history 流水聚合） */
export interface ListenSummary {
  totalSeconds: number
  totalPlays: number
  todaySeconds: number
  weekSeconds: number
  /** 曲库累计播放次数（play_count 总和，含历史） */
  totalTrackPlays: number
  /** 有效播放：同一曲目同一天只计一次 */
  effectivePlays: number
  uniqueTracks: number
  uniqueArtists: number
  uniqueAlbums: number
  firstListened: number | null
  lastListened: number | null
  /** 有收听记录的自然日数（供计算日均/周均/月均） */
  listenDays: number
}

/** 听歌统计：榜单行（kind = track/artist/album/genre，id/name/albumId 语义随 kind 变化） */
export interface ListenTopItem {
  kind: 'track' | 'artist' | 'album' | 'genre'
  id: number
  name: string
  artist: string | null
  albumId: number | null
  seconds: number
  plays: number
}

/** 听歌统计：趋势点（day 内容随粒度：日=YYYY-MM-DD，周=YYYY-Www，月=YYYY-MM） */
export interface ListenDailyPoint {
  day: string
  seconds: number
}

/** 听歌统计：24 小时分布点（hour = "00".."23" 本地时区；缺失小时由前端补零） */
export interface ListenHourPoint {
  hour: string
  seconds: number
}

/** 听歌统计：星期×小时热力格（dow 0=周日…6=周六，本地时区；缺失格由前端补零） */
export interface ListenHeatCell {
  dow: number
  hour: number
  seconds: number
}

/** 听歌统计：连续听歌天数（current/longest，按本地时区自然日） */
export type ListenStreak = [current: number, longest: number]

// ---------- 音乐库体检 ----------

export interface NameCount {
  name: string
  count: number
}

export interface YearCount {
  year: number
  count: number
}

export interface MetaCoverage {
  title: number
  artist: number
  album: number
  year: number
  genre: number
  trackNo: number
  total: number
}

/** 音乐库体检：曲库构成、分布、覆盖率（一次性聚合） */
export interface LibraryHealth {
  tracks: number
  albums: number
  artists: number
  genres: number
  totalSeconds: number
  totalSize: number
  sizeKnown: number
  formats: NameCount[]
  sources: NameCount[]
  years: YearCount[]
  sampleRates: NameCount[]
  bitDepths: NameCount[]
  lyricsEmbedded: number
  lyricsExternal: number
  lyricsQrc: number
  tracksWithLyrics: number
  albumsWithCover: number
  tracksWithCover: number
  mvCount: number
  meta: MetaCoverage
  metaIncomplete: number
}

export interface ScanHistoryItem {
  at: number
  sourceName: string
  added: number
  updated: number
  removed: number
  ms: number
}

// ---------- 性能与诊断（进程内内存态，重启清零） ----------

export interface DiagnosticsSnapshot {
  appVersion: string
  tauriVersion: string
  os: string
  arch: string
  webviewVersion: string | null
  uptimeSeconds: number
  setupMs: number
  memoryMb: number
  cpuPercent: number
  webdav: { requests: number; failures: number; totalMs: number }
  covers: { cacheHits: number; cacheMisses: number; extractOk: number; extractFail: number; totalMs: number }
  lyrics: { ok: number; fail: number; totalMs: number }
  playLatency: { count: number; avgMs: number; maxMs: number }
  panics: number
  frontendErrors: number
  lastScanEnumMs: number
  lastScanParseMs: number
}

export interface ScanProgress {
  sourceId: number
  /** "enumerate" = 正在枚举目录（total 未知）；"parse" = 解析入库中 */
  phase: 'enumerate' | 'parse'
  done: number
  total: number
  current: string
}

export interface ScanDone {
  sourceId: number
  added: number
  updated: number
  removed: number
  ms: number
}

export interface TrackQuery {
  view: 'all' | 'album' | 'artist' | 'favorites'
  refId?: number
  search?: string
  sort?: string
  page?: number
  pageSize?: number
  /** 搜索范围（title/artist/album/lyrics/filename） */
  fields?: string[]
  /** 是否启用拼音搜索 */
  pinyin?: boolean
}

export type ViewName = 'tracks' | 'albums' | 'artists' | 'playlist' | 'settings' | 'stats'

export interface NavRoute {
  view: ViewName
  albumId?: number
  albumTitle?: string
  artistId?: number
  artistName?: string
  playlistId?: number
  playlistName?: string
  recent?: boolean
  favorites?: boolean
  search?: string
}

export interface Playlist {
  id: number
  name: string
  trackCount: number
  createdAt: number | null
  /** 歌单封面：最新加入歌曲的专辑 id */
  coverAlbumId: number | null
  /** 歌单简介 */
  description: string | null
}

/** QRC 逐字歌词单词：毫秒时间轴（Rust 侧解析，见 src-tauri/src/qrc.rs） */
export interface QrcWord {
  word: string
  startTime: number
  endTime: number
}

/** QRC 逐字歌词行：words 为空表示间奏占位，text 为全部单词拼接 */
export interface QrcLine {
  startTime: number
  endTime: number
  text: string
  words: QrcWord[]
  /** 音译（罗马字）副行：显示在原文上方；前端折叠副行时写入，Rust 侧不产出该字段 */
  transliteration?: string
  /** 译文副行：显示在原文下方；前端折叠副行时写入，Rust 侧不产出该字段 */
  translation?: string
}

/** GitHub 最新 Release（应用内更新检查，Rust 侧已比较版本，见 src-tauri/src/updater.rs） */
export interface ReleaseInfo {
  version: string
  notes: string
  /** Release 说明（GitHub 渲染的 HTML 经 Rust 侧净化，富文本渲染用；null 回退纯文本 notes） */
  notesHtml: string | null
  htmlUrl: string
  /** 安装包下载地址（`*_x64-setup.exe` 资产）；为 null 时退化为「前往下载页」 */
  assetUrl: string | null
  assetName: string | null
  /** 安装包字节数（0 表示未知） */
  assetSize: number | null
  /** SHA-256 校验文件地址；为 null 时跳过校验（Rust 侧只记 warn） */
  sha256Url: string | null
}
