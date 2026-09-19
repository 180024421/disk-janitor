# 给构建产物做 Authenticode 签名。
# 证书来源：环境变量 DISKJANITOR_CERT_SHA1 指定的证书，否则取当前用户「代码签名」证书的最新一张。
# 找不到证书 / signtool 时只告警、不中断打包（保证没配证书的机器也能出包）。
param(
  [Parameter(Mandatory = $true)][string[]]$Path
)

$ErrorActionPreference = "Continue"

function Find-SignTool {
  $cmd = Get-Command signtool.exe -ErrorAction SilentlyContinue
  if ($cmd) { return $cmd.Source }
  $kits = "C:\Program Files (x86)\Windows Kits\10\bin"
  if (Test-Path $kits) {
    $hit = Get-ChildItem $kits -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
      Where-Object { $_.FullName -match '\\x64\\' } |
      Sort-Object FullName -Descending | Select-Object -First 1
    if ($hit) { return $hit.FullName }
  }
  return $null
}

$cert = $null
if ($env:DISKJANITOR_CERT_SHA1) {
  $cert = Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert -ErrorAction SilentlyContinue |
    Where-Object { $_.Thumbprint -eq $env:DISKJANITOR_CERT_SHA1 } |
    Select-Object -First 1
}
if (-not $cert) {
  $cert = Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert -ErrorAction SilentlyContinue |
    Sort-Object NotAfter -Descending | Select-Object -First 1
}
if (-not $cert) {
  Write-Warning "[sign] 未找到代码签名证书，跳过签名（导入 .pfx 后可自动签）"
  exit 0
}

$signtool = Find-SignTool
if (-not $signtool) {
  Write-Warning "[sign] 未找到 signtool.exe（需 Windows SDK），跳过签名"
  exit 0
}

Write-Host "[sign] 证书: $($cert.Subject) / $($cert.Thumbprint)"
foreach ($item in $Path) {
  if (-not (Test-Path $item)) {
    Write-Warning "[sign] 跳过（不存在）: $item"
    continue
  }
  & $signtool sign /sha1 $cert.Thumbprint /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 $item 2>&1 | Out-Null
  if ($LASTEXITCODE -ne 0) {
    Write-Warning "[sign] 时间戳服务不可用，改签不带时间戳: $item"
    & $signtool sign /sha1 $cert.Thumbprint /fd SHA256 $item 2>&1 | Out-Null
  }
  $sig = Get-AuthenticodeSignature $item
  Write-Host ("[sign] {0} -> {1}" -f (Split-Path -Leaf $item), $sig.Status)
}
exit 0
