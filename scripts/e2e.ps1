# LanMusic E2E 一键运行（隔离数据目录，可执行写操作）：
#   1. 清空并从 dev 数据目录播种 E2E 专属数据（com.lanmusic.desktop.e2e）
#   2. 以 e2e 配置起 dev 应用（调试端口 9222）→ 等 CDP 就绪
#   3. 跑 Playwright 全量用例（写操作只发生在 E2E 副本上）
#   4. 收尾关闭进程树
# 用法：
#   powershell -ExecutionPolicy Bypass -File scripts\e2e.ps1                # 全部用例
#   powershell -ExecutionPolicy Bypass -File scripts\e2e.ps1 -- --grep 播放  # 透传给 playwright
param([Parameter(ValueFromRemainingArguments = $true)]$Rest)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot

$DevData = "$env:APPDATA\com.lanmusic.desktop.dev"
$E2eData = "$env:APPDATA\com.lanmusic.desktop.e2e"

# 应用有单实例守卫：先关掉已在跑的实例
Get-Process lanmusic -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue

# 播种：每次运行都重置为 dev 库的副本（写操作可重复、不污染真实数据）
if (-not (Test-Path "$DevData\library.db")) {
    throw "未找到 dev 数据库：$DevData\library.db（先用 pnpm start 建一次库）"
}
Remove-Item -Recurse -Force $E2eData -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $E2eData | Out-Null
Copy-Item "$DevData\library.db" "$E2eData\library.db" -Force
Copy-Item "$DevData\library.db-shm" "$E2eData\library.db-shm" -Force -ErrorAction SilentlyContinue
Copy-Item "$DevData\library.db-wal" "$E2eData\library.db-wal" -Force -ErrorAction SilentlyContinue
if (Test-Path "$DevData\covers") {
    Copy-Item "$DevData\covers" "$E2eData\covers" -Recurse -Force
}
Write-Host "E2E 数据目录已播种：$E2eData" -ForegroundColor DarkCyan

# WebView2 调试端口 + 禁用后台节流/遮挡挂起（自动化期间窗口被遮挡也会持续渲染，
# 否则 WebView2 挂起后页面变空白，所有用例超时）
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222 --disable-background-timer-throttling --disable-backgrounding-occluded-windows --disable-renderer-backgrounding"

# 起 dev 应用：用 E2E 专属配置（identifier 不同 → 数据目录隔离）
$dev = Start-Process -FilePath "cmd.exe" `
    -ArgumentList "/c", "pnpm exec tauri dev --config src-tauri/tauri.e2e.conf.json" `
    -WorkingDirectory $Root -PassThru -WindowStyle Hidden

function Test-CdpReady {
    try {
        Invoke-WebRequest -Uri "http://127.0.0.1:9222/json/version" -UseBasicParsing `
            -TimeoutSec 2 | Out-Null
        return $true
    } catch { return $false }
}

try {
    $deadline = (Get-Date).AddSeconds(240)
    while (-not (Test-CdpReady)) {
        if ((Get-Date) -gt $deadline) { throw "240 秒内未等到 CDP 端口（9222），应用可能启动失败" }
        Start-Sleep -Milliseconds 500
    }
    Write-Host "CDP 就绪，开始跑 E2E 用例..." -ForegroundColor Cyan
    Set-Location $Root
    pnpm exec playwright test -c tests/e2e/playwright.config.ts @Rest
    if ($LASTEXITCODE -ne 0) { throw "E2E 用例失败（exit $LASTEXITCODE）" }
} finally {
    # 收尾：整个 dev 进程树（cmd → cargo → lanmusic）连带关掉
    try { taskkill /PID $($dev.Id) /T /F 2>$null | Out-Null } catch {}
    Get-Process lanmusic -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}
