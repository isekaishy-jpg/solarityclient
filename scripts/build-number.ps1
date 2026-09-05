<# Shared package sequence, kept outside disposable Cargo output. #>
Set-StrictMode -Version Latest

function Reserve-SolarityBuildNumber {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)] [string] $RepositoryRoot,
        [Parameter(Mandatory = $true)] [string] $SequencePath
    )

    # The caller holds the repository's package lock through compilation.
    # The committed number survives clones; the Git-common high-water mark
    # also survives local branch changes, worktrees, and failed package builds.
    $numberPath = Join-Path $RepositoryRoot 'BUILD_NUMBER'
    [uint32] $tracked = [uint32]::Parse([IO.File]::ReadAllText($numberPath).Trim(), [Globalization.CultureInfo]::InvariantCulture)
    [uint32] $highWater = 0
    if (Test-Path -LiteralPath $SequencePath -PathType Leaf) {
        $highWater = [uint32]::Parse([IO.File]::ReadAllText($SequencePath).Trim(), [Globalization.CultureInfo]::InvariantCulture)
    }
    [uint32] $previous = [Math]::Max($tracked, $highWater)
    if ($previous -eq [uint32]::MaxValue) {
        throw 'The package build sequence is exhausted.'
    }
    [uint32] $next = $previous + 1
    $numberText = $next.ToString([Globalization.CultureInfo]::InvariantCulture) + "`n"
    $encoding = [Text.UTF8Encoding]::new($false)
    # Reserve before changing the worktree or building. A later failure must
    # leave a gap instead of letting another package reuse this identity.
    [IO.File]::WriteAllText($SequencePath, $numberText, $encoding)
    [IO.File]::WriteAllText($numberPath, $numberText, $encoding)
    return $next
}
