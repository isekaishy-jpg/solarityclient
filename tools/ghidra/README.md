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

`ExportStockArchiveEvidence.java` is a focused follow-up exporter. It records
every embedded MPQ string together with its reference and containing recovered
function, without exporting decompiled source. Run it against the same analyzed
program when changing archive discovery or precedence behavior.

The first command uses `-overwrite`. Point it only at a dedicated analysis
project whose existing `Wow.exe` program may be replaced.

## Isolated M2 animation sampler oracle

`animation_sampler_oracle.py` loads the fingerprinted executable into
[Unicorn](https://www.unicorn-engine.org/docs/tutorial.html) and executes only
the original M2 interpolation and quaternion/matrix routines. It never runs
the client entry point. The harness supplies interval indices and fractions;
it validates key addressing and math, not timestamp search or scene timing.

For an isolated Python environment with Unicorn 2.1.4 installed, run:

```text
python tools/ghidra/animation_sampler_oracle.py <path-to-Wow.exe> --output target/stock-animation-oracle.json
```

The script rejects another executable fingerprint. Its output records the
original numeric results behind `model/track_sampling.rs` regression cases,
including non-unit quaternion matrices and all four interpolation selectors.
Unicorn is a research-tool dependency; normal Cargo tests require neither it
nor a stock executable.

## Isolated WMO registration oracle

`wmo_registration_oracle.py` executes original portal, BSP floor, segment-box,
and combined root/group registration routines from the same fingerprinted PE.
Portal and uncached floor queries have no hooks. Cached queries receive
equivalent predecoded leaf records through the cache-provider boundary. Root
group creation receives allocated object/reference records; its list insertion,
group indices, flags, and bounds still execute original instructions. Whole
resident groups are inputs, so the harness does not test asynchronous loading,
cross-root/terrain resolution, or dynamic-object reference insertion.

```text
python tools/ghidra/wmo_registration_oracle.py <path-to-Wow.exe> --portal-output target/wmo-portal-probe-native.txt --floor-output target/wmo-bsp-probe-native.txt --registration-output target/wmo-root-registration-native.txt --box-output target/wmo-segment-box-native.txt
```

The committed numeric fixtures in `crates/systems/tests/fixtures` run without
Unicorn or the executable. They cover portal sides and edges, plane proximity,
primary/fallback face flags, equal-distance replacement, float-spill precision,
BSP clipping/order, cache outcodes, the 8,192-face selection limit, group flags,
point containment, and floor-versus-portal precedence.
