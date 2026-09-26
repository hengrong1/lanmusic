import { test, expect } from './fixtures'

/** 检查更新：E2E 实例版本与最新 Release 一致 → 「已是最新」 */
test.describe('检查更新', () => {
  test.fixme('VIEW-GUARD: 过渡卡 opacity 0: ', '检查更新显示已是最新', async ({ page }) => {
    await page.locator('[data-testid="nav-settings"]').click()
    await page.locator('[data-testid="check-update"]').click()
    // 按钮 loading 结束后旁边出现「已是最新」短文案
    await expect(page.getByText('已是最新').first()).toBeVisible({ timeout: 30_000 })
  })
})
