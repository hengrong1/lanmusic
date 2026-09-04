import { computed, ref } from 'vue'
import type { NavRoute } from '@/types'

/** 每次启动都从默认视图（全部歌曲）开始，不记忆上次停留的界面 */
const current = ref<NavRoute>({ view: 'tracks' })
const history = ref<NavRoute[]>([])

export function useNav() {
  /** 路由是否实质相同：仅比较关键参数（view + 各 id + 标记），忽略显示用的 title/name */
  function sameRoute(a: NavRoute, b: NavRoute): boolean {
    return (
      a.view === b.view &&
      a.albumId === b.albumId &&
      a.artistId === b.artistId &&
      a.genre === b.genre &&
      a.playlistId === b.playlistId &&
      a.search === b.search &&
      a.recent === b.recent &&
      a.favorites === b.favorites
    )
  }

  function go(route: NavRoute) {
    // 目标与当前路由相同（如艺人页内再次点击同一艺人）：不重复压栈，
    // 否则返回键会连续弹出相同页面，看起来“返回不生效”
    if (!sameRoute(route, current.value)) {
      history.value.push({ ...current.value })
      if (history.value.length > 50) history.value.shift()
    }
    current.value = { ...route }
  }

  /** 用于搜索框输入：若已在搜索页则原地替换，避免堆叠历史 */
  function replaceSearch(q: string) {
    const r: NavRoute = { view: 'tracks', search: q || undefined }
    if (current.value.view === 'tracks' && current.value.search) {
      current.value = r
    } else if (q) {
      go(r)
    }
  }

  function back() {
    const prev = history.value.pop()
    if (prev) current.value = prev
  }

  return { current, go, replaceSearch, back, canBack: computed(() => history.value.length > 0) }
}
