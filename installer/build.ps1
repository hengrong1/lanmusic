# LanMusic Windows 安装包一键打包（Inno Setup）
# 用法：
#   powershell -ExecutionPolicy Bypass -File installer\build.ps1              # 构建产物 + 打包
#   powershell -ExecutionPolicy Bypass -File installer\build.ps1 -SkipBuild   # 跳过构建，只用现有产物打包
# 输出：installer\output\LanMusic_<版本>_x64-setup.exe

param([switch]$SkipBuild)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot

# 定位 Inno Setup 编译器（ISCC.exe）：环境变量 → PATH → 常见安装位置
function Find-Iscc {
    if ($env:ISCC -and (Test-Path $env:ISCC)) { return $env:ISCC }
    $cmd = Get-Command iscc.exe -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    $candidates = @(
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
        "D:\InnoSetup6\ISCC.exe"
    )
    foreach ($p in $candidates) {
        if ($p -and (Test-Path $p)) { return $p }
    }
    # 安装时写入的卸载信息里带安装目录（HKLM/HKCU 都可能）
    $keys = @(
        "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Inno Setup 6_is1",
        "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Inno Setup 6_is1"
    )
    foreach ($k in $keys) {
        if (Test-Path $k) {
            $loc = (Get-ItemProperty $k -ErrorAction SilentlyContinue).InstallLocation
            if ($loc -and (Test-Path (Join-Path $loc "ISCC.exe"))) { return (Join-Path $loc "ISCC.exe") }
        }
    }
    throw "未找到 Inno Setup 编译器（ISCC.exe）。请安装 Inno Setup 6，或设置环境变量 ISCC 指向 ISCC.exe。"
}

$Iscc = Find-Iscc

if (-not $SkipBuild) {
    Set-Location $Root
    # bundle.targets 为空：tauri build 只产出 exe，打包统一走 Inno Setup
    npm run tauri:build
    if ($LASTEXITCODE -ne 0) { throw "tauri build 失败" }
}

$Exe = Join-Path $Root "src-tauri\target\release\lanmusic.exe"
if (-not (Test-Path $Exe)) {
    throw "未找到编译产物：$Exe（请先运行 tauri build）"
}

# 三段版本号（如 0.5.5）：传给 .iss 作为**安装包文件名**的版本。
# Windows 文件版本是四段（0.5.5.0），若拿它当文件名，应用内更新按三段拼的资产地址
# 会 HEAD 404（见 src-tauri/src/updater.rs::asset_urls）。所以以 tauri.conf.json 的
# version 为唯一来源，与 tag 名、App 内比较用的版本三者保持一致。
$Ver = (Get-Content (Join-Path $Root "src-tauri\tauri.conf.json") | ConvertFrom-Json).version
if (-not $Ver) { throw "未能从 src-tauri\tauri.conf.json 读到 version" }

& $Iscc "/DMyAppVersionShort=$Ver" (Join-Path $PSScriptRoot "LanMusic.iss")
if ($LASTEXITCODE -ne 0) { throw "ISCC 打包失败" }

$Installer = Get-ChildItem (Join-Path $Root "installer\output\*.exe") |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1

# 生成 SHA-256 校验文件（应用内更新下载后校验用，见 src-tauri/src/updater.rs）
$hash = (Get-FileHash $Installer.FullName -Algorithm SHA256).Hash.ToLower()
$shaFile = "$($Installer.FullName).sha256"
Set-Content -Path $shaFile -Value $hash -NoNewline -Encoding ascii

Write-Host ""
Write-Host "安装包已生成：$($Installer.FullName)  $([math]::Round($Installer.Length / 1MB, 2)) MB" -ForegroundColor Green
Write-Host "校验文件已生成：$shaFile" -ForegroundColor Green
