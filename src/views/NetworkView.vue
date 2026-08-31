<script setup lang="ts">
import { onMounted, ref } from 'vue'
import {
  Check,
  Copy,
  Globe,
  Link2,
  LoaderCircle,
  Plus,
  RadioTower,
  RefreshCw,
  Server,
  Wifi,
} from '@lucide/vue'
import { useNetworkStore } from '@/stores/network'
import { useStagger } from '@/composables/useStagger'
import { toast } from '@/composables/useToast'
import type { LanDevice } from '@/types'

const net = useNetworkStore()
const root = ref<HTMLElement | null>(null)
useStagger(root, ref(true))

onMounted(async () => {
  await net.init()
  await net.startDiscover()
})

// ---- 连接发现的设备：输入配对码 ----
const pairing = ref<{ device: LanDevice; token: string } | null>(null)
const connecting = ref(false)

async function confirmPairing() {
  if (!pairing.value || connecting.value) return
  connecting.value = true
  try {
    await net.connectDevice(pairing.value.device, pairing.value.token.trim())
    pairing.value = null
  } catch (e) {
    toast(String(e), 'error')
  } finally {
    connecting.value = false
  }
}

// ---- 手动添加 ----
const showManualLan = ref(false)
const manualLan = ref({ addr: '', token: '' })
const manualLanBusy = ref(false)
async function submitManualLan() {
  if (manualLanBusy.value) return
  manualLanBusy.value = true
  try {
    await net.addManualLan(manualLan.value.addr.trim(), manualLan.value.token.trim())
    showManualLan.value = false
    manualLan.value = { addr: '', token: '' }
  } catch (e) {
    toast(String(e), 'error')
  } finally {
    manualLanBusy.value = false
  }
}

const showWebdav = ref(false)
const webdav = ref({ url: '', username: '', password: '', name: '' })
const webdavBusy = ref(false)
async function submitWebdav() {
  if (webdavBusy.value) return
  webdavBusy.value = true
  try {
    await net.addWebDav(webdav.value.url.trim(), webdav.value.username, webdav.value.password, webdav.value.name.trim() || undefined)
    showWebdav.value = false
    webdav.value = { url: '', username: '', password: '', name: '' }
  } catch (e) {
    toast(String(e), 'error')
  } finally {
    webdavBusy.value = false
  }
}

async function toggleShare() {
  const on = !net.share?.running
  try {
    await net.toggleShare(on)
  } catch (e) {
    toast(String(e), 'error')
  }
}

function copyToken() {
  if (net.share?.token) {
    navigator.clipboard.writeText(net.share.token).then(
      () => toast('配对码已复制'),
      () => toast('复制失败', 'error'),
    )
  }
}
</script>

