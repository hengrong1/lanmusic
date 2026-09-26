import { test as base, expect, chromium, type Page } from '@playwright/test'

/**
 * CDP 连接 fixture：连上正在运行的 LanMusic（WebView2）主窗口。
 *
 * - 真实后端：页面里的 Tauri invoke 照常工作，不是 mock；
 * - 只操作页面 DOM（点击/输入），不碰原生 UI（任务栏/托盘不在 DOM 里）；
 * - 连接为模块级单例（整个套件只连一次 CDP，反复连断会让调试端点不稳定）；
 * - browser.close() 只断开 CDP 连接，应用窗口不会被关掉。
 */

const CDP = process.env.LANMUSIC_CDP ?? 'http://127.0.0.1:9222'
/** 等待应用出现（pnpm start 冷启动要编译 + 起窗口） */
const CONNECT_TIMEOUT_MS = 240_000

async function connectMainPage(): Promise<Page> {
  const browser = await chromium.connectOverCDP(CDP)
  const ctx = browser.contexts()[0]
  if (!ctx) throw new Error('CDP 已连上但没有浏览器上下文')
  // 主窗口不能用 URL 区分（桌面歌词/托盘小窗与主窗口同源）——用主界面特征元素识别
  const deadline = Date.now() + 10_000
  for (;;) {
    for (const p of ctx.pages()) {
      if (p.isClosed()) continue
      try {
        if ((await p.locator('#search-input').count()) > 0) return p
      } catch {
        // 页面可能正在跳转，忽略后重试
      }
    }
    if (Date.now() > deadline) {
      throw new Error(`CDP 已连上但未找到主窗口（共 ${ctx.pages().length} 个页面）`)
    }
    await new Promise((r) => setTimeout(r, 500))
  }
}

// 模块级单例连接：整个套件只连一次 CDP
let mainPage: Page | null = null
let consoleHooked = false
let connecting: Promise<Page> | null = null

async function getMainPage(): Promise<Page> {
  if (mainPage) return mainPage
  if (!connecting) {
    connecting = (async () => {
      let lastErr: unknown = null
      const deadline = Date.now() + CONNECT_TIMEOUT_MS
      for (;;) {
        try {
          const p = await connectMainPage()
          mainPage = p
          return p
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
    })()
  }
  return connecting
}

export const test = base.extend<{ page: Page }>({
  page: [
    async ({}, use, testInfo) => {
      const page = await getMainPage()
      // 窗口被遮挡时 WebView2 节流 rAF，视图入场过渡会卡在 opacity 0（view-guard 日志
      // op=0 可证），元素不可见导致点击全部挂起 → 每个用例前把窗口带到前台
      await page.bringToFront().catch(() => {})
      if (!consoleHooked) {
        consoleHooked = true
        page.on('pageerror', (e) => console.log('[pageerror]', e.message.slice(0, 300)))
        page.on('console', (m) => {
          if (m.type() === 'error') console.log('[console.error]', m.text().slice(0, 300))
        })
      }
      await use(page)
      // 留证：每个用例结束截一张图（成功也截，方便回看自动化做了什么）
      try {
        await page.screenshot({
          path: `test-results/e2e/${testInfo.title.replace(/[^\w\u4e00-\u9fa5-]+/g, '_')}.png`,
        })
      } catch {
        // 页面可能正忙，截图失败不影响用例结论
      }
    },
    { timeout: CONNECT_TIMEOUT_MS + 120_000 },
  ],
})

export { expect }
