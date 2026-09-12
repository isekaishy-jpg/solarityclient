<#
.SYNOPSIS
Runs the installed Testing client with opt-in CPU and/or sampled GPU diagnostics.
.DESCRIPTION
Uses the installed launcher's existing settings and timestamped log. Timing flags
apply only to this process tree; the normal Testing launch remains unchanged.
#>
[CmdletBinding()]
param(
    [ValidateSet('cpu', 'gpu', 'both')]
    [string] $Mode = 'both',
    [string] $InstallRoot = (Join-Path $env:LOCALAPPDATA 'SolarityClient/testing')
)

$ErrorActionPreference = 'Stop'
$launcher = Join-Path $InstallRoot 'launch-solarity-testing.ps1'
if (-not (Test-Path -LiteralPath $launcher -PathType Leaf)) {
    throw "Testing launcher is unavailable: $launcher"
}
$previousCpuTiming = $env:SOLARITY_FRAME_TIMINGS
$previousGpuTiming = $env:SOLARITY_GPU_TIMINGS
try {
    $env:SOLARITY_FRAME_TIMINGS = if ($Mode -in 'cpu', 'both') { '1' } else { $null }
    $env:SOLARITY_GPU_TIMINGS = if ($Mode -in 'gpu', 'both') { '1' } else { $null }
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $launcher
    if ($LASTEXITCODE -ne 0) { throw "Profiled Testing client exited with code $LASTEXITCODE" }
}
finally {
    $env:SOLARITY_FRAME_TIMINGS = $previousCpuTiming
    $env:SOLARITY_GPU_TIMINGS = $previousGpuTiming
}
