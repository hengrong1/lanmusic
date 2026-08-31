import { ref, watchEffect } from 'vue'

export type ThemeMode = 'light' | 'dark' | 'system'

const mode = ref<ThemeMode>((localStorage.getItem('lm.theme') as ThemeMode) || 'dark')
const systemDark = ref(window.matchMedia('(prefers-color-scheme: dark)').matches)

window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', (e) => {
  systemDark.value = e.matches
})

const resolved = ref<'light' | 'dark'>('dark')
watchEffect(() => {
  resolved.value = mode.value === 'system' ? (systemDark.value ? 'dark' : 'light') : mode.value
  document.documentElement.classList.toggle('dark', resolved.value === 'dark')
})

export function useTheme() {
  function setTheme(m: ThemeMode) {
    mode.value = m
    localStorage.setItem('lm.theme', m)
  }
  return { mode, resolved, setTheme }
}
