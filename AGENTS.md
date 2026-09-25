# lanmusic 项目约定

## 「做包」= 推标签走 CI 发布

用户说「做包」时，执行以下完整流程（默认版本号 +1 patch，如 0.5.23 → 0.5.24；用户另有说明时按其指定）：

1. **改版本号**——三处同步，唯一来源是 `src-tauri/tauri.conf.json` 的 `version`（三段）：
   - `src-tauri/tauri.conf.json`
   - `package.json`
   - `src-tauri/Cargo.toml`（改完跑一次 `cargo update -p lanmusic --offline` 或任一 cargo 命令刷新 Cargo.lock）
2. **提交代码**：工作区先清理/提交所有待发布改动，再提交版本号：`chore(release): vX.Y.Z`
3. **创建标签**：`git tag vX.Y.Z`（与版本严格一致；安装包文件名用三段版本，四段会让应用内更新 404）
4. **推送**：推到 **`github` 远端**（GitHub Actions 的 build.yml 监听 `v*` 标签，构建 Windows Inno Setup 安装包 + macOS dmg，创建 Release 草稿）：
   `git push github main --tags`（或 `git push github main && git push github vX.Y.Z`）

注意：
- 本地 Inno Setup 打包（`installer/build.ps1`）仅用于本地试装，不算「做包」；CI 的 Windows 打包与其同源
- `installer/LanMusic.iss` 的 AppId 是固定 GUID，不可改动
- 发布前建议先过 `pnpm verify`（typecheck + cargo test + clippy）
- 本机 Rust 工具链依赖 Windows SDK，位于 `F:\Dev\Windows Kits\10`（注册表 KitsRoot10 与
  WOW6432Node 下 `Microsoft SDKs\Windows\v10.0\InstallationFolder` 均指向此处；`cargo test`
  需要先关闭正在运行的 LanMusic 应用，否则 lanmusic.exe 被占用无法链接）
