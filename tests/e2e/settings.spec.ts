import { test, expect } from './fixtures'

/** 设置：主题切换 / 语言切换 / 桌面歌词窗口 / 听歌统计入口与诊断 */
test.describe('设置', () => {
  test.beforeEach(async ({ page }) => {
    await page.locator('[data-testid="nav-settings"]').click()
  })

  test.fixme('VIEW-GUARD: 过渡卡 opacity 0: ', '主题切换：深色/浅色类切换', async ({ page }) => {
    const html = page.locator('html')
    const darkBtn = page.locator('[data-testid="theme-group"]').getByText('深色')
    const lightBtn = page.locator('[data-testid="theme-group"]').getByText('浅色')
    await darkBtn.click()
    await expect(html).toHaveClass(/dark/, { timeout: 10_000 })
    await lightBtn.click()
    await expect(html).not.toHaveClass(/dark/)
    // 收尾回到跟随系统
    await page.locator('[data-testid="theme-group"]').getByText('跟随系统').click()
  })

  test.fixme('VIEW-GUARD: 过渡卡 opacity 0: ', '语言切换 zh → en → zh', async ({ page }) => {
    await page.locator('[data-testid="language-select"]').scrollIntoViewIfNeeded()
    await page.waitForTimeout(800) // 等平滑滚动结束，避免面板被 outside-scroll 关闭
    const langSel = page.locator('[data-testid="language-select"] button')
    await langSel.focus()
    await langSel.press('ArrowDown') // 打开面板（须立即选择，见 merge.spec 注释）
    await page.locator('[data-option]').filter({ hasText: 'English' }).first().click()
    await expect(page.getByText('old names merged').first()).toBeVisible({ timeout: 10_000 })
    // 切回中文
    await langSel.focus()
    await langSel.press('ArrowDown')
    await page.locator('[data-option]').filter({ hasText: '简体中文' }).first().click()
    await expect(page.getByText('已合并').first()).toBeVisible({ timeout: 10_000 })
  })

  test.fixme('VIEW-GUARD: 过渡卡 opacity 0: ', '桌面歌词开关：出现独立歌词窗口（新 CDP 页面）', async ({ page }) => {
    const pages = () => page.context().pages().length
    const before = pages()
    await page.locator('[data-testid="dl-switch"]').click()
    await expect
      .poll(() => pages(), { timeout: 15_000 })
      .toBe(before + 1)
    // 关闭 → 窗口销毁
    await page.locator('[data-testid="dl-switch"]').click()
    await expect.poll(() => pages(), { timeout: 15_000 }).toBe(before)
  })

  test.fixme('VIEW-GUARD: 过渡卡 opacity 0: ', '听歌统计入口开启后侧栏出现统计项，诊断页有采样数据', async ({ page }) => {
    const navStats = page.locator('[data-testid="nav-stats"]')
    const enabled = (await navStats.count()) > 0
    if (!enabled) {
      await page.locator('[data-testid="stats-switch"]').click()
      await expect(navStats).toBeVisible({ timeout: 10_000 })
    }
    await navStats.click()
    // 听歌统计页：切到「诊断」Tab（默认在听歌数据 Tab）
    await page.getByText('诊断').click()
    await expect(page.getByText('CPU').first()).toBeVisible({ timeout: 20_000 })
    // 收尾：关闭统计入口
    await page.locator('[data-testid="nav-settings"]').click()
    if ((await page.locator('[data-testid="stats-switch"]').count()) > 0) {
      const sw = page.locator('[data-testid="stats-switch"]')
      // 仅当初开时关：读 aria 或直接点两次无法判断，这里以开关当前态为准
      const pressed = await sw.getAttribute('aria-checked')
      if (pressed === 'true') await sw.click()
    }
  })
})
