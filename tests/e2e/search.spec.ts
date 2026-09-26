import { test, expect } from './fixtures'

/** 搜索：下拉结果 + Enter 进结果视图 */
test.describe('搜索', () => {
  test('输入关键词出现下拉结果', async ({ page }) => {
    await page.locator('#search-input').click()
    await page.locator('#search-input').fill('a')
    const results = page.locator('.search-pop ul li')
    await expect(results.first()).toBeVisible({ timeout: 15_000 })
  })

  test('回车进入搜索结果视图，列表渲染', async ({ page }) => {
    await page.locator('#search-input').click()
    await page.locator('#search-input').fill('a')
    await page.locator('#search-input').press('Enter')
    const rows = page.locator('[data-testid="track-row"]')
    await expect(rows.first()).toBeVisible({ timeout: 15_000 })
    const count = await rows.count()
    // eslint-disable-next-line no-console
    console.log(`搜索结果视图 ${count} 行`)
    expect(count).toBeGreaterThan(0)
  })
})
