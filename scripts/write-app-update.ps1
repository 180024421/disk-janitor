# Writes deploy/app-update.json for portable hot-update (UTF-8).
param(
  [Parameter(Mandatory = $true)][string]$Version,
  [Parameter(Mandatory = $true)][int]$VersionCode,
  [string]$DownloadUrl
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path (Join-Path $root "Cargo.toml"))) {
  $root = $PSScriptRoot
}
Set-Location $root

$exe = Join-Path $root "release\DiskJanitor-$Version.exe"
if (-not (Test-Path $exe)) { throw "missing $exe" }
if ((Get-Item $exe).Length -gt 300MB) { throw "update payload exceeds 300 MB" }

$sha = (Get-FileHash -Algorithm SHA256 -Path $exe).Hash.ToLowerInvariant()
if ($sha -notmatch '^[0-9a-f]{64}$') { throw "invalid SHA256 generated for $exe" }
$defaultUrl = "https://jiaoben.lidashuai.top/api/files/app-update/disk-janitor/DiskJanitor-$Version.exe"
if ([string]::IsNullOrWhiteSpace($DownloadUrl)) { $DownloadUrl = $defaultUrl }
$parsedUrl = $null
if (-not [Uri]::TryCreate($DownloadUrl, [UriKind]::Absolute, [ref]$parsedUrl) -or
    $parsedUrl.Scheme -ne [Uri]::UriSchemeHttps -or
    [string]::IsNullOrWhiteSpace($parsedUrl.Host)) {
  throw "desktopUrl must be an absolute HTTPS URL"
}
$displayName = -join @([char]0x5927, [char]0x5E05, [char]0x6E05, [char]0x7406, [char]0x5668) # 大帅清理器
$obj = [ordered]@{
  versionCode = $VersionCode
  versionName = $Version
  desktopUrl  = $parsedUrl.AbsoluteUri
  sha256      = $sha
  signature   = $null
  signatureMode = "staged"
  changelog   = "$displayName $Version #$VersionCode"
  displayName = $displayName
  enabled     = $true
}
$json = ($obj | ConvertTo-Json -Depth 5) + "`n"
$dir = Join-Path $root "deploy"
if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
$utf8 = New-Object System.Text.UTF8Encoding $false
$manifestPath = Join-Path $dir "app-update.json"
[System.IO.File]::WriteAllText($manifestPath, $json, $utf8)

# ---- 清单 Ed25519 签名：私钥不在仓库，由打包机持有 ----
# 私钥位置：--SignKeyFile 参数 > $env:DJ_MANIFEST_SIGN_KEY > %LOCALAPPDATA%\disk-janitor-release\manifest-sign.key
$signer = Join-Path $root "target\release\dj-manifest-sign.exe"
if (-not (Test-Path $signer)) {
  cargo build --release --bin dj-manifest-sign
  if ($LASTEXITCODE -ne 0) { throw "build dj-manifest-sign failed" }
}
& $signer sign $manifestPath
if ($LASTEXITCODE -ne 0) { throw "manifest signing failed; unsigned manifests are rejected by clients >= 0.9.8" }
& $signer verify $manifestPath
if ($LASTEXITCODE -ne 0) { throw "manifest signature verify failed" }

Copy-Item -Force $manifestPath (Join-Path $root "release\app-update.json")
Write-Host "[OK] app-update.json sha256=$sha signed displayName=$displayName"
