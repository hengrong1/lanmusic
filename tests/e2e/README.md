# LanMusic E2E 测试(WebView2 CDP 直连)

原理:通过 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` 打开
WebView2 调试端口,Playwright `connectOverCDP` 连入**真实应用窗口**,像测网页一样驱动
DOM——Tauri invoke 走真实后端(SQL 真写库),不是 mock。

## 运行

```powershell
powershell -ExecutionPolicy Bypass -File scripts/e2e.ps1          # 一键:播种数据 + 起应用 + 跑用例 + 收尾
pnpm test:e2e                                                     # 应用已在运行时直接跑
```

- **数据隔离**:E2E 用独立数据目录(`com.lanmusic.desktop.e2e`,identifier 由
  `src-tauri/tauri.e2e.conf.json` 覆盖),每次运行前从 dev 库重新播种——
  用例中的写操作(歌单/合并/移除)不污染真实曲库。
- **留证**:每个用例结束自动截图到 `test-results/e2e/`。

## 已知问题(当前 fixme 的用例)

view-guard 日志(`[view-guard] 视图 xx 未正常渲染 | op=0`)证实:**视图入场过渡
偶尔卡在 opacity 0**(窗口被遮挡/失焦时 WebView2 节流 rAF,transition 永不结束),
该视图上的元素全部不可交互,用例整体超时。这是应用级 bug(自动化的意外收获),
修复后把各 spec 里的 `test.fixme` 翻回 `test` 即可解封以下用例:

- player.spec(播放/暂停/切歌/喜欢/队列)
- playlist.spec(歌单全流程)
- removed.spec(移除/还原)
- search.spec(搜索)
- settings.spec(主题/语言/桌面歌词/统计诊断)
- updater.spec(检查更新)
- smoke.spec 的搜索/设置/弹窗 3 条

## BaseSelect 自动化坑位(merge.spec 有完整示例)

1. 合成 click 会触发文档级监听把刚打开的面板关掉 → **开面板与选择之间不能有 await**;
2. 面板打开后焦点在面板搜索框,后续按键必须用 `page.keyboard`(locator.press 会把
   焦点拉回触发器,按键全部落空);
3. 选项列表异步加载,先等 `[data-option]` 可见再操作。
