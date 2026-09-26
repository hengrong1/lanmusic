import { test as base, expect, chromium, type Page } from '@playwright/test'

/**
 * CDP 连接 fixture：连上正在运行的 LanMusic（WebView2）主窗口。
 *
 * - 真实后端：页面里的 Tauri invoke 照常工作，不是 mock；
 * - 只操作页面 DOM（点击/输入），不碰原生 UI（任务栏/托盘不在 DOM 里）；
 * - browser.close() 只断开 CDP 连接，应用窗口不会被关掉。
 */

const CDP = process.env.LANMUSIC_CDP ?? 'http://127.0.0.1:9222'
/** 等待应用出现（pnpm start 冷启动要编译 + 起窗口） */
const CONNECT_TIMEOUT_MS = 120_000

async function connectMainPage(): Promise<Page> {
  const browser = await chromium.connectOverCDP(CDP)
  const ctx = browser.contexts()[0]
  const page =
    // dev 模式页面 URL 是 http://localhost:1420；打包版是 tauri://localhost 或 http://tauri.localhost
    ctx?.pages().find((p) => !p.isClosed() && /localhost|tauri/.test(p.url())) ??
    ctx?.pages()[0]
  if (!page) throw new Error('CDP 已连上但未找到 LanMusic 页面')
  return page
}

export const test = base.extend<{ page: Page }>({
  page: [
    async ({}, use, testInfo) => {
      let lastErr: unknown = null
      const deadline = Date.now() + CONNECT_TIMEOUT_MS
      let page: Page | null = null
      while (page === null) {
        try {
          page = await connectMainPage()
        } catch (e) {
          lastErr = e
          if (Date.now() > deadline) {
            throw new Error(
              `连接 LanMusic 失败（CDP=${CDP}）：请先用调试端口启动应用（scripts/e2e.ps1）。最后错误：${String(lastErr)}`,
            )
          }
          await new Promise((r) => setTimeout(r, 1000))
        }
      }
      await use(page)
      // 留证：每个用例结束截一张图（成功也截，方便回看自动化做了什么）
      try {
        await page.screenshot({
          path: `../../test-results/e2e/${testInfo.title.replace(/[^\w\u4e00-\u9fa5-]+/g, '_')}.png`,
        })
      } catch {
        // 页面可能正忙，截图失败不影响用例结论
      }
    },
    { timeout: CONNECT_TIMEOUT_MS + 60_000 },
  ],
})

export { expect }
