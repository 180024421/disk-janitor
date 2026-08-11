@echo off
REM Portable build for Dashuai Cleaner (大帅清理器). ASCII-only script body for cmd.exe.
setlocal EnableExtensions
cd /d "%~dp0"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

where link.exe >nul 2>&1
if errorlevel 1 (
  for /f "usebackq tokens=*" %%i in (`"%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2^>nul`) do (
    call "%%i\VC\Auxiliary\Build\vcvars64.bat" >nul
  )
)

echo === Dashuai Cleaner portable pack ===
taskkill /F /IM disk-janitor.exe >nul 2>&1
taskkill /F /IM DashuaiCleaner.exe >nul 2>&1
powershell -NoProfile -Command "Get-Process | Where-Object { $_.ProcessName -like '*清理*' -or $_.MainWindowTitle -like '*大帅清理器*' } | Stop-Process -Force -ErrorAction SilentlyContinue" >nul 2>&1
timeout /t 1 /nobreak >nul

cargo build --release
if errorlevel 1 (
  echo [FAIL] cargo build failed
  exit /b 1
)

for /f "tokens=2 delims==" %%v in ('findstr /b "version" Cargo.toml') do (
  set "VER=%%~v"
  goto :have_ver
)
:have_ver
set "VER=%VER: =%"
set "VER=%VER:"=%"

set "CODE="
for /f "tokens=1-3 delims=." %%a in ("%VER%") do (
  set /a CODE=%%a*10000+%%b*100+%%c
)
if "%CODE%"=="" set "CODE=500"
echo [version] %VER% -^> versionCode=%CODE%

if not exist "release" mkdir "release"
if not exist "deploy" mkdir "deploy"
copy /Y "target\release\disk-janitor.exe" "release\DiskJanitor-%VER%.exe" >nul
copy /Y "target\release\disk-janitor.exe" "release\disk-janitor.exe" >nul
copy /Y "target\release\disk-janitor.exe" "release\DashuaiCleaner.exe" >nul
powershell -NoProfile -Command ^
  "$cn = Join-Path 'release' ([char]0x5927+[char]0x5E05+[char]0x6E05+[char]0x7406+[char]0x5668+'.exe'); Copy-Item -Force 'release\disk-janitor.exe' $cn"
if exist "resources" (
  if not exist "release\resources" mkdir "release\resources"
  copy /Y "resources\*.png" "release\resources\" >nul
)

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\write-app-update.ps1" -Version "%VER%" -VersionCode %CODE%
if errorlevel 1 exit /b 1

echo.
echo [OK] release\DiskJanitor-%VER%.exe
echo      deploy\app-update.json
dir /b "release\*.exe"
exit /b 0
