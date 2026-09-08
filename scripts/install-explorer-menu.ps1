[CmdletBinding()]
param(
    [Parameter(Mandatory = $false)]
    [string]$ExecutablePath
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($ExecutablePath)) {
    $candidates = @(
        (Join-Path $PSScriptRoot '..\disk-janitor.exe'),
        (Join-Path $PSScriptRoot '..\DashuaiCleaner.exe'),
        (Join-Path $PSScriptRoot '..\release\DashuaiCleaner.exe'),
        (Join-Path $PSScriptRoot '..\target\release\disk-janitor.exe')
    )
    $ExecutablePath = $candidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
        Select-Object -First 1
}

if ([string]::IsNullOrWhiteSpace($ExecutablePath) -or
    -not (Test-Path -LiteralPath $ExecutablePath -PathType Leaf)) {
    throw 'Executable not found. Pass its full path with -ExecutablePath.'
}

$exe = (Resolve-Path -LiteralPath $ExecutablePath).Path
$classesRoot = 'HKCU:\Software\Classes'
$entries = @(
    @{ Key = 'Directory\shell\DashuaiCleaner'; Placeholder = '%1' },
    @{ Key = 'Directory\Background\shell\DashuaiCleaner'; Placeholder = '%V' },
    @{ Key = 'Drive\shell\DashuaiCleaner'; Placeholder = '%1' }
)

foreach ($entry in $entries) {
    $verbKey = Join-Path $classesRoot $entry.Key
    $commandKey = Join-Path $verbKey 'command'
    New-Item -Path $commandKey -Force | Out-Null
    Set-Item -LiteralPath $verbKey -Value 'Analyze with Dashuai Cleaner'
    New-ItemProperty -LiteralPath $verbKey -Name 'Icon' -Value $exe `
        -PropertyType String -Force | Out-Null
    $command = '"{0}" --path "{1}" --scan' -f $exe, $entry.Placeholder
    Set-Item -LiteralPath $commandKey -Value $command
}

Write-Host "Explorer context menu installed for current user: $exe"
