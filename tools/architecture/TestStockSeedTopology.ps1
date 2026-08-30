param(
    [Parameter(Mandatory = $true)]
    [string] $InventoryDirectory,

    [string] $RepositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# These modules are deliberate architectural splits whose responsibilities
# cross the primary owner selected for an individual recovered artifact.
$crossCuttingSeedFiles = [ordered]@{
    'cpu/pool' = @('executor', 'task', 'worker')
    'ecs/view' = @('state')
    'ecs/world' = @('registry', 'state')
    'systems/camera' = @('controller', 'transition')
    'systems/object' = @('lifecycle', 'update')
    'systems/player' = @('control', 'interaction', 'selection')
    'ui/event' = @('dispatch', 'payload', 'registry')
}

# These recovered implementation families are supplied through approved
# dependencies or narrow platform adapters rather than copied into Rust.
$dependencySeeds = @(
    'asset/archive',
    'media/audio/backend',
    'media/audio/codec',
    'media/cinematic',
    'runtime/platform/lcd'
)

function Convert-ArtifactToModuleName {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Artifact,

        [Parameter(Mandatory = $true)]
        [string] $Owner
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
    if ($name -in $reservedNames) {
        $name = "${name}_source"
    }
    $ownerLeaf = ($Owner -split '/')[-1]
    if ($name -eq $ownerLeaf) { $name = "${name}_source" }
    return $name
}

function Test-ModuleDeclaration {
    param(
        [Parameter(Mandatory = $true)]
        [string] $DeclarationFile,

        [Parameter(Mandatory = $true)]
        [string] $ModuleName,

        [Parameter(Mandatory = $true)]
        [AllowEmptyCollection()]
        [System.Collections.Generic.List[string]] $Failures
    )

    $declaration = "mod $ModuleName;"
    $declarationText = Get-Content -Raw -LiteralPath $DeclarationFile
    if ($declarationText -notmatch "(?m)^$([regex]::Escape($declaration))`r?$") {
        $relativeFile = [IO.Path]::GetRelativePath($RepositoryRoot, $DeclarationFile)
        $Failures.Add("missing '$declaration' in $relativeFile")
    }
}

$sourcePath = Join-Path $InventoryDirectory 'stock-source-ownership.tsv'
$rttiPath = Join-Path $InventoryDirectory 'stock-rtti-ownership.tsv'
$importPath = Join-Path $InventoryDirectory 'stock-import-ownership.tsv'
if (
    -not (Test-Path -LiteralPath $sourcePath) -or
    -not (Test-Path -LiteralPath $rttiPath) -or
    -not (Test-Path -LiteralPath $importPath)
) {
    throw 'inventory directory does not contain all three ownership reports'
}

$sourceInventory = @(Import-Csv -Delimiter "`t" -LiteralPath $sourcePath)
$rttiInventory = @(Import-Csv -Delimiter "`t" -LiteralPath $rttiPath)
$importInventory = @(Import-Csv -Delimiter "`t" -LiteralPath $importPath)
if ($sourceInventory.Count -ne 395) {
    throw "expected 395 stock source artifacts, found $($sourceInventory.Count)"
}
if ($rttiInventory.Count -ne 566) {
    throw "expected 566 stock RTTI artifacts, found $($rttiInventory.Count)"
}
if ($importInventory.Count -ne 470) {
    throw "expected 470 stock import artifacts, found $($importInventory.Count)"
}

$unmapped = @($sourceInventory + $rttiInventory + $importInventory | Where-Object {
    [string]::IsNullOrWhiteSpace($_.crate) -or [string]::IsNullOrWhiteSpace($_.module)
})
if ($unmapped.Count -gt 0) {
    throw "ownership inventory contains $($unmapped.Count) unmapped artifacts"
}

$mappedSeeds = @($sourceInventory + $rttiInventory + $importInventory |
    ForEach-Object { "$($_.crate)/$($_.module)" } |
    Sort-Object -Unique)
$requiredSeeds = @($mappedSeeds + $crossCuttingSeedFiles.Keys | Sort-Object -Unique)
$failures = [System.Collections.Generic.List[string]]::new()

foreach ($seed in $requiredSeeds) {
    $segments = @($seed -split '/')
    $crate = $segments[0]
    $moduleSegments = @($segments[1..($segments.Count - 1)])
    $modulePath = Join-Path $RepositoryRoot "crates/$crate/src/$($moduleSegments -join '/')/mod.rs"
    if (-not (Test-Path -LiteralPath $modulePath -PathType Leaf)) {
        $failures.Add("missing module facade: $seed")
        continue
    }

    $declarationFile = Join-Path $RepositoryRoot "crates/$crate/src/lib.rs"
    foreach ($moduleSegment in $moduleSegments) {
        $failureCount = $failures.Count
        Test-ModuleDeclaration -DeclarationFile $declarationFile -ModuleName $moduleSegment -Failures $failures
        if ($failures.Count -gt $failureCount) { break }
        $declarationFile = Join-Path (Split-Path -Parent $declarationFile) "$moduleSegment/mod.rs"
    }
}

# Each non-vendor source family gets one traceable Rust implementation seed.
# Paired C++ headers and sources intentionally collapse onto the same module.
$sourceFileSeeds = @($sourceInventory |
    Where-Object disposition -ne 'vendor' |
    ForEach-Object {
        $owner = "$($_.crate)/$($_.module)"
        [pscustomobject]@{
            owner = $owner
            module_name = Convert-ArtifactToModuleName -Artifact $_.artifact -Owner $owner
        }
    } |
    Sort-Object owner, module_name -Unique)
$ownersWithSourceFiles = @($sourceFileSeeds.owner | Sort-Object -Unique)
$implementationSeedCount = 0

foreach ($seed in $requiredSeeds) {
    $crate, $module = $seed -split '/', 2
    $moduleDirectory = Join-Path $RepositoryRoot "crates/$crate/src/$module"
    $moduleFacade = Join-Path $moduleDirectory 'mod.rs'
    $childNames = [System.Collections.Generic.List[string]]::new()

    foreach ($sourceSeed in @($sourceFileSeeds | Where-Object owner -eq $seed)) {
        $childNames.Add($sourceSeed.module_name)
    }
    if ($seed -notin $ownersWithSourceFiles) {
        $childNames.Add('types')
    }
    if ($seed -in $dependencySeeds) {
        $childNames.Add('dependency')
    }
    if ($crossCuttingSeedFiles.Contains($seed)) {
        foreach ($childName in $crossCuttingSeedFiles[$seed]) {
            $childNames.Add($childName)
        }
    }

    foreach ($childName in @($childNames | Sort-Object -Unique)) {
        $implementationSeedCount += 1
        $childPath = Join-Path $moduleDirectory "$childName.rs"
        if (-not (Test-Path -LiteralPath $childPath -PathType Leaf)) {
            $relativePath = [IO.Path]::GetRelativePath($RepositoryRoot, $childPath)
            $failures.Add("missing implementation seed: $relativePath")
            continue
        }
        Test-ModuleDeclaration -DeclarationFile $moduleFacade -ModuleName $childName -Failures $failures
    }
}

# One integration-test target per crate includes one external test module for
# every required production owner, avoiding hundreds of separate test binaries.
foreach ($crateGroup in ($requiredSeeds | Group-Object { ($_ -split '/')[0] })) {
    $crate = $crateGroup.Name
    $testRoot = Join-Path $RepositoryRoot "crates/$crate/tests/stock_seed.rs"
    if (-not (Test-Path -LiteralPath $testRoot -PathType Leaf)) {
        $failures.Add("missing external test root: crates/$crate/tests/stock_seed.rs")
        continue
    }

    $testRootText = Get-Content -Raw -LiteralPath $testRoot
    foreach ($seed in $crateGroup.Group) {
        $null, $module = $seed -split '/', 2
        $testModuleName = $module -replace '/', '_'
        $testPath = Join-Path $RepositoryRoot "crates/$crate/tests/stock_seed/$module.rs"
        if (-not (Test-Path -LiteralPath $testPath -PathType Leaf)) {
            $relativePath = [IO.Path]::GetRelativePath($RepositoryRoot, $testPath)
            $failures.Add("missing external test seed: $relativePath")
        }
        $pathAttribute = "#[path = `"stock_seed/$module.rs`"]"
        $moduleDeclaration = "mod $testModuleName;"
        if (
            $testRootText -notmatch "(?m)^$([regex]::Escape($pathAttribute))`r?$" -or
            $testRootText -notmatch "(?m)^$([regex]::Escape($moduleDeclaration))`r?$"
        ) {
            $relativeRoot = [IO.Path]::GetRelativePath($RepositoryRoot, $testRoot)
            $failures.Add("missing test declaration for '$seed' in $relativeRoot")
        }
    }
}

if ($failures.Count -gt 0) {
    $failures | Sort-Object -Unique
    throw "stock seed topology has $($failures.Count) failure(s)"
}

[pscustomobject]@{
    source_artifacts = $sourceInventory.Count
    rtti_artifacts = $rttiInventory.Count
    import_artifacts = $importInventory.Count
    mapped_module_owners = $mappedSeeds.Count
    required_seed_modules = $requiredSeeds.Count
    implementation_seed_files = $implementationSeedCount
    external_test_seed_files = $requiredSeeds.Count
    topology_failures = 0
} | Format-List
