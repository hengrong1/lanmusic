import { invoke } from '@tauri-apps/api/core'
import type {
  AlbumItem,
  ArtistItem,
  GenreItem,
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
  queryGenres: (search?: string, page = 0, pageSize = 300) =>
    invoke<Page<GenreItem>>('query_genres', { search, page, pageSize }),
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
  playlistSetDescription: (id: number, description: string) =>
    invoke<void>('playlist_set_description', { id, description }),
  playlistDelete: (id: number) => invoke<void>('playlist_delete', { id }),
  playlistGetItems: (id: number) => invoke<Track[]>('playlist_get_items', { id }),
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

  // 设置（M2/M3）
  getSetting: (key: string) => invoke<string | null>('get_setting', { key }),
  setSetting: (key: string, value: string) => invoke<void>('set_setting', { key, value }),

  // WebDAV（M3）
  webdavAddSource: (url: string, username: string, password: string, name?: string) =>
    invoke<Source>('webdav_add_source', { url, username, password, name }),
}
