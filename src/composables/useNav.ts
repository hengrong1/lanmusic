import { computed, ref, watch } from 'vue'
import type { NavRoute } from '@/types'

const LS_NAV = 'lm.nav'
const VALID_VIEWS = ['tracks', 'albums', 'artists', 'playlist', 'settings']

/** 启动时恢复上次停留的视图（筛选上下文） */
function loadRoute(): NavRoute {
  try {
    const raw = localStorage.getItem(LS_NAV)
    if (raw) {
      const r = JSON.parse(raw)
      if (r && VALID_VIEWS.includes(r.view)) return r
    }
  } catch {
    /* 忽略损坏的存档 */
  }
  return { view: 'tracks' }
}

const current = ref<NavRoute>(loadRoute())
const history = ref<NavRoute[]>([])

watch(
  current,
  (v) => {
    try {
      localStorage.setItem(LS_NAV, JSON.stringify(v))
    } catch {
      /* ignore */
    }
  },
  { deep: true },
)

export function useNav() {
  function go(route: NavRoute) {
    history.value.push({ ...current.value })
    if (history.value.length > 50) history.value.shift()
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
