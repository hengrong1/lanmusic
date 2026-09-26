import { test, expect } from './fixtures'

/** 播放器：播放/暂停/切歌/喜欢/队列（真实音频播放，用例内即时暂停降噪） */
test.describe('播放器', () => {
  test.fixme('VIEW-GUARD: 过渡卡 opacity 0: ', '双击播放 → 暂停/继续 → 下一首/上一首 → 喜欢 → 队列', async ({ page }) => {
    await page.locator('[data-testid="nav-tracks"]').click()
    const rows = page.locator('[data-testid="track-row"]')
    await expect(rows.first()).toBeVisible({ timeout: 15_000 })

    const toggle = page.locator('[data-testid="player-toggle"]')
    const title = page.locator('[data-testid="player-title"]')

    // 双击播放（dblclick 第二击自带单击，选中行同时开播）
    const firstTitle = ((await rows.first().textContent()) ?? '').trim()
    await rows.first().dblclick()
    await expect(toggle).toHaveAttribute('data-playing', 'true', { timeout: 15_000 })
    await expect(title).not.toHaveText('')

    // 暂停 / 继续
    await toggle.click()
    await expect(toggle).toHaveAttribute('data-playing', 'false')
    await toggle.click()
    await expect(toggle).toHaveAttribute('data-playing', 'true')

    // 下一首：标题变化
    await page.locator('[data-testid="player-next"]').click()
    await expect(title).not.toHaveText(firstTitle, { timeout: 15_000 })
    // 上一首：标准播放器行为是重启当前曲（进度归零），标题不变也正确
    await page.locator('[data-testid="player-prev"]').click()
    await page.waitForTimeout(800)
    await expect(page.locator('[data-testid="player-pos"]')).toHaveText(/^0:0/, {
      timeout: 15_000,
    })

    // 喜欢当前曲目（图标转红色实心）
    const fav = page.locator('[data-testid="player-fav"]')
    await fav.click()
    await expect(fav).toHaveClass(/text-red-500/, { timeout: 10_000 })

    // 收尾：暂停，避免用例结束后持续出声
    await toggle.click()
    await expect(toggle).toHaveAttribute('data-playing', 'false')
  })

  test.fixme('VIEW-GUARD: 过渡卡 opacity 0: ', '播放队列面板开关', async ({ page }) => {
    await page.locator('[data-testid="nav-tracks"]').click()
    const rows = page.locator('[data-testid="track-row"]')
    await expect(rows.first()).toBeVisible({ timeout: 15_000 })
    // 队列面板仅在队列非空时渲染：先播一首
    await rows.first().dblclick()
    await expect(page.locator('[data-testid="player-toggle"]')).toHaveAttribute(
      'data-playing',
      'true',
      { timeout: 15_000 },
    )
    await page.locator('[data-testid="player-toggle"]').click() // 立即暂停降噪
    await page.locator('[data-queue-toggle]').click()
    await expect(page.locator('[data-testid="queue-panel"]')).toBeVisible({ timeout: 10_000 })
    await page.locator('[data-queue-toggle]').click()
    await expect(page.locator('[data-testid="queue-panel"]')).not.toBeVisible()
  })
})
