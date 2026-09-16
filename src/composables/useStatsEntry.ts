import { ref } from 'vue'

/**
 * 「听歌统计」侧栏入口开关（模块级单例：设置页与侧栏共享同一响应式状态）。
 *
 * **默认关闭**（用户要求）——与项目「默认开启的开关存 '0' 才算关」的通例相反，
 * 属于显式开启类：键不存在即关，只有显式存 '1' 才开（口径同 lm.lrcTrans 系列）。
 * 开关只隐藏侧栏入口；统计流水（play_history）始终在记录，不受此开关影响。
 */
const KEY = 'lm.statsEnabled'
const enabled = ref(localStorage.getItem(KEY) === '1')

export function useStatsEntry() {
  function setStatsEnabled(v: boolean) {
    enabled.value = v
    localStorage.setItem(KEY, v ? '1' : '0')
  }
  return { statsEnabled: enabled, setStatsEnabled }
}
