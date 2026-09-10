export interface SearchSettings {
  /** 搜索范围：title/artist/album/lyrics/filename */
  fields: string[]
  /** 拼音搜索 */
  pinyin: boolean
  /** 搜索排序偏好：relevance 相关度 / added 时间添加 / plays 播放次数 */
  sort: 'relevance' | 'added' | 'plays'
  /** 输入防抖（ms） */
  debounceMs: number
}

const KEY = 'lm.searchSettings'

export const DEFAULT_SEARCH_SETTINGS: SearchSettings = {
  fields: ['title', 'artist', 'album'],
  pinyin: true,
  sort: 'relevance',
  debounceMs: 300,
}

export function getSearchSettings(): SearchSettings {
  try {
    const raw = localStorage.getItem(KEY)
    if (raw) return { ...DEFAULT_SEARCH_SETTINGS, ...(JSON.parse(raw) as Partial<SearchSettings>) }
  } catch {
    /* 损坏回退默认 */
  }
  return { ...DEFAULT_SEARCH_SETTINGS }
}

export function setSearchSettings(s: SearchSettings) {
  localStorage.setItem(KEY, JSON.stringify(s))
}