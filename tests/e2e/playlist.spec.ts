import { test, expect } from './fixtures'

const log = (m: string) => console.log(`[playlist] ${m}`)

/** 歌单全流程：新建 → 添加歌曲 → 重命名 → 删除（全程隔离数据） */
test.describe('歌单', () => {
  const NAME = 'E2E 测试歌单'
  const NAME2 = 'E2E 测试歌单（重命名）'

  test('新建 → 添加歌曲 → 重命名 → 删除', async ({ page }) => {
    test.setTimeout(120_000)

    // 新建
    await page.locator('[data-testid="playlist-create"]').click({ timeout: 10_000 })
    log('1 create-dialog')
    await page.locator('[data-testid="playlist-name"] input').fill(NAME)
    await page.locator('[data-testid="playlist-save"]').click({ timeout: 10_000 })
    log('2 saved')
    await expect(page.locator('nav').getByText(NAME)).toBeVisible({ timeout: 10_000 })

    // 添加歌曲：右键第一行 → 添加到歌单（hover 开子菜单）→ 点歌单名
    await page.locator('[data-testid="nav-tracks"]').click()
    const rows = page.locator('[data-testid="track-row"]')
    await expect(rows.first()).toBeVisible({ timeout: 15_000 })
    await rows.first().click({ button: 'right' })
    log('3 row-contextmenu')
  await page.screenshot({ path: 'test-results/e2e/debug/pl-contextmenu.png' })
  // eslint-disable-next-line no-console
  console.log('[playlist] 菜单元素数:', await page.locator('ContextMenu, [class*="min-w-44"]').count())
    await page.getByText('加入歌单').hover()
    log('4 hover-add')
    await page.locator('body').getByText(NAME).last().click({ timeout: 10_000 })
    log('5 picked-playlist')

    // 打开歌单，应恰好 1 首
    await page.locator('nav').getByText(NAME).first().click()
    log('6 opened-playlist')
    await expect(page.locator('[data-testid="track-row"]')).toHaveCount(1, { timeout: 10_000 })

    // 重命名：右键歌单 → 重命名
    await page.locator('nav').getByText(NAME).first().click({ button: 'right' })
    await page.getByText('重命名').click({ timeout: 10_000 })
    log('7 rename-dialog')
    await page.locator('[data-testid="playlist-name"] input').fill(NAME2)
    await page.locator('[data-testid="playlist-save"]').click({ timeout: 10_000 })
    log('8 renamed')
    await expect(page.locator('nav').getByText(NAME2)).toBeVisible({ timeout: 10_000 })

    // 删除：右键歌单 → 删除 → 确认
    await page.locator('nav').getByText(NAME2).first().click({ button: 'right' })
    await page.getByText('删除').click({ timeout: 10_000 })
    log('9 delete-click')
    await page.locator('[data-testid="confirm-accept"]').click({ timeout: 10_000 })
    log('10 delete-confirmed')
    await expect(page.locator('nav').getByText(NAME2)).toHaveCount(0, { timeout: 10_000 })
    log('11 done')
  })
})
