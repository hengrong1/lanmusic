# LanMusic E2E 一键运行（隔离数据目录，可执行写操作）：
#   每个 spec 文件都用「全新播种的应用实例」运行——单文件内状态自洽，
#   文件间互不污染，也规避长会话下 WebView2 渲染退化（view-guard op=0）问题。
# 用法：
#   powershell -ExecutionPolicy Bypass -File scripts\e2e.ps1                    # 全部 spec
#   powershell -ExecutionPolicy Bypass -File scripts\e2e.ps1 -Grep 合并          # 过滤用例
#   powershell -ExecutionPolicy Bypass -File scripts\e2e.ps1 -Only merge        # 只跑 merge.spec
param(
    [string]$Grep,
    [string]$Only
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot

$DevData = "$env:APPDATA\com.lanmusic.desktop.dev"
$E2eData = "$env:APPDATA\com.lanmusic.desktop.e2e"

# WebView2 调试端口 + 禁用后台节流/遮挡挂起（自动化期间窗口被遮挡也持续渲染）
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS =
    "--remote-debugging-port=9222 --disable-background-timer-throttling --disable-backgrounding-occluded-windows --disable-renderer-backgrounding"

function Start-E2eApp {
    # 应用有单实例守卫：先关掉已在跑的实例
    Get-Process lanmusic -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    # 播种：重置为 dev 库的副本（写操作可重复、不污染真实数据）
    if (-not (Test-Path "$DevData\library.db")) {
        throw "未找到 dev 数据库：$DevData\library.db（先用 pnpm start 建一次库）"
    }
    Remove-Item -Recurse -Force $E2eData -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force -Path $E2eData | Out-Null
    # 连 WebView2 profile（localStorage 等持久 UI 状态）一起重置，保证完全确定性
    Remove-Item -Recurse -Force "$env:LOCALAPPDATA\com.lanmusic.desktop.e2e" -ErrorAction SilentlyContinue
    Copy-Item "$DevData\library.db" "$E2eData\library.db" -Force
    Copy-Item "$DevData\library.db-shm" "$E2eData\library.db-shm" -Force -ErrorAction SilentlyContinue
    Copy-Item "$DevData\library.db-wal" "$E2eData\library.db-wal" -Force -ErrorAction SilentlyContinue
    if (Test-Path "$DevData\covers") {
        Copy-Item "$DevData\covers" "$E2eData\covers" -Recurse -Force
    }
    return Start-Process -FilePath "cmd.exe" `
        -ArgumentList "/c", "pnpm exec tauri dev --config src-tauri/tauri.e2e.conf.json" `
        -WorkingDirectory $Root -PassThru -WindowStyle Hidden
}

function Test-CdpReady {
    try {
        Invoke-WebRequest -Uri "http://127.0.0.1:9222/json/version" -UseBasicParsing `
            -TimeoutSec 2 | Out-Null
        return $true
    } catch { return $false }
}

function Wait-Cdp {
    param([int]$Seconds = 240)
    $deadline = (Get-Date).AddSeconds($Seconds)
    while (-not (Test-CdpReady)) {
        if ((Get-Date) -gt $deadline) { throw "$Seconds 秒内未等到 CDP 端口（9222），应用可能启动失败" }
        Start-Sleep -Milliseconds 500
    }
}

function Stop-E2eApp {
    param([int]$DevPid)
    try { taskkill /PID $DevPid /T /F 2>$null | Out-Null } catch {}
    Get-Process lanmusic -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}

# spec 清单（-Only 过滤文件名）
$specs = Get-ChildItem "$Root\tests\e2e" -Filter *.spec.ts | ForEach-Object { $_.Name }
if ($Only) { $specs = $specs | Where-Object { $_ -like "*$Only*" } }
if (-not $specs) { throw "没有匹配的 spec 文件" }

$failed = @()
foreach ($spec in $specs) {
    Write-Host ""
    Write-Host "===== $spec（全新实例） =====" -ForegroundColor Cyan
    $dev = Start-E2eApp
    try {
        Wait-Cdp
        $filter = @()
        if ($Grep) { $filter += @("--grep", $Grep) }
        pnpm exec playwright test -c tests/e2e/playwright.config.ts @filter $spec
        if ($LASTEXITCODE -ne 0) { $failed += $spec; Write-Host "FAIL $spec" -ForegroundColor Red }
    } catch {
        $failed += $spec
        Write-Host "FAIL $spec ：$_" -ForegroundColor Red
    } finally {
        Stop-E2eApp -DevPid $dev.Id
    }
}

Write-Host ""
if ($failed.Count) {
    Write-Host "失败的 spec：$($failed -join ', ')" -ForegroundColor Red
    exit 1
}
Write-Host "全部 spec 通过" -ForegroundColor Green
