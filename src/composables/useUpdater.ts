import { ref } from 'vue'
import { check, type Update } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import { toast } from '@/composables/useToast'

/**
 * 应用内更新（tauri-plugin-updater + GitHub Releases）。
 *
 * 流程：checkForUpdate()（检查 latest.json，签名校验由 Rust 侧完成）
 *      → available 时提示 → downloadAndInstall()（带下载进度）
 *      → ready → relaunch() 重启应用完成更新。
 *
 * 前置条件：
 * - tauri.conf.json 已配置 updater 端点（需替换 YOUR_GITHUB_USERNAME）；
 * - GitHub 仓库 Secrets 配置 TAURI_SIGNING_PRIVATE_KEY（私钥见 ~/.tauri/lanmusic.key）。
 */

export type UpdateStatus =
  | 'idle' // 未检查
  | 'checking' // 检查中
  | 'available' // 有新版本
  | 'downloading' // 下载安装中
  | 'ready' // 安装完成待重启
  | 'uptodate' // 已是最新

const status = ref<UpdateStatus>('idle')
/** 新版本号 */
const newVersion = ref('')
/** 新版说明（Release body） */
const releaseNotes = ref('')
/** 下载进度 0-1（total 未知时为 -1，UI 显示不定进度） */
const progress = ref(-1)
const downloadedMb = ref(0)
const totalMb = ref(0)

let update: Update | null = null

/** 检查更新。silent=true 用于启动时静默检查（有更新才提示，出错不弹框） */
async function checkForUpdate(silent = false): Promise<boolean> {
  if (status.value === 'checking' || status.value === 'downloading') return false
  status.value = 'checking'
  try {
    // check() 内部会对比本地与远端版本，远端更高才返回 Update
    update = (await check()) ?? null
    if (update) {
      newVersion.value = update.version
      releaseNotes.value = update.body ?? ''
      status.value = 'available'
      progress.value = -1
      if (silent) toast(`发现新版本 v${update.version}，可到「设置 → 关于」更新`)
      return true
    }
    status.value = 'uptodate'
    if (!silent) toast('当前已是最新版本')
    return false
  } catch (e) {
    status.value = 'idle'
    if (!silent) toast(`检查更新失败：${e}`, 'error')
    return false
  }
}

/** 下载并安装更新（安装完成后 status → ready，调用 relaunch 重启生效） */
async function downloadAndInstall(): Promise<void> {
  if (!update || status.value !== 'available') return
  status.value = 'downloading'
  progress.value = -1
  try {
    await update.downloadAndInstall((event) => {
      switch (event.event) {
        case 'Started':
          if (event.data.contentLength) {
            progress.value = 0
            totalMb.value = Math.round(event.data.contentLength / 1024 / 1024)
          }
          break
        case 'Progress': {
          downloadedMb.value += event.data.chunkLength / 1024 / 1024
          if (progress.value >= 0 && totalMb.value > 0) {
            progress.value = Math.min(1, downloadedMb.value / totalMb.value)
          }
          break
        }
        case 'Finished':
          progress.value = 1
          break
      }
    })
    status.value = 'ready'
    toast('更新已下载完成，重启应用后生效')
  } catch (e) {
    status.value = 'available'
    toast(`更新下载失败：${e}`, 'error')
  }
}

/** 重启应用使更新生效 */
async function restartToUpdate(): Promise<void> {
  await relaunch()
}

export function useUpdater() {
  return {
    status,
    newVersion,
    releaseNotes,
    progress,
    downloadedMb,
    totalMb,
    checkForUpdate,
    downloadAndInstall,
    restartToUpdate,
  }
}