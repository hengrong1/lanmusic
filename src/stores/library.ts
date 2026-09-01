import { computed, ref, watch } from 'vue'
import { defineStore } from 'pinia'
import { listen } from '@tauri-apps/api/event'
import type {
  LibraryStats,
  Page,
  Playlist,
  ScanDone,
  ScanProgress,
  Source,
  Track,
  TrackQuery,
} from '@/types'
import { api } from '@/api/commands'
import { toast } from '@/composables/useToast'

export const useLibraryStore = defineStore('library', () => {
  const sources = ref<Source[]>([])
  const stats = ref<LibraryStats>({ tracks: 0, albums: 0, artists: 0, favorites: 0 })
  const scanProgress = ref<Record<number, ScanProgress>>({})
  const playlists = ref<Playlist[]>([])

  // 排序条件持久化：刷新后保持上次的排序
  const savedSort = localStorage.getItem('lm.sort')
  const query = ref<Required<Pick<TrackQuery, 'view' | 'sort' | 'page' | 'pageSize'>> & {
    refId?: number
    search?: string
  }>({ view: 'all', sort: savedSort || 'title', page: 0, pageSize: 200 })
  watch(
    () => query.value.sort,
    (s) => {
      // 'recent' 是最近播放页的临时排序，不写入存档
      if (s !== 'recent') localStorage.setItem('lm.sort', s)
    },
  )

  // 当前曲目视图（全部/专辑/艺人/搜索共用）
  const trackPage = ref<Page<Track>>({ total: 0, items: [] })
  const loading = ref(false)

  const hasSource = computed(() => sources.value.length > 0)

  async function loadSources() {
    sources.value = await api.listSources()
  }
  async function loadStats() {
    stats.value = await api.libraryStats()
  }

  async function loadTracks(append = false) {
    loading.value = true
    try {
      const page = await api.queryTracks({ ...query.value })
      if (append) {
        trackPage.value = {
          total: page.total,
          items: [...trackPage.value.items, ...page.items],
        }
      } else {
        trackPage.value = page
      }
    } finally {
      loading.value = false
    }
  }

  function setQuery(patch: Partial<TrackQuery> & { view?: 'all' | 'album' | 'artist' | 'favorites' }) {
    query.value = {
      ...query.value,
      ...patch,
      page: patch.page ?? 0,
      search: patch.search ?? (patch.view ? undefined : query.value.search),
    }
    void loadTracks()
  }

  async function loadMore() {
    if (trackPage.value.items.length >= trackPage.value.total) return
    query.value.page++
    await loadTracks(true)
  }

  async function addFolder(path: string) {
    await api.addLocalSource(path)
    await Promise.all([loadSources(), loadStats()])
  }

  async function addWebDav(url: string, username: string, password: string, name?: string) {
    await api.webdavAddSource(url, username, password, name)
    await Promise.all([loadSources(), loadStats()])
  }

  async function removeSource(id: number) {
    await api.removeSource(id)
    await Promise.all([loadSources(), loadStats(), loadTracks()])
  }

  async function rescan(id: number, mode: 'auto' | 'full' = 'auto') {
    await api.rescanSource(id, mode)
  }

  async function setFastImport(id: number, enabled: boolean) {
    await api.setSourceFastImport(id, enabled)
    await loadSources()
  }

  // ---------- 歌单 ----------
  async function loadPlaylists() {
    playlists.value = await api.playlistList()
  }
  async function createPlaylist(name: string) {
    const p = await api.playlistCreate(name)
    await loadPlaylists()
    return p
  }
  async function renamePlaylist(id: number, name: string) {
    await api.playlistRename(id, name)
    await loadPlaylists()
  }
  async function setPlaylistDescription(id: number, description: string) {
    await api.playlistSetDescription(id, description)
    await loadPlaylists()
  }
  async function deletePlaylist(id: number) {
    await api.playlistDelete(id)
    await loadPlaylists()
  }
  async function addToPlaylist(playlistId: number, trackIds: number[]) {
    const added = await api.playlistAddTracks(playlistId, trackIds)
    await loadPlaylists()
    toast(added > 0 ? `已加入歌单（${added} 首）` : '所选歌曲已在歌单中')
    return added
  }
  async function removeFromPlaylist(playlistId: number, trackId: number) {
    await api.playlistRemoveTrack(playlistId, trackId)
    await loadPlaylists()
  }
  async function removeTracksFromPlaylist(playlistId: number, trackIds: number[]) {
    await api.playlistRemoveTracks(playlistId, trackIds)
    await loadPlaylists()
  }
  async function reorderPlaylist(playlistId: number, trackIds: number[]) {
    await api.playlistReorder(playlistId, trackIds)
    await loadPlaylists()
  }

  /** App 启动时调用：注册事件 + 首次加载 */
  async function init() {
    await listen<ScanProgress>('scan:progress', (e) => {
      scanProgress.value = { ...scanProgress.value, [e.payload.sourceId]: e.payload }
    })

    await listen<ScanDone>('scan:done', async (e) => {
      const { [e.payload.sourceId]: _removed, ...rest } = scanProgress.value
      scanProgress.value = rest
      await Promise.all([loadSources(), loadStats()])
      if (!query.value.search) void loadTracks()
      const parts = [`新增 ${e.payload.added}`, `更新 ${e.payload.updated}`]
      if (e.payload.removed) parts.push(`移除 ${e.payload.removed}`)
      toast(`扫描完成（${(e.payload.ms / 1000).toFixed(1)}s）：${parts.join('，')}`)
    })

    await listen<{ sourceId: number; message: string }>('scan:error', (e) => {
      const { [e.payload.sourceId]: _removed, ...rest } = scanProgress.value
      scanProgress.value = rest
      toast(`扫描失败：${e.payload.message}`, 'error')
    })

    await Promise.all([loadSources(), loadStats(), loadTracks(), loadPlaylists()])
  }

  return {
    sources,
    stats,
    scanProgress,
    playlists,
    trackPage,
    loading,
    query,
    hasSource,
    loadSources,
    loadStats,
    loadTracks,
    setQuery,
    loadMore,
    addFolder,
    addWebDav,
    removeSource,
    rescan,
    setFastImport,
    loadPlaylists,
    createPlaylist,
    renamePlaylist,
    setPlaylistDescription,
    deletePlaylist,
    addToPlaylist,
    removeFromPlaylist,
    removeTracksFromPlaylist,
    reorderPlaylist,
    init,
  }
})
