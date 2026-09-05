<# Deterministic package-sequence regression, isolated from the real counter. #>
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot '../build-number.ps1')
$fixtureRoot = Join-Path ([IO.Path]::GetTempPath()) ('solarity-build-sequence-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixtureRoot | Out-Null
$numberPath = Join-Path $fixtureRoot 'BUILD_NUMBER'
$sequencePath = Join-Path $fixtureRoot 'sequence'
try {
    [IO.File]::WriteAllText($numberPath, '0')
    foreach ($expected in @(1, 2)) {
        $actual = Reserve-SolarityBuildNumber -RepositoryRoot $fixtureRoot -SequencePath $sequencePath
        if ($actual -ne $expected) { throw "Expected $expected; got $actual." }
    }
    # An older branch or failed package cannot reuse a previously reserved ID.
    [IO.File]::WriteAllText($numberPath, '0')
    if ((Reserve-SolarityBuildNumber -RepositoryRoot $fixtureRoot -SequencePath $sequencePath) -ne 3) {
        throw 'The shared high-water mark was not preserved.'
    }
    # A newer committed counter also advances the local sequence after a pull.
    [IO.File]::WriteAllText($numberPath, '999999')
    if ((Reserve-SolarityBuildNumber -RepositoryRoot $fixtureRoot -SequencePath $sequencePath) -ne 1000000) {
        throw 'The sequence did not advance beyond six digits.'
    }
    [IO.File]::WriteAllText($numberPath, [uint32]::MaxValue.ToString())
    $rejected = $false
    try { Reserve-SolarityBuildNumber -RepositoryRoot $fixtureRoot -SequencePath $sequencePath | Out-Null }
    catch { $rejected = $true }
    if (-not $rejected) { throw 'The exhausted sequence wrapped.' }
    Write-Host 'Package sequence checks passed.'
}
finally {
    # Delete only the three explicit files allocated by this fixture, then its
    # empty directory. No computed recursive deletion is needed.
    foreach ($path in @($numberPath, $sequencePath)) {
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path }
    }
    Remove-Item -LiteralPath $fixtureRoot
}
