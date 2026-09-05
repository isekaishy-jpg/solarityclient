<#
.SYNOPSIS
Reserves the next package build number and compiles a Testing or release client.
.DESCRIPTION
Run from the canonical packaging checkout after configuring the pinned native
dependencies. BUILD_NUMBER records the reserved number for source history.
Ordinary cargo commands do not reserve numbers. Failed attempts leave gaps.
#>
[CmdletBinding()]
param(
    [ValidateSet('test-client', 'release')]
    [string] $Profile = 'test-client'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'build-number.ps1')
$repositoryRoot = (Resolve-Path -LiteralPath (Split-Path -Parent $PSScriptRoot)).Path
$gitCommon = (& git -C $repositoryRoot rev-parse --git-common-dir).Trim()
if ($LASTEXITCODE -ne 0) { throw 'Cannot locate the package sequence directory.' }
if (-not [IO.Path]::IsPathRooted($gitCommon)) {
    $gitCommon = Join-Path $repositoryRoot $gitCommon
}
$gitCommon = [IO.Path]::GetFullPath($gitCommon)
$sequencePath = Join-Path $gitCommon 'solarity-build-number'
$lockPath = Join-Path $gitCommon 'solarity-package.lock'

# Sharing is denied for the whole build so two worktrees cannot stamp different
# executables from a moving BUILD_NUMBER. A conflicting package attempt fails.
$packageLock = [IO.File]::Open($lockPath, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
try {
    $number = Reserve-SolarityBuildNumber -RepositoryRoot $repositoryRoot -SequencePath $sequencePath
    Write-Host ('Reserved Build {0:D6} for {1}' -f $number, $Profile)
    Push-Location -LiteralPath $repositoryRoot
    try {
        & cargo build --locked --profile $Profile --package solarity-runtime --bin solarity-runtime
        if ($LASTEXITCODE -ne 0) { throw "Package compilation failed with exit code $LASTEXITCODE; Build $number remains reserved." }
    }
    finally { Pop-Location }
    $artifact = Join-Path $repositoryRoot "target\$Profile\solarity-runtime.exe"
    $previousRuntimePath = $env:PATH
    try {
        $env:PATH = (Join-Path $repositoryRoot 'vcpkg_installed\x64-windows\bin') + ';' + $env:PATH
        & $artifact --version
        if ($LASTEXITCODE -ne 0) { throw 'The packaged executable could not report its identity.' }
    }
    finally { $env:PATH = $previousRuntimePath }
    Write-Host "Record BUILD_NUMBER in the package commit: $number"
}
finally { $packageLock.Dispose() }
