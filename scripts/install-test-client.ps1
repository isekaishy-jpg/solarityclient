<#
.SYNOPSIS
Builds and installs the persistent Solarity user-testing client.

.DESCRIPTION
The Cargo artifact is built with the dedicated `test-client` profile and then
copied outside the disposable target tree. A generated launcher records every
run to a timestamped log, and a Desktop shortcut targets that launcher.
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string] $DataRoot,

    [string] $Locale = "enUS",
    [string] $LoginEndpoint = "127.0.0.1:3724",
    [string] $LoginClientIp = "127.0.0.1",

    [ValidateRange(1, 256)]
    [int] $CpuWorkers = 4,

    [ValidateRange(1, 1048576)]
    [int] $CpuCapacity = 256,

    [ValidateRange(1, 256)]
    [int] $NetworkWorkers = 2,

    [ValidateRange(1, 600000)]
    [int] $NetworkShutdownMilliseconds = 2000,

    [ValidateRange(1, 2147483647)]
    [int] $WindowWidth = 1280,

    [ValidateRange(1, 2147483647)]
    [int] $WindowHeight = 720,

    [ValidateSet("windowed", "fullscreen-windowed")]
    [string] $WindowMode = "fullscreen-windowed",

    [ValidateRange(0, 1024)]
    [int] $GpuIndex = 0,

    [string] $InstallRoot = (Join-Path $env:LOCALAPPDATA "SolarityClient\testing"),
    [string] $ShortcutPath = (Join-Path ([Environment]::GetFolderPath("Desktop")) "Solarity Client (Testing).lnk"),
    [switch] $SkipBuild
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$resolvedRepositoryRoot = (Resolve-Path -LiteralPath $repositoryRoot).Path
$resolvedDataRoot = (Resolve-Path -LiteralPath $DataRoot).Path
if (-not (Test-Path -LiteralPath $resolvedDataRoot -PathType Container)) {
    throw "Client data root is not a directory: $resolvedDataRoot"
}

$artifactPath = Join-Path $resolvedRepositoryRoot "target\test-client\solarity-runtime.exe"
if (-not $SkipBuild) {
    & cargo build --locked --profile test-client --package solarity-runtime
    if ($LASTEXITCODE -ne 0) {
        throw "The test-client Cargo build failed with exit code $LASTEXITCODE."
    }
}
if (-not (Test-Path -LiteralPath $artifactPath -PathType Leaf)) {
    throw "The test-client artifact is unavailable: $artifactPath"
}

$resolvedInstallRoot = [System.IO.Path]::GetFullPath($InstallRoot)
$resolvedShortcutPath = [System.IO.Path]::GetFullPath($ShortcutPath)
New-Item -ItemType Directory -Path $resolvedInstallRoot -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $resolvedInstallRoot "logs") -Force | Out-Null
New-Item -ItemType Directory -Path (Split-Path -Parent $resolvedShortcutPath) -Force | Out-Null

$installedExecutable = Join-Path $resolvedInstallRoot "solarity-runtime.exe"
$stagedExecutable = Join-Path $resolvedInstallRoot "solarity-runtime.exe.new"
try {
    Copy-Item -LiteralPath $artifactPath -Destination $stagedExecutable -Force
    Move-Item -LiteralPath $stagedExecutable -Destination $installedExecutable -Force
}
finally {
    if (Test-Path -LiteralPath $stagedExecutable -PathType Leaf) {
        Remove-Item -LiteralPath $stagedExecutable -Force
    }
}

