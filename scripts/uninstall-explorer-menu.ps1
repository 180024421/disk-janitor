[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$classesRoot = 'HKCU:\Software\Classes'
$keys = @(
    'Directory\shell\DashuaiCleaner',
    'Directory\Background\shell\DashuaiCleaner',
    'Drive\shell\DashuaiCleaner'
)

foreach ($key in $keys) {
    $path = Join-Path $classesRoot $key
    if (Test-Path -LiteralPath $path) {
        Remove-Item -LiteralPath $path -Recurse -Force
    }
}

Write-Host 'Explorer context menu removed for current user.'
