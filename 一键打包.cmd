@echo off
setlocal EnableExtensions
cd /d "%~dp0"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

where link.exe >nul 2>&1
if errorlevel 1 (
  for /f "usebackq tokens=*" %%i in (`"%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2^>nul`) do (
    call "%%i\VC\Auxiliary\Build\vcvars64.bat" >nul
  )
)

echo === disk-janitor 一键打包 ===
taskkill /F /IM disk-janitor.exe >nul 2>&1
timeout /t 1 /nobreak >nul

cargo build --release
if errorlevel 1 (
  echo [失败] 编译出错（若拒绝访问，请先关闭正在运行的磁盘管家）
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
echo [版本] %VER% → versionCode=%CODE%

if not exist "release" mkdir "release"
if not exist "deploy" mkdir "deploy"
copy /Y "target\release\disk-janitor.exe" "release\DiskJanitor-%VER%.exe" >nul
copy /Y "target\release\disk-janitor.exe" "release\disk-janitor.exe" >nul

set "SHA="
where certutil >nul 2>&1
if not errorlevel 1 (
  for /f "skip=1 tokens=*" %%h in ('certutil -hashfile "release\DiskJanitor-%VER%.exe" SHA256 ^| findstr /R /V ":"') do (
    if not defined SHA set "SHA=%%h"
  )
)
set "SHA=%SHA: =%"
set "SHA=%SHA:"=%"

> "deploy\app-update.json" (
  echo {
  echo   "versionCode": %CODE%,
  echo   "versionName": "%VER%",
  echo   "desktopUrl": "http://111.229.202.251:8687/disk-janitor/releases/DiskJanitor-%VER%.exe",
  if defined SHA (
    echo   "sha256": "%SHA%",
  ) else (
    echo   "sha256": "",
  )
  echo   "changelog": "disk-janitor %VER% #%CODE%",
  echo   "displayName": "Disk Janitor",
  echo   "enabled": true
  echo }
)
copy /Y "deploy\app-update.json" "release\app-update.json" >nul

echo.
echo [完成] release\DiskJanitor-%VER%.exe
echo        deploy\app-update.json  ^(versionCode=%CODE% sha256=%SHA%^)
echo        上传 exe 后把 app-update 写到 jiaoben / 静态目录
echo.
dir /b "release\*.exe"
exit /b 0
