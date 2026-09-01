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
  fav: boolean
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
