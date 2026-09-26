import { test, expect } from './fixtures'

/**
 * 最小演示用例集（全部只读，不改曲库数据）：
 * 启动渲染、顶部搜索、进入设置、已合并名单弹窗。
 */

test('应用启动并渲染主界面', async ({ page }) => {
  await expect(page.locator('#search-input')).toBeVisible()
})

test.fixme('顶部搜索:输入后下拉出现真实搜索结果', async ({ page }) => {
  await page.locator('#search-input').click()
  await page.locator('#search-input').fill('a')
  // 搜索经真实后端（Tauri invoke + SQL），下拉列出命中的曲目
  const results = page.locator('.search-pop ul li')
  await expect(results.first()).toBeVisible({ timeout: 15_000 })
  // eslint-disable-next-line no-console
  console.log(`搜索 "a" 命中 ${await results.count()} 条`)
})

test.fixme('侧边栏进入设置页', async ({ page }) => {
  await page.locator('[data-testid="nav-settings"]').click()
  // 「已合并名单」区块常驻渲染（无记录时显示 0）
  await expect(page.getByText('已合并').first()).toBeVisible()
})

test.fixme('已合并名单:打开管理弹窗并关闭', async ({ page }) => {
  await page.locator('[data-testid="nav-settings"]').click()
  const manage = page.locator('[data-testid="merged-manage"]')
  const hasMerged = (await manage.count()) > 0
  test.skip(!hasMerged, '曲库当前没有合并记录，跳过弹窗用例')
  await manage.click()
  const dialog = page.locator('[data-testid="merged-dialog"]')
  await expect(dialog).toBeVisible()
  await page.locator('[data-testid="merged-dialog-close"]').click()
  await expect(dialog).not.toBeVisible()
})
