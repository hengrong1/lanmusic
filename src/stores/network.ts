import { ref } from 'vue'
import { defineStore } from 'pinia'
import { listen } from '@tauri-apps/api/event'
import type { LanDevice, ShareStatus } from '@/types'
import { api } from '@/api/commands'
import { toast } from '@/composables/useToast'

export const useNetworkStore = defineStore('network', () => {
  const devices = ref<Record<string, LanDevice>>({})
  const share = ref<ShareStatus | null>(null)
  const discovering = ref(false)

  async function loadShare() {
    share.value = await api.shareGetStatus()
  }

  async function toggleShare(on: boolean) {
    await api.shareSetEnabled(on)
    // 服务启停是异步的，稍等后再取状态；share:status 事件也会推送
    await new Promise((r) => setTimeout(r, 400))
    await loadShare()
  }

  async function startDiscover() {
    await api.netDiscoverStart()
    discovering.value = true
  }

  async function stopDiscover() {
    await api.netDiscoverStop()
    discovering.value = false
  }

  async function connectDevice(device: LanDevice, token: string) {
    const addr = `${device.host}:${device.port}`
    const src = await api.lanAddSource(addr, token, device.name)
    toast(`已连接「${src.name}」，正在扫描曲库`)
    devices.value = { ...devices.value }
    return src
  }

  async function addManualLan(addr: string, token: string, name?: string) {
    const src = await api.lanAddSource(addr, token, name)
    toast(`已连接「${src.name}」，正在扫描曲库`)
    return src
  }

  async function addWebDav(url: string, username: string, password: string, name?: string) {
    const src = await api.webdavAddSource(url, username, password, name)
    toast(`已添加「${src.name}」，正在扫描`)
    return src
  }

  async function init() {
    await listen<{ id: string; name: string; host: string; port: number }>('net:device_found', (e) => {
      devices.value = { ...devices.value, [e.payload.id]: e.payload }
    })
    await listen<{ id: string }>('net:device_lost', (e) => {
      const next = { ...devices.value }
      delete next[e.payload.id]
      devices.value = next
    })
    await listen<{ running: boolean; port?: number; error?: string }>('share:status', () => {
      void loadShare()
    })
    await loadShare()
  }

  return {
    devices,
    share,
    discovering,
    loadShare,
    toggleShare,
    startDiscover,
    stopDiscover,
    connectDevice,
    addManualLan,
    addWebDav,
    init,
  }
})
