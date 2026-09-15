import { ref } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { openUrl } from '@tauri-apps/plugin-opener'
import { getVersion } from '@tauri-apps/api/app'
import { api } from '@/api/commands'
import { toast } from '@/composables/useToast'
import { t } from '@/i18n/translate'
import { errorText } from '@/i18n/error'

/**
 * 应用内更新（自研，GitHub Releases；Windows 打包为 Inno Setup，见 installer/）。
 *
 * 流程：checkForUpdate()（Rust 侧查 Release 并比较版本，见 updater.rs）
 *      → available（有新版本，展示说明）
 *      → downloadUpdate()（Rust 侧下载 + SHA-256 校验，进度经
 *        `update:download-progress` 事件回传）
 *      → ready → installAndRestart()（静默安装 + 装完自动启动新版，当前进程退出）。
 *
 * Release 资产约定：`LanMusic_<版本>_x64-setup.exe` + 同名 `.sha256`。
 * 无安装包资产时（例如仅 dmg）退化为 openReleasePage() 跳转 Release 页手动下载。
 */

export type UpdateStatus =
  | 'idle' // 未检查
  | 'checking' // 检查中
  | 'available' // 有新版本待下载
  | 'downloading' // 下载安装包中
  | 'ready' // 已下载校验通过，待安装
  | 'uptodate' // 已是最新

const status = ref<UpdateStatus>('idle')
/** 更新弹窗开关：发现新版本时自动打开，稍后/关闭后可在「设置 → 关于」重新触发 */
const dialogOpen = ref(false)
/** 当前版本号（getVersion()，失败时为空串，UI 降级不显示） */
const currentVersion = ref('')
/** 新版本号 */
const newVersion = ref('')
/** 新版说明（Release body 剥离后的纯文本，无结构；富文本字段为空时回退展示） */
const releaseNotes = ref('')
/** 新版说明（GitHub 渲染的 HTML，Rust 侧 sanitize 净化；空串 = 无富文本，回退纯文本） */
const releaseNotesHtml = ref('')
/** Release 页面地址（无安装包资产或用户想手动下载时打开） */
const releaseUrl = ref('')
/** 安装包下载地址（有值时可用应用内下载） */
const assetUrl = ref('')
const assetName = ref('')
const assetSize = ref(0)
/** SHA-256 校验文件地址（空串表示该 Release 未提供校验文件） */
const sha256Url = ref('')
/** 已下载校验通过的安装包本地路径（installAndRestart 使用） */
const installerPath = ref('')
/** 下载进度 0-1（总量未知时为 -1，UI 显示不定进度） */
const progress = ref(-1)
const downloadedMb = ref(0)
const totalMb = ref(0)

/** 是否能应用内更新（有安装包资产） */
function canAutoUpdate(): boolean {
  return !!assetUrl.value
}

interface DownloadProgressPayload {
  downloaded: number
  total: number
}

let progressListening = false
/** 订阅下载进度事件（模块级单例，一次即可）。Rust 侧 200ms 节流。 */
function ensureProgressListener(): void {
  if (progressListening) return
  progressListening = true
  listen<DownloadProgressPayload>('update:download-progress', (e) => {
    const { downloaded, total } = e.payload
    downloadedMb.value = downloaded / 1024 / 1024
    totalMb.value = total / 1024 / 1024
    progress.value = total > 0 ? Math.min(1, downloaded / total) : -1
  }).catch(() => {
    // 注册失败（极少数情况）允许下次进入下载流程时重试
    progressListening = false
  })
}

/** 读取当前版本号（只取一次，取不到留空，UI 自动隐藏该段） */
async function loadCurrentVersion(): Promise<void> {
  if (currentVersion.value) return
  try {
    currentVersion.value = await getVersion()
  } catch {
    currentVersion.value = ''
  }
}

/** 检查更新。silent=true 用于启动时静默检查（有更新才提示，出错不弹框） */
async function checkForUpdate(silent = false): Promise<boolean> {
  if (status.value === 'checking' || status.value === 'downloading') return false
  status.value = 'checking'
  void loadCurrentVersion()
  try {
    const release = await api.checkGithubUpdate()
    if (release) {
      newVersion.value = release.version
      releaseNotes.value = release.notes
      releaseNotesHtml.value = release.notesHtml ?? ''
      releaseUrl.value = release.htmlUrl
      assetUrl.value = release.assetUrl ?? ''
      assetName.value = release.assetName ?? ''
      assetSize.value = release.assetSize ?? 0
      sha256Url.value = release.sha256Url ?? ''
      installerPath.value = ''
      status.value = 'available'
      progress.value = -1
      // 发现新版本：弹出更新弹窗（无论启动静默检查还是手动检查）
      dialogOpen.value = true
      return true
    }
    status.value = 'uptodate'
    if (!silent) toast(t('settings.upToDate'))
    return false
  } catch (e) {
    status.value = 'idle'
    if (!silent) toast(t('toast.checkUpdateFailed', { error: errorText(e) }), 'error')
    return false
  }
}

/** 下载安装包（Rust 侧流式下载 + SHA-256 校验），完成后 status → ready */
async function downloadUpdate(): Promise<void> {
  if (status.value !== 'available' || !canAutoUpdate()) return
  ensureProgressListener()
  status.value = 'downloading'
  // 进度归零：上次失败/重试不能从旧值累加
  progress.value = -1
  downloadedMb.value = 0
  totalMb.value = assetSize.value > 0 ? assetSize.value / 1024 / 1024 : 0
  try {
    installerPath.value = await api.downloadUpdateInstaller(
      assetUrl.value,
      sha256Url.value || null,
      assetSize.value || null,
    )
    status.value = 'ready'
    toast(t('settings.updateDownloadedHint'))
  } catch (e) {
    status.value = 'available'
    // 校验失败/网络失败的原文在 Rust 侧已写明（含 SHA-256 结论），这里原样透出
    toast(t('toast.updateDownloadFailed', { error: errorText(e) }), 'error')
  }
}

/** 安装并重启：静默运行安装包，装完自动启动新版（当前进程随后退出） */
async function installAndRestart(): Promise<void> {
  if (status.value !== 'ready' || !installerPath.value) return
  try {
    await api.installUpdateAndRestart(installerPath.value)
  } catch (e) {
    toast(t('toast.updateDownloadFailed', { error: errorText(e) }), 'error')
  }
}

/** 打开 GitHub Release 页手动下载（无安装包资产，或用户偏好手动下载） */
async function openReleasePage(): Promise<void> {
  if (!releaseUrl.value) return
  try {
    await openUrl(releaseUrl.value)
    dialogOpen.value = false
  } catch (e) {
    toast(t('toast.checkUpdateFailed', { error: errorText(e) }), 'error')
  }
}

/** 关闭更新弹窗（下载中禁止关闭，避免误触后丢失进度提示） */
function closeUpdateDialog(): void {
  if (status.value === 'downloading') return
  dialogOpen.value = false
}

export function useUpdater() {
  return {
    status,
    dialogOpen,
    currentVersion,
    newVersion,
    releaseNotes,
    releaseNotesHtml,
    releaseUrl,
    assetName,
    installerPath,
    progress,
    downloadedMb,
    totalMb,
    canAutoUpdate,
    checkForUpdate,
    downloadUpdate,
    installAndRestart,
    openReleasePage,
    closeUpdateDialog,
  }
}
