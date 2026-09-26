import { test, expect } from './fixtures'

/** 已移除歌曲：从曲库移除（不删文件）→ 记录出现 → 还原回曲库（隔离数据） */
test.describe('已移除歌曲', () => {
  test('移除 → 记录 → 还原', async ({ page }) => {
    await page.locator('[data-testid="nav-tracks"]').click()
    console.log('[rm] 1 nav-tracks')
    const rows = page.locator('[data-testid="track-row"]')
    await expect(rows.first()).toBeVisible({ timeout: 15_000 })
    // 只取标题单元格文本（整行 textContent 是 索引+标题+艺人+专辑+时长 的拼接）
    const removedTitle = (
      (await rows.first().locator('div').nth(1).textContent()) ??
      ''
    ).trim()

    // 右键 → 从曲库移除 → 确认
    await rows.first().click({ button: 'right' })
    console.log('[rm] 2 contextmenu')
    await page.getByText('从曲库移除').click()
    console.log('[rm] 3 remove-click')
    await page.locator('[data-testid="confirm-accept"]').click()
    console.log('[rm] 4 confirmed')

    // 设置 → 已移除歌曲 → 查看记录
    await page.locator('[data-testid="nav-settings"]').click()
    console.log('[rm] 5 nav-settings')
    await page.getByText('查看记录').click()
    console.log('[rm] 6 open-modal')
    await page.screenshot({ path: 'test-results/e2e/debug/rm-modal.png' })
    // eslint-disable-next-line no-console
    console.log('[rm] modal 标题可见:', await page.getByText('已移除歌曲').first().isVisible().catch(() => false))
    const modal = page.getByText('已移除歌曲').first()
    await expect(modal).toBeVisible()
    await expect(page.getByText(removedTitle).first()).toBeVisible({ timeout: 15_000 })

    // 还原到曲库：触发来源增量扫描，记录消失
    // 还原：弹窗只提供「全部还原」（触发来源增量扫描，记录随曲目重新入库而清空）
    await page.getByText('全部还原').click({ timeout: 10_000 })
    console.log('[rm] 7 restore-all-click')
    await expect(page.getByText(removedTitle).first()).toHaveCount(0, { timeout: 120_000 })
    console.log('[rm] 8 restored')
  })
})
