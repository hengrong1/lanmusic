import { test, expect } from './fixtures'

/** 检查更新：E2E 实例版本与最新 Release 一致 → 「已是最新」（网络慢时自动重试） */
test.describe('检查更新', () => {
  test('检查更新显示已是最新', async ({ page }) => {
    test.setTimeout(180_000)
    await page.locator('[data-testid="nav-settings"]').click({ timeout: 10_000 })
    // 网络抖动可能让单次检查失败（toast 提示），最多重试 3 次
    for (let attempt = 1; attempt <= 3; attempt++) {
      await page.locator('[data-testid="check-update"]').click({ timeout: 10_000 })
      const ok = await page
        .getByText('已是最新')
        .first()
        .isVisible({ timeout: 45_000 })
        .catch(() => false)
      if (ok) return
      // 失败后按钮回到可用态，稍候重试
      await expect(page.locator('[data-testid="check-update"]')).toBeEnabled({ timeout: 10_000 })
      await page.waitForTimeout(1000)
    }
    throw new Error('检查更新重试 3 次仍未显示「已是最新」')
  })
})
