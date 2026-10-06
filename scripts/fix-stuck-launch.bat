@echo off
setlocal
rem ============================================================
rem  LanMusic stuck-launch fixer
rem  Use when LanMusic cannot start (stuck on loading / not
rem  clickable). Root cause class: leftover zombie processes
rem  (Playwright chrome-headless-shell.exe + its node.exe driver)
rem  holding kernel objects that block the new lanmusic.exe.
rem
rem  Order matters: shell -> node driver -> lanmusic zombie.
rem  After drivers die, blocked threads unwind and zombie
rem  process objects disappear by themselves.
rem ============================================================

echo [1/3] Killing leftover chrome-headless-shell.exe ...
taskkill /F /IM chrome-headless-shell.exe /T 2>nul

echo [2/3] Killing leftover WorkBuddy automation node drivers ...
powershell -NoProfile -Command "Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'node.exe' -and $_.CommandLine -like '*workbuddy-tmp*' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }"

echo [3/3] Closing all lanmusic.exe instances (they cannot be
echo       healthy when the app refuses to start) ...
taskkill /F /IM lanmusic.exe /T 2>nul

echo.
echo Waiting 3 seconds for process objects to unwind ...
timeout /t 3 /nobreak >nul

echo Relaunching LanMusic ...
start "" "%LocalAppData%\Programs\LanMusic\lanmusic.exe"

echo.
echo If it STILL hangs after this script, reboot Windows --
echo that clears any remaining stuck kernel objects for sure.
echo (Check the reason afterwards in Event Viewer ^> Application,
echo  look for AppHangXProcB1 mentioning lanmusic.exe and the
echo  blocking process name in field P6.)
echo.
pause
