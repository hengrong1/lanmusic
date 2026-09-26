import { defineConfig } from '@playwright/test'

/**
 * LanMusic E2E（WebView2 CDP 直连）。
 *
 * 前提：应用以调试端口启动（scripts/e2e.ps1 会自动设置）：
 *   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222
 * 连接地址可用 LANMUSIC_CDP 环境变量覆盖。端口未开时 fixture 会重试 30s。
 */
export default defineConfig({
  testDir: '.',
  timeout: 60_000,
  workers: 1, // 共享同一个应用实例，用例串行
  fullyParallel: false,
  retries: 0,
  reporter: [['list']],
  outputDir: '../../test-results/e2e',
})
