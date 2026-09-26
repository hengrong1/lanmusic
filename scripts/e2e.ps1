# LanMusic E2E 一键运行：带 CDP 调试端口启动 dev 应用 → 等 CDP 就绪 → 跑 Playwright → 关闭应用
# 用法：
#   powershell -ExecutionPolicy Bypass -File scripts\e2e.ps1                # 全部用例
#   powershell -ExecutionPolicy Bypass -File scripts\e2e.ps1 -- --grep 搜索  # 透传给 playwright
param([Parameter(ValueFromRemainingArguments = $true)]$Args)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot

# 应用有单实例守卫：先关掉已在跑的实例，否则新实例会把启动让给旧实例（无调试端口）
Get-Process lanmusic -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue

# WebView2 调试端口（Playwright 经 CDP 连入真实窗口）
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222"

# 起 dev 应用（vite + tauri dev，子进程树）
$dev = Start-Process -FilePath "cmd.exe" -ArgumentList "/c", "pnpm start" `
    -WorkingDirectory $Root -PassThru -WindowStyle Hidden

function Test-CdpReady {
    try {
        Invoke-WebRequest -Uri "http://127.0.0.1:9222/json/version" -UseBasicParsing `
            -TimeoutSec 2 | Out-Null
        return $true
    } catch { return $false }
}

try {
    $deadline = (Get-Date).AddSeconds(180)
    while (-not (Test-CdpReady)) {
        if ((Get-Date) -gt $deadline) { throw "180 秒内未等到 CDP 端口（9222），应用可能启动失败" }
        Start-Sleep -Milliseconds 500
    }
    Write-Host "CDP 就绪，开始跑 E2E 用例..." -ForegroundColor Cyan
    Set-Location $Root
    pnpm exec playwright test -c tests/e2e/playwright.config.ts @Args
    if ($LASTEXITCODE -ne 0) { throw "E2E 用例失败（exit $LASTEXITCODE）" }
} finally {
    # 收尾：整个 dev 进程树（cmd → cargo → lanmusic）连带关掉
    Stop-Process -Id $dev.Id -Force -ErrorAction SilentlyContinue
    cmd /c "taskkill /PID $($dev.Id) /T /F" 2>$null | Out-Null
    Get-Process lanmusic -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}
