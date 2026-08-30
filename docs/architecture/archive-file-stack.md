# Build-12340 archive and file stack

This document records the stock evidence governing the initial Solarity asset
boundary. It applies to the exact 3.3.5a executable identified in
[`tools/ghidra/README.md`](../../tools/ghidra/README.md). It is not a generalized
MPQ load-order proposal.

## Recovered startup behavior

The archive startup routine at `0x00405dd0` uses a 28-row descriptor table at
`0x00ab6168`. Base priorities begin at 63 and decrement for every descriptor
except alternate (type 3) and streaming (type 4) rows. The consolidated archive
profile present in the local test client therefore has these priorities:

| Priority | Archive |
| ---: | --- |
| 49 | `expansion.MPQ` |
| 48 | `lichking.MPQ` |
| 47 | `common.MPQ` |
| 46 | `common-2.MPQ` |
| 45 | `<locale>/locale-<locale>.MPQ` |
| 44 | `<locale>/speech-<locale>.MPQ` |
| 43 | `<locale>/expansion-locale-<locale>.MPQ` |
| 42 | `<locale>/lichking-locale-<locale>.MPQ` |
| 41 | `<locale>/expansion-speech-<locale>.MPQ` |
| 40 | `<locale>/lichking-speech-<locale>.MPQ` |

The omitted table rows describe the stock split, alternate, development, and
streaming distributions. The current implementation selects the complete
consolidated WotLK profile and reports a missing member instead of silently
substituting another layout. Supporting another stock layout requires its own
evidence-backed profile.

Patch enumeration occurs in the routine at `0x00405ab0`:

1. Enumerate `Data\patch-?.MPQ` and
   `Data\<locale>\patch-<locale>-?.MPQ`. The `?` wildcard is exactly one
   character.
2. Sort the full relative paths descending through the comparator at
   `0x00401200`.
3. Append `Data\patch.MPQ`, then
   `Data\<locale>\patch-<locale>.MPQ`, when present.
4. Open the list in reverse order, assigning priorities from 64 upward.

For the common enUS patch set, highest-to-lowest resolution order is therefore
`patch-3`, `patch-2`, `enUS/patch-enUS-3`, `enUS/patch-enUS-2`, `patch`, and
`enUS/patch-enUS`. This sometimes surprising global/localized relationship is
covered by generated-MPQ integration tests and must not be replaced with a
guessed locale-first policy.

## Locale table

The locale probe at `0x00402d50` reads this 12-entry table at `0x00ad3028`, in
order:

```text
deDE enGB enUS esES frFR koKR zhCN zhTW enCN enTW esMX ruRU
```

The legacy `enCN` and `enTW` tokens are intentionally retained. Tokens absent
from this table are rejected rather than mapped to a nearby locale.

## Implementation boundary

`solarity-asset` validates archive-relative paths once, normalizes separators
and ASCII case once, then probes mounted archives in descending stock priority.
It does not build `wow-mpq`'s eager patch-chain file map: a client has only a
small number of mounted archives, while a complete cross-archive filename map
would scale with every entry in multi-gigabyte data files. The dependency's MPQ
types and errors remain private to the adapter.

WDBC loading consumes the same arbitrary-file lookup. It has no database-only
archive search or loose-file fallback.

## 64-bit and HD assets

Solarity is a 64-bit application. The asset and composition crates reject a
32-bit compilation target, and archive positions and declared sizes remain
64-bit until a payload must be represented in the process address space.

HD packs do not create an `hd/` namespace, quality fallback, or second decoder
path. They contain the same virtual filenames and formats with larger payloads.
Normal archive precedence selects the winning bytes; downstream BLP, M2, WMO,
and terrain decoders inspect that payload exactly as they would the stock
entry. A stock-compatible lettered archive such as `patch-H.MPQ` is already
included by the recovered single-character patch wildcard and sorts through
the same patch algorithm. The generated fixture suite includes a multi-sector
HD replacement to keep this behavior explicit.

Archive reads currently return owned bytes because compressed MPQ entries must
be reconstructed before their format decoder consumes them. Runtime loading
must schedule those reads off the interactive thread and keep decoded/GPU
resource caches budgeted; the existence of larger HD entries is not permission
to introduce a lower-quality fallback that stock never selected.

## Validation

Generated MPQs keep automated tests independent of developer client assets:

```powershell
cargo test -p solarity-asset --test stock_seed
```

Validate selected paths against a local client without copying its data into
the repository:

```powershell
cargo run -p solarity-asset --example validate_client_data -- `
    'C:\path\to\World of Warcraft\Data' `
    enUS `
    'DBFilesClient\Map.dbc' `
    'Interface\FrameXML\FrameXML.toc'
```

DBC arguments additionally validate their WDBC headers and declared record and
string-block bounds.

For local source archaeology, one UTF-8 archive member can be streamed to
standard output without extracting or copying the archive stack:

```powershell
cargo run -p solarity-asset --example print_client_text -- `
    'C:\path\to\World of Warcraft\Data' `
    enUS `
    'Interface\GlueXML\GlueTemplates.lua'
```
