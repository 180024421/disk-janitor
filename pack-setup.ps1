# Build Dashuai Cleaner Setup (大帅清理器-Setup-x.y.z.exe)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
if (Test-Path (Join-Path $root "Cargo.toml")) {
  # script in repo root
} else {
  $root = Split-Path -Parent $root
}
Set-Location $root
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"

$product = -join @([char]0x5927, [char]0x5E05, [char]0x6E05, [char]0x7406, [char]0x5668) # 大帅清理器

function Ensure-VcVars {
  $link = Get-Command link.exe -ErrorAction SilentlyContinue
  if ($link) { return }
  $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
  if (-not (Test-Path $vswhere)) { return }
  $vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
  if (-not $vs) { return }
  $bat = Join-Path $vs "VC\Auxiliary\Build\vcvars64.bat"
  if (-not (Test-Path $bat)) { return }
  cmd /c "`"$bat`" >nul && set" | ForEach-Object {
    if ($_ -match "^(.*?)=(.*)$") {
      [System.Environment]::SetEnvironmentVariable($matches[1], $matches[2])
    }
  }
}

Write-Host "=== $product Setup pack ==="
Ensure-VcVars
Get-Process -Name "disk-janitor","DashuaiCleaner" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.ProcessName -like "*$product*" } | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 1

Write-Host "[1/4] portable build..."
& cmd /c "pack.cmd"
if ($LASTEXITCODE -ne 0) { throw "pack.cmd failed" }

$verLine = Get-Content .\Cargo.toml | Where-Object { $_ -match '^version\s*=' } | Select-Object -First 1
if ($verLine -notmatch '"([^"]+)"') { throw "cannot parse version" }
$ver = $Matches[1]
$codeParts = $ver.Split(".")
$code = [int]$codeParts[0] * 10000 + [int]$codeParts[1] * 100 + [int]$codeParts[2]

Write-Host "[2/4] payload.zip..."
$packDir = Join-Path $root "release\pack"
if (Test-Path $packDir) { Remove-Item -Recurse -Force $packDir }
New-Item -ItemType Directory -Path (Join-Path $packDir "resources") -Force | Out-Null
Copy-Item -Force "release\disk-janitor.exe" (Join-Path $packDir "disk-janitor.exe")
Copy-Item -Force "release\disk-janitor.exe" (Join-Path $packDir "DashuaiCleaner.exe")
Copy-Item -Force "release\disk-janitor.exe" (Join-Path $packDir "$product.exe")
if (Test-Path "resources") {
  Copy-Item -Force "resources\*.png" (Join-Path $packDir "resources\") -ErrorAction SilentlyContinue
}
if (Test-Path "release\resources") {
  Copy-Item -Force "release\resources\*.png" (Join-Path $packDir "resources\") -ErrorAction SilentlyContinue
}

$payload = Join-Path $root "installer\payload.zip"
if (Test-Path $payload) { Remove-Item -Force $payload }
Compress-Archive -Path (Join-Path $packDir "*") -DestinationPath $payload -Force

Write-Host "[3/4] compile setup..."
& cargo build --release --bin dashuai-cleaner-setup --features setup
if ($LASTEXITCODE -ne 0) { throw "setup build failed" }

Write-Host "[4/4] copy outputs..."
$setupSrc = "target\release\dashuai-cleaner-setup.exe"
$setupCn = Join-Path $root "release\$product-Setup-$ver.exe"
$setupEn = Join-Path $root "release\DashuaiCleaner-Setup-$ver.exe"
Copy-Item -Force $setupSrc $setupCn
Copy-Item -Force $setupSrc $setupEn

$sha = (Get-FileHash -Algorithm SHA256 -Path $setupCn).Hash.ToLowerInvariant()
$meta = [ordered]@{
  productName = $product
  versionCode = $code
  versionName = $ver
  setupFile   = "$product-Setup-$ver.exe"
  sha256      = $sha
}
$utf8 = New-Object System.Text.UTF8Encoding $false
$metaJson = ($meta | ConvertTo-Json -Depth 5) + "`n"
[System.IO.File]::WriteAllText((Join-Path $root "deploy\setup-release.json"), $metaJson, $utf8)
Copy-Item -Force (Join-Path $root "deploy\setup-release.json") (Join-Path $root "release\setup-release.json")

Write-Host ""
Write-Host "[OK] portable: release\DiskJanitor-$ver.exe"
Write-Host "[OK] setup:    $setupCn"
Write-Host "[OK] setup:    $setupEn"
Write-Host "Hot-update still uses deploy\app-update.json (portable exe)."
Get-ChildItem "release\*Setup*.exe" | ForEach-Object { $_.FullName }
exit 0
