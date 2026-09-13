/// 自定义协议 URL 构建。
/// Windows 上 Tauri 自定义协议以 http://{scheme}.localhost 形式访问。
import { IS_WIN } from '@/utils/platform'

export function trackStreamUrl(id: number): string {
  return IS_WIN ? `http://music.localhost/track/${id}` : `music://track/${id}`
}

export function coverUrl(albumId: number | null | undefined): string | null {
  if (albumId == null) return null
  return IS_WIN ? `http://cover.localhost/album/${albumId}` : `cover://album/${albumId}`
}

/// 自定义背景：bg://file/{name}（Rust 侧复制到 appData/backgrounds/ 的文件名）
export function bgUrl(file: string): string {
  return IS_WIN ? `http://bg.localhost/file/${file}` : `bg://file/${file}`
}