<template>
  <div ref="root" class="h-full overflow-y-auto px-6 pt-5 pb-8">
    <div class="mb-6">
      <p data-stagger class="text-xs font-semibold tracking-wider text-violet-500 uppercase">局域网</p>
      <h1 data-stagger class="mt-0.5 text-2xl font-bold text-zinc-900 dark:text-zinc-50">跨设备音乐共享</h1>
    </div>

    <div class="max-w-3xl space-y-8">
      <!-- 共享模式 -->
      <section data-stagger class="rounded-2xl border border-zinc-200 bg-white p-5 dark:border-zinc-800 dark:bg-zinc-900">
        <div class="flex items-center gap-3">
          <div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-violet-100 text-violet-600 dark:bg-violet-500/15 dark:text-violet-400">
            <RadioTower class="h-5 w-5" />
          </div>
          <div class="min-w-0 flex-1">
            <p class="font-semibold text-zinc-800 dark:text-zinc-100">在局域网共享我的音乐库</p>
            <p class="text-xs text-zinc-500">其他设备上的 LanMusic 可自动发现本机并播放（仅本地来源，只读）</p>
          </div>
          <button
            class="relative h-6 w-11 shrink-0 rounded-full transition"
            :class="net.share?.running ? 'bg-violet-500' : 'bg-zinc-200 dark:bg-zinc-700'"
            @click="toggleShare"
          >
            <span
              class="absolute top-0.5 h-5 w-5 rounded-full bg-white shadow transition-all"
              :class="net.share?.running ? 'left-[22px]' : 'left-0.5'"
            ></span>
          </button>
        </div>

        <div v-if="net.share?.running" class="mt-4 flex flex-wrap items-center gap-3 rounded-xl bg-zinc-50 px-4 py-3 text-sm dark:bg-zinc-800/60">
          <span class="text-zinc-500">端口</span>
          <span class="font-mono tabular-nums text-zinc-800 dark:text-zinc-100">{{ net.share.port }}</span>
          <span class="ml-4 text-zinc-500">配对码</span>
          <span class="font-mono text-lg tracking-[0.3em] text-violet-600 dark:text-violet-400">{{ net.share.token }}</span>
          <button
            class="flex items-center gap-1 rounded-full px-2.5 py-1 text-xs text-zinc-500 hover:bg-zinc-200 dark:hover:bg-zinc-700"
            title="复制配对码"
            @click="copyToken"
          >
            <Copy class="h-3.5 w-3.5" /> 复制
          </button>
          <span class="ml-auto text-xs text-zinc-400">在另一台设备上输入此配对码即可连接</span>
        </div>
      </section>

      <!-- 发现的设备 -->
      <section data-stagger>
        <div class="mb-3 flex items-center justify-between">
          <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">发现的设备</h2>
          <button
            class="flex items-center gap-1.5 rounded-full px-3 py-1.5 text-xs text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800"
            @click="net.discovering ? net.stopDiscover() : net.startDiscover()"
          >
            <RefreshCw class="h-3.5 w-3.5" :class="net.discovering ? 'animate-spin' : ''" />
            {{ net.discovering ? '正在扫描同网段…' : '重新扫描' }}
          </button>
        </div>

        <div class="space-y-2">
          <div
            v-for="d in net.devices"
            :key="d.id"
            class="flex items-center gap-3 rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900"
          >
            <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-zinc-100 text-zinc-500 dark:bg-zinc-800 dark:text-zinc-400">
              <Wifi class="h-4.5 w-4.5" />
            </div>
            <div class="min-w-0 flex-1">
              <p class="truncate text-sm font-medium text-zinc-800 dark:text-zinc-100">{{ d.name }}</p>
              <p class="font-mono text-xs text-zinc-500">{{ d.host }}:{{ d.port }}</p>
            </div>
            <button
              class="rounded-full bg-violet-500 px-4 py-1.5 text-xs font-medium text-white transition hover:bg-violet-400"
              @click="pairing = { device: d, token: '' }"
            >
              连接
            </button>
          </div>

          <p v-if="!Object.keys(net.devices).length" class="rounded-xl border border-dashed border-zinc-300 p-4 text-center text-sm text-zinc-400 dark:border-zinc-700">
            {{ net.discovering ? '正在搜索局域网内的 LanMusic 设备…' : '未发现设备，点击「重新扫描」或在对方设备上打开共享' }}
          </p>
        </div>
      </section>

      <!-- 手动添加 -->
      <section data-stagger>
        <div class="mb-3 flex items-center gap-2">
          <h2 class="text-sm font-semibold text-zinc-800 dark:text-zinc-100">手动添加来源</h2>
          <button
            class="flex items-center gap-1 rounded-full bg-zinc-100 px-3 py-1 text-xs text-zinc-600 hover:bg-zinc-200 dark:bg-zinc-800 dark:text-zinc-300 dark:hover:bg-zinc-700"
            @click="showManualLan = !showManualLan"
          >
            <Plus class="h-3 w-3" /> 局域网设备
          </button>
          <button
            class="flex items-center gap-1 rounded-full bg-zinc-100 px-3 py-1 text-xs text-zinc-600 hover:bg-zinc-200 dark:bg-zinc-800 dark:text-zinc-300 dark:hover:bg-zinc-700"
            @click="showWebdav = !showWebdav"
          >
            <Plus class="h-3 w-3" /> WebDAV / NAS
          </button>
        </div>

        <form
          v-if="showManualLan"
          class="mb-3 space-y-3 rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900"
          @submit.prevent="submitManualLan"
        >
          <div class="flex items-center gap-2 text-sm text-zinc-500">
            <Server class="h-4 w-4" /> 输入对方设备的地址与配对码
          </div>
          <div class="flex gap-3">
            <input
              v-model="manualLan.addr"
              class="h-9 flex-1 rounded-lg border border-zinc-200 bg-transparent px-3 text-sm outline-none focus:border-violet-400 dark:border-zinc-700"
              placeholder="IP 或 IP:端口，如 192.168.1.20:45678"
              required
            />
            <input
              v-model="manualLan.token"
              class="h-9 w-36 rounded-lg border border-zinc-200 bg-transparent px-3 font-mono text-sm outline-none focus:border-violet-400 dark:border-zinc-700"
              placeholder="配对码"
              required
            />
          </div>
          <button
            class="flex items-center gap-2 rounded-full bg-violet-500 px-4 py-1.5 text-sm font-medium text-white hover:bg-violet-400"
            :disabled="manualLanBusy"
          >
            <LoaderCircle v-if="manualLanBusy" class="h-4 w-4 animate-spin" />
            <Link2 v-else class="h-4 w-4" />
            连接设备
          </button>
        </form>

        <form
          v-if="showWebdav"
          class="space-y-3 rounded-xl border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900"
          @submit.prevent="submitWebdav"
        >
          <div class="flex items-center gap-2 text-sm text-zinc-500">
            <Globe class="h-4 w-4" /> 支持 https://nas.local:5006 或 http://192.168.1.2:5005
          </div>
          <div class="grid gap-3" style="grid-template-columns: 2fr 1fr 1fr">
            <input
              v-model="webdav.url"
              class="h-9 rounded-lg border border-zinc-200 bg-transparent px-3 text-sm outline-none focus:border-violet-400 dark:border-zinc-700"
              placeholder="WebDAV 地址"
              required
            />
            <input
              v-model="webdav.username"
              class="h-9 rounded-lg border border-zinc-200 bg-transparent px-3 text-sm outline-none focus:border-violet-400 dark:border-zinc-700"
              placeholder="账号"
              autocomplete="off"
            />
            <input
              v-model="webdav.password"
              type="password"
              class="h-9 rounded-lg border border-zinc-200 bg-transparent px-3 text-sm outline-none focus:border-violet-400 dark:border-zinc-700"
              placeholder="密码"
              autocomplete="off"
            />
          </div>
          <div class="flex items-center gap-3">
            <input
              v-model="webdav.name"
              class="h-9 w-56 rounded-lg border border-zinc-200 bg-transparent px-3 text-sm outline-none focus:border-violet-400 dark:border-zinc-700"
              placeholder="备注名（可选）"
            />
            <button
              class="flex items-center gap-2 rounded-full bg-violet-500 px-4 py-1.5 text-sm font-medium text-white hover:bg-violet-400"
              :disabled="webdavBusy"
            >
              <LoaderCircle v-if="webdavBusy" class="h-4 w-4 animate-spin" />
              <Check v-else class="h-4 w-4" />
              添加并扫描
            </button>
          </div>
        </form>
      </section>

      <!-- 配对码输入 -->
      <div
        v-if="pairing"
        class="fixed inset-0 z-[60] flex items-center justify-center bg-black/40 backdrop-blur-sm"
        @click.self="pairing = null"
      >
        <form
          class="w-[380px] rounded-2xl border border-zinc-200 bg-white p-5 shadow-2xl dark:border-zinc-700 dark:bg-zinc-800"
          @submit.prevent="confirmPairing"
        >
          <h2 class="text-base font-semibold text-zinc-900 dark:text-zinc-50">连接「{{ pairing.device.name }}」</h2>
          <p class="mt-1 text-sm text-zinc-500">
            输入对方设备共享页显示的 6 位配对码（{{ pairing.device.host }}:{{ pairing.device.port }}）
          </p>
          <input
            v-model="pairing.token"
            class="mt-4 h-11 w-full rounded-lg border border-zinc-200 bg-transparent px-3 text-center font-mono text-xl tracking-[0.4em] outline-none focus:border-violet-400 dark:border-zinc-700"
            placeholder="●●●●●●"
            maxlength="6"
            required
          />
          <div class="mt-5 flex justify-end gap-2">
            <button
              type="button"
              class="rounded-full px-4 py-1.5 text-sm text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-700"
              @click="pairing = null"
            >
              取消
            </button>
            <button
              class="flex items-center gap-2 rounded-full bg-violet-500 px-4 py-1.5 text-sm font-medium text-white hover:bg-violet-400"
              :disabled="connecting"
            >
              <LoaderCircle v-if="connecting" class="h-4 w-4 animate-spin" />
              连接
            </button>
          </div>
        </form>
      </div>
    </div>
  </div>
</template>
