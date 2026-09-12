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

export type ViewName = 'tracks' | 'albums' | 'artists' | 'playlist' | 'settings'

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
