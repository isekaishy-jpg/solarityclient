# Stock client architecture analysis

`ExportStockArchitecture.java` extracts architecture evidence from a completed
Ghidra analysis. It writes only program metadata, imports, RTTI names, embedded
source-file strings, and their cross-references. It does not export decompiled
source or copy any part of the client executable into this repository.

## Target binary

The current architecture seed was derived from this exact client:

| Field | Value |
| --- | --- |
| Product version | 3.3.5.12340 |
| Size | 7,704,216 bytes |
| MD5 | `45892bdedd0ad70aed4ccd22d9fb5984` |
| SHA-256 | `aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8` |
| PE language | `x86:LE:32:default:windows` |

Verify the SHA-256 before treating another executable as the same evidence
source. A modified private-server executable may retain the version resource
while changing code and offsets.

## Reproduce the report

Ghidra 12.1.3 requires a 64-bit JDK 21. The following PowerShell commands keep
the Ghidra project and generated reports outside the repository:

```powershell
$ghidraHome = 'C:\path\to\ghidra_12.1.3_PUBLIC'
$jdkHome = 'C:\path\to\jdk-21'
$wowExecutable = 'C:\path\to\Wow.exe'
$projectRoot = 'C:\path\to\solarity-ghidra'
$evidenceRoot = Join-Path $projectRoot 'evidence'
$repositoryRoot = (Resolve-Path '..\..').Path

(Get-FileHash -Algorithm SHA256 -LiteralPath $wowExecutable).Hash
$env:JAVA_HOME = $jdkHome

& (Join-Path $ghidraHome 'support\analyzeHeadless.bat') `
    $projectRoot `
    'SolarityWow335' `
    -import $wowExecutable `
    -overwrite `
    -analysisTimeoutPerFile 1200

& (Join-Path $ghidraHome 'support\analyzeHeadless.bat') `
    $projectRoot `
    'SolarityWow335' `
    -process 'Wow.exe' `
    -noanalysis `
    -scriptPath (Join-Path $repositoryRoot 'tools\ghidra') `
    -postScript 'ExportStockArchitecture.java' $evidenceRoot
```

The exporter creates:

- `program.tsv`: executable identity and recovered function count;
- `imports.tsv`: external namespaces and imported symbols;
- `source_files.tsv`: embedded source paths, string addresses, cross-references,
  and containing recovered functions;
- `rtti_types.tsv`: MSVC RTTI descriptors and their addresses.

The first command uses `-overwrite`. Point it only at a dedicated analysis
project whose existing `Wow.exe` program may be replaced.
