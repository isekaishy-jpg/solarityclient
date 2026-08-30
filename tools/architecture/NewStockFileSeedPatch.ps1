param(
    [Parameter(Mandatory = $true)]
    [string] $InventoryDirectory,

    [string] $RepositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$crossCuttingSeedFiles = [ordered]@{
    'cpu/pool' = @('executor', 'task', 'worker')
    'ecs/view' = @('state')
    'ecs/world' = @('registry', 'state')
    'systems/camera' = @('controller', 'transition')
    'systems/object' = @('lifecycle', 'update')
    'systems/player' = @('control', 'interaction', 'selection')
    'ui/event' = @('dispatch', 'payload', 'registry')
}

$dependencySeeds = @(
    'asset/archive',
    'media/audio/backend',
    'media/audio/codec',
    'media/cinematic',
    'runtime/platform/lcd'
)

$crossCuttingDocumentation = @{
    'cpu/pool/executor' = '//! Ownership and lifecycle of the repository CPU executor.'
    'cpu/pool/task' = '//! Typed CPU task vocabulary and completion boundaries.'
    'cpu/pool/worker' = '//! Worker participation, shutdown, and capacity boundaries.'
    'ecs/view/state' = '//! Persistent renderer-independent camera target, mode, zoom, and orientation state.'
    'ecs/world/registry' = '//! Entity registration and lookup ownership for the active world.'
    'ecs/world/state' = '//! Active-world identity, lifecycle, and shared ECS state.'
    'systems/camera/controller' = '//! Player and world camera control policy.'
    'systems/camera/transition' = '//! Camera mode, target, vehicle, and shake transitions.'
    'systems/object/lifecycle' = '//! Shared object creation, activation, and removal behavior.'
    'systems/object/update' = '//! Application of stock object updates to ECS-owned state.'
    'systems/player/control' = '//! Local-player input and control behavior.'
    'systems/player/interaction' = '//! Local-player interaction orchestration across world systems.'
    'systems/player/selection' = '//! Local-player target, focus, and selection behavior.'
    'ui/event/dispatch' = '//! Frame and script event dispatch ordering.'
    'ui/event/payload' = '//! Typed UI event payload boundaries.'
    'ui/event/registry' = '//! UI event subscription and registration ownership.'
}

function Convert-ArtifactToModuleName {
    param(
        [Parameter(Mandatory = $true)][string] $Artifact,
        [Parameter(Mandatory = $true)][string] $Owner
    )

    $stem = [IO.Path]::GetFileNameWithoutExtension($Artifact)
    $name = $stem -creplace '([A-Z]+)([A-Z][a-z])', '$1_$2'
    $name = $name -creplace '([a-z0-9])([A-Z])', '$1_$2'
    $name = ($name -replace '[^A-Za-z0-9]+', '_').Trim('_').ToLowerInvariant()
    $reservedNames = @(
        'as', 'async', 'await', 'break', 'const', 'continue', 'crate', 'dyn',
        'else', 'enum', 'extern', 'false', 'fn', 'for', 'if', 'impl', 'in',
        'let', 'loop', 'match', 'mod', 'move', 'mut', 'pub', 'ref', 'return',
        'self', 'static', 'struct', 'super', 'trait', 'true', 'try', 'type',
        'unsafe', 'use', 'where', 'while'
    )
    if ($name -in $reservedNames) { $name = "${name}_source" }
    $ownerLeaf = ($Owner -split '/')[-1]
    if ($name -eq $ownerLeaf) { $name = "${name}_source" }
    return $name
}

function Convert-ToPatchPath {
    param([Parameter(Mandatory = $true)][string] $Path)
    return [IO.Path]::GetRelativePath($RepositoryRoot, $Path).Replace('\', '/')
}

$sourceInventory = @(Import-Csv -Delimiter "`t" -LiteralPath (Join-Path $InventoryDirectory 'stock-source-ownership.tsv'))
$rttiInventory = @(Import-Csv -Delimiter "`t" -LiteralPath (Join-Path $InventoryDirectory 'stock-rtti-ownership.tsv'))
$importInventory = @(Import-Csv -Delimiter "`t" -LiteralPath (Join-Path $InventoryDirectory 'stock-import-ownership.tsv'))
$mappedSeeds = @($sourceInventory + $rttiInventory + $importInventory |
    ForEach-Object { "$($_.crate)/$($_.module)" } |
    Sort-Object -Unique)
$requiredSeeds = @($mappedSeeds + $crossCuttingSeedFiles.Keys | Sort-Object -Unique)

$sourceFileSeeds = @($sourceInventory |
    Where-Object disposition -ne 'vendor' |
    ForEach-Object {
        $owner = "$($_.crate)/$($_.module)"
        [pscustomobject]@{
            owner = $owner
            module_name = Convert-ArtifactToModuleName -Artifact $_.artifact -Owner $owner
            artifact = $_.artifact
        }
    })
$ownersWithSourceFiles = @($sourceFileSeeds.owner | Sort-Object -Unique)
$expectedChildren = @{}

foreach ($seed in $requiredSeeds) {
    $children = [System.Collections.Generic.List[string]]::new()
    foreach ($sourceSeed in @($sourceFileSeeds | Where-Object owner -eq $seed)) {
        $children.Add($sourceSeed.module_name)
    }
    if ($seed -notin $ownersWithSourceFiles) { $children.Add('types') }
    if ($seed -in $dependencySeeds) { $children.Add('dependency') }
    if ($crossCuttingSeedFiles.Contains($seed)) {
        foreach ($child in $crossCuttingSeedFiles[$seed]) { $children.Add($child) }
    }
    $expectedChildren[$seed] = @($children | Sort-Object -Unique)
}

'*** Begin Patch'

foreach ($seed in $requiredSeeds) {
    $crate, $module = $seed -split '/', 2
    $moduleDirectory = Join-Path $RepositoryRoot "crates/$crate/src/$module"
    $facadePath = Join-Path $moduleDirectory 'mod.rs'
    $facadeText = Get-Content -Raw -LiteralPath $facadePath
    $missingDeclarations = @($expectedChildren[$seed] | Where-Object {
        $facadeText -notmatch "(?m)^mod $([regex]::Escape($_));`r?$"
    })
    if ($missingDeclarations.Count -gt 0) {
        $lastLine = @(Get-Content -LiteralPath $facadePath)[-1]
        "*** Update File: $(Convert-ToPatchPath -Path $facadePath)"
        '@@'
        " $lastLine"
        '+'
        foreach ($child in $missingDeclarations) { "+mod $child;" }
    }

    foreach ($child in $expectedChildren[$seed]) {
        $childPath = Join-Path $moduleDirectory "$child.rs"
        if (Test-Path -LiteralPath $childPath) { continue }
        "*** Add File: $(Convert-ToPatchPath -Path $childPath)"
        if ($child -eq 'types') {
            '+//! RTTI-backed type, state, and identifier vocabulary for this stock responsibility.'
        } elseif ($child -eq 'dependency') {
            '+//! Adapter boundary for the approved dependency or platform service replacing this vendor family.'
        } elseif ($crossCuttingDocumentation.ContainsKey("$seed/$child")) {
            "+$($crossCuttingDocumentation["$seed/$child"])"
        } else {
            $artifacts = @($sourceFileSeeds |
                Where-Object { $_.owner -eq $seed -and $_.module_name -eq $child } |
                Select-Object -ExpandProperty artifact |
                Sort-Object -Unique)
            $evidence = ($artifacts | ForEach-Object { "``$_``" }) -join ' and '
            "+//! Stock implementation responsibility recovered from $evidence."
        }
    }
}

foreach ($crateGroup in ($requiredSeeds | Group-Object { ($_ -split '/')[0] })) {
    $crate = $crateGroup.Name
    $testRoot = Join-Path $RepositoryRoot "crates/$crate/tests/stock_seed.rs"
    if (-not (Test-Path -LiteralPath $testRoot)) {
        "*** Add File: $(Convert-ToPatchPath -Path $testRoot)"
        '+//! External stock-compatibility test modules for this crate.'
        '+'
        foreach ($seed in $crateGroup.Group) {
            $null, $module = $seed -split '/', 2
            $testModuleName = $module -replace '/', '_'
            "+#[path = `"stock_seed/$module.rs`"]"
            "+mod $testModuleName;"
        }
    }

    foreach ($seed in $crateGroup.Group) {
        $null, $module = $seed -split '/', 2
        $testPath = Join-Path $RepositoryRoot "crates/$crate/tests/stock_seed/$module.rs"
        if (Test-Path -LiteralPath $testPath) { continue }
        "*** Add File: $(Convert-ToPatchPath -Path $testPath)"
        "+//! External stock-compatibility tests for ``$crate/$module`` belong here."
    }
}

'*** End Patch'
