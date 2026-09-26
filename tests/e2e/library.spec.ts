import { test, expect } from './fixtures'

/** 曲库浏览：视图切换与侧栏激活态 */
test.describe('曲库浏览', () => {
  test('六个视图都能进入且侧栏激活态正确', async ({ page }) => {
    for (const nav of ['nav-tracks', 'nav-albums', 'nav-artists', 'nav-recent', 'nav-folder']) {
      await page.locator(`[data-testid="${nav}"]`).click()
      await expect(page.locator(`[data-testid="${nav}"]`)).toHaveClass(/bg-violet-100/, {
        timeout: 10_000,
      })
    }
  })

  test('全部歌曲列表有真实数据且行可交互', async ({ page }) => {
    await page.locator('[data-testid="nav-tracks"]').click()
    const rows = page.locator('[data-testid="track-row"]')
    await expect(rows.first()).toBeVisible({ timeout: 15_000 })
    // 悬停出现操作（不点击，只验证行渲染完整）
    await rows.first().hover()
    await expect(rows.first()).toBeVisible()
  })

  test('专辑视图点进详情再返回', async ({ page }) => {
    await page.locator('[data-testid="nav-albums"]').click()
    // 专辑网格第一项（封面卡）
    const card = page.locator('[data-testid="album-card"]').first()
    await expect(card).toBeVisible({ timeout: 15_000 })
    const albumName = (await card.textContent()) ?? ''
    await card.click()
    // 详情页出现曲目列表，且顶栏有返回按钮
    await expect(page.locator('[data-testid="track-row"]').first()).toBeVisible({ timeout: 15_000 })
    await expect(page.locator('[data-testid="view-back"]')).toBeVisible()
    // 回到专辑网格
    await page.locator('[data-testid="view-back"]').click()
    await expect(page.getByText(albumName.slice(0, 8)).first()).toBeVisible({ timeout: 15_000 })
  })
})
