import { invoke } from '@tauri-apps/api/core'
import type {
  AlbumItem,
  ArtistItem,
  LibraryStats,
  Page,
  Playlist,
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

  // 库查询
  queryTracks: (q: TrackQuery) => invoke<Page<Track>>('query_tracks', { q }),
  queryAlbums: (search?: string, page = 0, pageSize = 120) =>
    invoke<Page<AlbumItem>>('query_albums', { search, page, pageSize }),
  queryArtists: (search?: string, page = 0, pageSize = 300) =>
    invoke<Page<ArtistItem>>('query_artists', { search, page, pageSize }),
  getTrack: (id: number) => invoke<Track | null>('get_track', { id }),
  getTracksByIds: (ids: number[]) => invoke<Track[]>('get_tracks_by_ids', { ids }),
  getStreamUrl: (id: number) => invoke<string>('get_stream_url', { id }),
  libraryStats: () => invoke<LibraryStats>('library_stats'),

  // 其他
  revealTrack: (id: number) => invoke<void>('reveal_track', { id }),

  // 歌单（M2）
  playlistList: () => invoke<Playlist[]>('playlist_list'),
  playlistCreate: (name: string) => invoke<Playlist>('playlist_create', { name }),
  playlistRename: (id: number, name: string) => invoke<void>('playlist_rename', { id, name }),
  playlistDelete: (id: number) => invoke<void>('playlist_delete', { id }),
  playlistGetItems: (id: number) => invoke<Track[]>('playlist_get_items', { id }),
  playlistAddTracks: (id: number, trackIds: number[]) =>
    invoke<void>('playlist_add_tracks', { id, trackIds }),
  playlistRemoveTrack: (id: number, trackId: number) =>
    invoke<void>('playlist_remove_track', { id, trackId }),
  playlistReorder: (id: number, trackIds: number[]) =>
    invoke<void>('playlist_reorder', { id, trackIds }),

  // 播放统计与歌词（M2）
  reportPlay: (id: number) => invoke<void>('report_play', { id }),
  getLyrics: (id: number) => invoke<string | null>('get_lyrics', { id }),
  favoriteToggle: (id: number, fav: boolean) => invoke<void>('favorite_toggle', { id, fav }),

  // 设置（M2/M3）
  getSetting: (key: string) => invoke<string | null>('get_setting', { key }),
  setSetting: (key: string, value: string) => invoke<void>('set_setting', { key, value }),

  // WebDAV（M3）
  webdavAddSource: (url: string, username: string, password: string, name?: string) =>
    invoke<Source>('webdav_add_source', { url, username, password, name }),
}