# The Rust executable dynamically links the pinned FFmpeg/vcpkg runtime. Keep
# the installed DLL set identical to the current build tree so ABI-versioned
# files are replaced instead of accumulating across test builds.
$dependencyBin = Join-Path $resolvedRepositoryRoot "vcpkg_installed\x64-windows\bin"
if (-not (Test-Path -LiteralPath $dependencyBin -PathType Container)) {
    throw "The pinned x64 runtime dependency directory is unavailable: $dependencyBin"
}
$dependencyDlls = @(Get-ChildItem -LiteralPath $dependencyBin -Filter "*.dll" -File)
if ($dependencyDlls.Count -eq 0) {
    throw "No pinned x64 runtime DLLs were found in: $dependencyBin"
}
$dependencyNames = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
foreach ($dependency in $dependencyDlls) {
    [void] $dependencyNames.Add($dependency.Name)
    Copy-Item -LiteralPath $dependency.FullName -Destination (Join-Path $resolvedInstallRoot $dependency.Name) -Force
}
foreach ($installedDll in Get-ChildItem -LiteralPath $resolvedInstallRoot -Filter "*.dll" -File) {
    if ($dependencyNames.Contains($installedDll.Name)) {
        continue
    }
    $resolvedDll = [IO.Path]::GetFullPath($installedDll.FullName)
    if (-not [IO.Path]::GetDirectoryName($resolvedDll).Equals($resolvedInstallRoot, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to remove a runtime DLL outside the install root: $resolvedDll"
    }
    Remove-Item -LiteralPath $resolvedDll -Force
}

$launcherPath = Join-Path $resolvedInstallRoot "launch-solarity-testing.ps1"
$launcherTemplate = @'
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$executable = Join-Path $PSScriptRoot "solarity-runtime.exe"
$logDirectory = Join-Path $PSScriptRoot "logs"
New-Item -ItemType Directory -Path $logDirectory -Force | Out-Null
$logPath = Join-Path $logDirectory ("solarity-{0:yyyyMMdd-HHmmss}.log" -f (Get-Date))
$timezoneMinutes = [int][TimeZoneInfo]::Local.GetUtcOffset([DateTime]::Now).TotalMinutes
$arguments = @(
    "--data-root", '__DATA_ROOT__',
    "--profile-root", $PSScriptRoot,
    "--locale", '__LOCALE__',
    "--cpu-workers", '__CPU_WORKERS__',
    "--cpu-capacity", '__CPU_CAPACITY__',
    "--network-workers", '__NETWORK_WORKERS__',
    "--network-shutdown-ms", '__NETWORK_SHUTDOWN__',
    "--login-endpoint", '__LOGIN_ENDPOINT__',
    "--login-timezone-minutes", $timezoneMinutes.ToString([Globalization.CultureInfo]::InvariantCulture),
    "--login-client-ip", '__LOGIN_CLIENT_IP__',
    "--window-width", '__WINDOW_WIDTH__',
    "--window-height", '__WINDOW_HEIGHT__',
    "--window-mode", '__WINDOW_MODE__',
    "--gpu-index", '__GPU_INDEX__'
)

$previousErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = "Continue"
& $executable @arguments 2>&1 |
    ForEach-Object {
        if ($_ -is [Management.Automation.ErrorRecord]) {
            $_.Exception.Message
        }
        else {
            $_
        }
    } |
    Tee-Object -LiteralPath $logPath
$exitCode = $LASTEXITCODE
$ErrorActionPreference = $previousErrorActionPreference
if ($exitCode -ne 0) {
    Write-Host ""
    Write-Host "Solarity exited with code $exitCode. Log: $logPath" -ForegroundColor Red
    Read-Host "Press Enter to close"
}
exit $exitCode
'@

function ConvertTo-SingleQuotedPowerShellLiteral([string] $Value) {
    return $Value.Replace("'", "''")
}

function Set-Utf8NoBomContent([string] $LiteralPath, [string[]] $Value) {
    # Windows PowerShell 5.1 lacks Set-Content's utf8NoBOM encoding name. Use
    # the framework encoder so the persistent launcher is portable across the
    # PowerShell versions supported by the testing shortcut.
    $encoding = [Text.UTF8Encoding]::new($false)
    [IO.File]::WriteAllLines($LiteralPath, $Value, $encoding)
}

$launcher = $launcherTemplate
$launcher = $launcher.Replace("__DATA_ROOT__", (ConvertTo-SingleQuotedPowerShellLiteral $resolvedDataRoot))
$launcher = $launcher.Replace("__LOCALE__", (ConvertTo-SingleQuotedPowerShellLiteral $Locale))
$launcher = $launcher.Replace("__CPU_WORKERS__", $CpuWorkers.ToString([Globalization.CultureInfo]::InvariantCulture))
$launcher = $launcher.Replace("__CPU_CAPACITY__", $CpuCapacity.ToString([Globalization.CultureInfo]::InvariantCulture))
$launcher = $launcher.Replace("__NETWORK_WORKERS__", $NetworkWorkers.ToString([Globalization.CultureInfo]::InvariantCulture))
$launcher = $launcher.Replace("__NETWORK_SHUTDOWN__", $NetworkShutdownMilliseconds.ToString([Globalization.CultureInfo]::InvariantCulture))
$launcher = $launcher.Replace("__LOGIN_ENDPOINT__", (ConvertTo-SingleQuotedPowerShellLiteral $LoginEndpoint))
$launcher = $launcher.Replace("__LOGIN_CLIENT_IP__", (ConvertTo-SingleQuotedPowerShellLiteral $LoginClientIp))
$launcher = $launcher.Replace("__WINDOW_WIDTH__", $WindowWidth.ToString([Globalization.CultureInfo]::InvariantCulture))
$launcher = $launcher.Replace("__WINDOW_HEIGHT__", $WindowHeight.ToString([Globalization.CultureInfo]::InvariantCulture))
$launcher = $launcher.Replace("__WINDOW_MODE__", $WindowMode)
$launcher = $launcher.Replace("__GPU_INDEX__", $GpuIndex.ToString([Globalization.CultureInfo]::InvariantCulture))
Set-Utf8NoBomContent -LiteralPath $launcherPath -Value $launcher

$commit = (& git -C $resolvedRepositoryRoot rev-parse --verify HEAD).Trim()
if ($LASTEXITCODE -ne 0) {
    throw "Could not resolve the installed Git revision."
}
$buildInformation = @(
    "revision=$commit"
    "dirty=$([bool](& git -C $resolvedRepositoryRoot status --porcelain))"
    "installed_at=$([DateTimeOffset]::Now.ToString('O'))"
    "source=$resolvedRepositoryRoot"
    "data_root=$resolvedDataRoot"
    "locale=$Locale"
    "login_endpoint=$LoginEndpoint"
    "profile=test-client"
    "runtime_dlls=$([string]::Join(',', ($dependencyDlls.Name | Sort-Object)))"
)
Set-Utf8NoBomContent -LiteralPath (Join-Path $resolvedInstallRoot "build-info.txt") -Value $buildInformation

$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($resolvedShortcutPath)
$shortcut.TargetPath = (Get-Command powershell.exe).Source
$shortcut.Arguments = "-NoLogo -NoProfile -ExecutionPolicy Bypass -File `"$launcherPath`""
$shortcut.WorkingDirectory = $resolvedInstallRoot
$shortcut.IconLocation = "$installedExecutable,0"
$shortcut.Description = "Launch the persistent Solarity testing client"
$shortcut.Save()

Write-Host "Installed test client: $installedExecutable"
Write-Host "Created Desktop shortcut: $resolvedShortcutPath"
Write-Host "Run logs: $(Join-Path $resolvedInstallRoot 'logs')"
