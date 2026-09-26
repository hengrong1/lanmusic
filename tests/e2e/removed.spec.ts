import { test, expect } from './fixtures'

/** 已移除歌曲：从曲库移除（不删文件）→ 记录出现 → 还原回曲库（隔离数据） */
test.describe('已移除歌曲', () => {
  test.fixme('VIEW-GUARD: 过渡卡 opacity 0: ', '移除 → 记录 → 还原', async ({ page }) => {
    await page.locator('[data-testid="nav-tracks"]').click()
    const rows = page.locator('[data-testid="track-row"]')
    await expect(rows.first()).toBeVisible({ timeout: 15_000 })
    const removedTitle = ((await rows.first().textContent()) ?? '').trim()

    // 右键 → 从曲库移除 → 确认
    await rows.first().click({ button: 'right' })
    await page.getByText('从曲库移除').click()
    await page.locator('[data-testid="confirm-accept"]').click()

    // 设置 → 已移除歌曲 → 查看记录
    await page.locator('[data-testid="nav-settings"]').click()
    await page.getByText('查看记录').click()
    const modal = page.getByText('已移除歌曲').first()
    await expect(modal).toBeVisible()
    await expect(page.getByText(removedTitle).first()).toBeVisible({ timeout: 15_000 })

    // 还原到曲库：触发来源增量扫描，记录消失
    await page.getByText('还原到曲库').first().click()
    await expect(page.getByText(removedTitle)).toHaveCount(0, { timeout: 120_000 })
  })
})
