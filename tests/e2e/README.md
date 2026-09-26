# LanMusic E2E 测试（WebView2 CDP 直连）

原理：通过 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` 打开
WebView2 调试端口，Playwright `connectOverCDP` 连入**真实应用窗口**，像测网页一样驱动
DOM——Tauri invoke 走真实后端（SQL 真写库），不是 mock。

## 运行

```powershell
powershell -ExecutionPolicy Bypass -File scripts\e2e.ps1          # 一键：全部 spec
powershell -ExecutionPolicy Bypass -File scripts\e2e.ps1 -Only merge    # 只跑某个 spec
```

每个 **spec 文件**都使用「全新应用实例」：重置 E2E 数据目录（从 dev 库播种）+ 连
WebView2 profile（localStorage 等 UI 状态一并重置）→ 起 `tauri dev`（e2e identifier，
独立于真实曲库）→ 等 CDP 就绪 → 跑用例 → 收尾进程树。写操作（歌单/合并/移除）不污染
真实曲库，且每次运行完全确定性。

覆盖范围（19 条，全绿）：曲库视图切换/专辑详情、合并→取消合并往返、播放/暂停/切歌/
喜欢/队列、歌单全流程（含右键菜单）、搜索（下拉+结果视图）、已移除歌曲/还原、设置
（主题/语言/桌面歌词窗口/听歌统计+诊断）、检查更新。

## 踩平的坑位（写新用例前必读）

1. **BaseSelect**：合成 click 会触发文档级监听把刚打开的面板关掉 → 开面板与选择之间
   不能有 await；面板打开后焦点在面板搜索框，后续按键必须用 `page.keyboard`
   （`locator.press` 会把焦点拉回触发器，按键全部落空）；选项异步加载，先等
   `[data-option]` 可见。完整示例见 merge.spec 的 `openAndPick` + 重试逻辑。
2. **主窗口识别**：桌面歌词/托盘小窗与主窗口同源（URL 相同），须用特征元素
   （`#search-input`）识别主窗口；Playwright 失败快照也会拿错页面（常显示托盘窗），
   以用例内日志/截图为准。
3. **tooltip 覆盖小按钮**：带 `v-tooltip` 的小按钮（如新建歌单 +），悬停即出 tooltip
   盖住按钮，真实点击会打到 tooltip 上 → 用 `locator.evaluate(el => el.click())` 绕开。
4. **BaseInput 的 testid 落在根 div**：fill/定位需写 `[data-testid="x"] input`。
5. **WebView2 节流**：窗口被遮挡会挂起渲染/定时器（view-guard 日志 op=0、页面空白），
   运行器已加 `--disable-background-timer-throttling` 等参数 + 禁用面板遮挡兜底。
6. **上下文菜单文案**：右键菜单「加入歌单」的 key 与设置区「添加到歌单」不同，断言前
   先 grep 语言包确认实际文案。
7. **失败快照不可信**：error-context.md 常抓到托盘窗页面，定位问题以 `console.log`
   步骤日志 + 手动截图（`page.screenshot`）为准。
