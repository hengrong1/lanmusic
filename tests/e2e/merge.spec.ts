import { test, expect } from './fixtures'

const log = (m: string) => console.log(`[merge] ${m}`)

/**
 * 打开 BaseSelect 面板并选中第 optionIndex 个选项。
 * 坑位记录：
 * - 合成 click 会触发文档级监听把刚打开的面板关掉 → 开面板与选择间不能有多余 await；
 * - openPanel 的焦点迁移（聚焦面板搜索框）可能被窗口激活时序吞掉 → 显式补焦；
 * - 面板打开后按键必须用 page.keyboard（locator.press 会把焦点拉回触发器）。
 */
async function openAndPick(
  page: import('@playwright/test').Page,
  testid: string,
  optionIndex: number,
) {
  for (let attempt = 1; attempt <= 4; attempt++) {
    await page.locator(`[data-testid="${testid}"] button`).click({ timeout: 10_000 })
    const search = page.locator('[role="listbox"] input').first()
    try {
      await search.waitFor({ state: 'visible', timeout: 3_000 })
      await search.focus()
      const opt = page.locator('[role="listbox"] [data-option]').nth(optionIndex)
      await opt.waitFor({ state: 'visible', timeout: 5_000 }) // 等异步选项加载
      for (let i = 0; i <= optionIndex; i++) await page.keyboard.press('ArrowDown')
      await page.keyboard.press('Enter') // 焦点在选项上：原生 Enter 触发点击选中
      await search.waitFor({ state: 'hidden', timeout: 3_000 }) // 面板关闭
      return
    } catch {
      // 面板被 outside-scroll 提前关闭 / 选项未加载完：Esc 收尾后重试
      await page.keyboard.press('Escape').catch(() => {})
      await page.waitForTimeout(300)
    }
  }
  throw new Error(`openAndPick(${testid}) 重试 4 次仍未选中`)
}

/** 合并艺人：自定义合并 → 新记录出现 → 取消合并恢复（全程隔离数据） */
test.describe('合并艺人', () => {
  test('合并 → 取消合并往返', async ({ page }) => {
    test.setTimeout(120_000)
    await page.locator('[data-testid="nav-settings"]').click({ timeout: 10_000 })
    log('1 nav-settings')

    // 合并前记录集合（种子库可能自带历史合并）。别名列表异步加载，先等它就绪。
    // ⚠️ 定位器收窄到 main：管理弹窗 Teleport 到 body，也有同名列表行，不收窄会双计
    await page.waitForTimeout(1200)
    const records = page.locator('main ul li').filter({ hasText: '→' })
    const before = await page.locator('main ul li .line-through').allTextContents()
    log(`before records: ${JSON.stringify(before)}`)

    // 保留第 1 个选项的艺人，并入第 2 个；面板可能被 outside-scroll 提前关闭，重试至按钮可用
    for (let attempt = 1; attempt <= 3; attempt++) {
      await openAndPick(page, 'merge-keep', 0)
      await openAndPick(page, 'merge-absorb', 1)
      if (await page.locator('[data-testid="merge-apply"]').isEnabled().catch(() => false)) break
      log(`attempt ${attempt}: 合并按钮仍禁用，重选`)
      await page.keyboard.press('Escape').catch(() => {})
    }
    log('2 both picked')
    await expect(page.locator('[data-testid="merge-apply"]')).toBeEnabled({ timeout: 5_000 })
    await page.locator('[data-testid="merge-apply"]').click()
    log('4 applied')

    // 合并（真实 SQL 迁移，发生在隔离副本上）→ 新增一条记录
    await expect(records).toHaveCount(before.length + 1, { timeout: 15_000 })

    // 新增行 = 旧名不在合并前集合里的那条
    let absorbName = ''
    for (let i = 0; i < (await records.count()); i++) {
      const t = ((await records.nth(i).textContent()) ?? '').replace(/\s+/g, ' ')
      const alias = t.split('→')[0].trim()
      if (!before.includes(alias)) absorbName = alias
    }
    expect(absorbName).toBeTruthy()
    log(`5 new record: ${absorbName}`)

    // 打开管理弹窗 → 对新增行取消合并
    await page.locator('[data-testid="merged-manage"]').click({ timeout: 10_000 })
    const dialog = page.locator('[data-testid="merged-dialog"]')
    await expect(dialog).toBeVisible({ timeout: 10_000 })
    await expect(dialog.getByText(absorbName)).toBeVisible({ timeout: 10_000 })
    await dialog
      .locator('li')
      .filter({ hasText: absorbName })
      .locator('[data-testid="unmerge-btn"]')
      .click({ timeout: 10_000 })
    log('6 unmerged')
    await page.locator('[data-testid="confirm-accept"]').click({ timeout: 10_000 })

    // 记录回到合并前状态
    await expect(records).toHaveCount(before.length, { timeout: 30_000 })
    log('7 done')
  })
})
