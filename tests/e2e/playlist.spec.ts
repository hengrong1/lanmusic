import { test, expect } from './fixtures'

/** 歌单全流程：新建 → 添加歌曲 → 重命名 → 删除（全程隔离数据） */
test.describe('歌单', () => {
  const NAME = 'E2E 测试歌单'
  const NAME2 = 'E2E 测试歌单（重命名）'

  test.fixme('VIEW-GUARD: 过渡卡 opacity 0: ', '新建 → 添加歌曲 → 重命名 → 删除', async ({ page }) => {
    // 新建
    await page.locator('[data-testid="playlist-create"]').click()
  await page.screenshot({ path: 'test-results/e2e/debug/playlist-1-create-dialog.png' })
    await page.locator('[data-testid="playlist-name"]').fill(NAME)
    await page.locator('[data-testid="playlist-save"]').click()
  await page.screenshot({ path: 'test-results/e2e/debug/playlist-10-renamed.png' })
  await page.screenshot({ path: 'test-results/e2e/debug/playlist-2-saved.png' })
    await expect(page.locator('.sidebar, nav').getByText(NAME)).toBeVisible({ timeout: 10_000 })

    // 添加歌曲：右键第一行 → 添加到歌单 → 子菜单点歌单名
    await page.locator('[data-testid="nav-tracks"]').click()
  await page.screenshot({ path: 'test-results/e2e/debug/playlist-3-nav-tracks.png' })
    const rows = page.locator('[data-testid="track-row"]')
    await expect(rows.first()).toBeVisible({ timeout: 15_000 })
    await rows.first().click({ button: 'right' })
  await page.screenshot({ path: 'test-results/e2e/debug/playlist-4-contextmenu.png' })
    await page.getByText('添加到歌单').hover()
  await page.screenshot({ path: 'test-results/e2e/debug/playlist-5-hover-submenu.png' })
    await page.locator('body').getByText(NAME).last().click()
  await page.screenshot({ path: 'test-results/e2e/debug/playlist-6-added.png' })

    // 打开歌单，应恰好 1 首
    await page.locator('.sidebar, nav').getByText(NAME).first().click()
  await page.screenshot({ path: 'test-results/e2e/debug/playlist-7-opened-playlist.png' })
    await expect(page.locator('[data-testid="track-row"]')).toHaveCount(1, { timeout: 10_000 })

    // 重命名：右键歌单 → 重命名
    await page.locator('nav').getByText(NAME).first().click({ button: 'right' })
  await page.screenshot({ path: 'test-results/e2e/debug/playlist-8-pl-contextmenu.png' })
    await page.getByText('重命名').click()
  await page.screenshot({ path: 'test-results/e2e/debug/playlist-9-rename-dialog.png' })
    await page.locator('[data-testid="playlist-name"]').fill(NAME2)
    await page.locator('[data-testid="playlist-save"]').click()
    await expect(page.locator('nav').getByText(NAME2)).toBeVisible({ timeout: 10_000 })

    // 删除：右键歌单 → 删除 → 确认
    await page.locator('nav').getByText(NAME2).first().click({ button: 'right' })
  await page.screenshot({ path: 'test-results/e2e/debug/playlist-11-pl-contextmenu2.png' })
    await page.getByText('删除').click()
  await page.screenshot({ path: 'test-results/e2e/debug/playlist-12-delete-menu.png' })
    await page.locator('[data-testid="confirm-accept"]').click()
  await page.screenshot({ path: 'test-results/e2e/debug/playlist-13-delete-confirmed.png' })
    await expect(page.locator('nav').getByText(NAME2)).toHaveCount(0, { timeout: 10_000 })
  })
})
