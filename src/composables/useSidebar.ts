import { ref, watch } from 'vue'

const STORAGE_KEY = 'sidebar:collapsed'
const collapsed = ref(localStorage.getItem(STORAGE_KEY) === 'true')

watch(collapsed, (v) => localStorage.setItem(STORAGE_KEY, String(v)))

export function useSidebar() {
  return { collapsed }
}
