# Product versions and numbered builds

The current product version is **0.0.3a**. This user-authorized release boundary
covers the work since 0.0.2a across world audio and movement, transports, water,
player recovery, sky and lighting, and the associated UI and asset fixes. The
water and lighting work overlaps the next development slice; the version change
does not declare every affected system complete. See the
[comprehensive 0.0.3a patch notes](releases/0.0.3a.md) and their complete commit
ledger for implemented behavior, validation, and remaining integration work.

The preceding **0.0.2a** release marked the accepted basic locomotion milestone;
its historical scope is recorded in [local player movement](architecture/local-player-movement.md).

A version bump is a release decision, not a side effect of committing a fix
or compiling code. Numbered Testing builds continue across version changes.

## Patch notes required for every version change

Before changing a product version, identify the previous version-change commit
and review every intervening commit, its description and affected behavior.
Write `docs/releases/<product-version>.md` with comprehensive grouped notes,
the exact comparison range, validation evidence, known limitations, and an
accounting of every commit, including package-only and internal changes. A
linked commit ledger may hold that accounting. Reconcile intermediate claims
against the final source: a later integration can complete an earlier helper,
while retained but unwired code must remain explicitly identified as such.

Link each release from [CHANGELOG.md](../CHANGELOG.md) and this guide. Include
the version and packaging changes themselves in the release record, and verify
that all workspace package versions and the compiled product identity agree.
Version changes may mark overlapping slices when the user chooses that boundary;
patch notes must not turn that decision into a claim of complete stock parity
or an unmeasured performance target.

## Version spelling

`Cargo.toml` is the version source. All workspace packages inherit its
SemVer-compatible spelling; the runtime embeds the product spelling:

| Stage | Cargo version example | Product version |
| --- | --- | --- |
| Alpha | `0.0.0-alpha` | `0.0.0a` |
| Beta | `0.0.1-beta` | `0.0.1b` |
| Release candidate | `0.0.1-rc` | `0.0.1rc` |
| Stable | `0.0.1` | `0.0.1s` |

The build number increases once per packaged Testing or release build. It
starts at one for this convention, since historic local compilations were
not counted. Zero denotes the source state before the first numbered package.
Display uses at least six digits: `Solarity 0.0.0a (Build 000001)`.

## Creating a package

Use the canonical packaging checkout, with the pinned native dependency
environment configured, and synchronize its committed `BUILD_NUMBER` before
packaging. Independent clones must not issue package numbers concurrently;
any future CI packaging must use this same single sequence authority.

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/build-client.ps1 -Profile test-client
powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/build-client.ps1 -Profile release
```

The script reserves the next number before compiling. It writes the tracked
`BUILD_NUMBER` and a high-water mark in Git's common directory, and holds an
exclusive package lock through compilation. Linked worktrees share that
high-water mark. Failed attempts consume their number; gaps are expected.
Switching to an older branch, deleting `target`, or changing release stages
must not reuse a reserved number. Record the updated `BUILD_NUMBER` in the
package commit and push it before another checkout packages a build.

Ordinary `cargo build`, tests, and lint checks do not reserve a number. They
embed the current source identity. A locally rebuilt executable is not a new
numbered package until the packaging script has reserved its number.

`scripts/install-test-client.ps1` calls the package script by default. Use
`-SkipBuild` to install an already numbered artifact without consuming another
number. Installation queries the executable's `--build-info`, so a newer
checkout cannot relabel an older executable. The installed `build-info.txt`
also records the executable's SHA-256 and installation time.

Normal Testing installs omit `-FrameTimings`; enable it only for an explicit
diagnostic run. Instrumented application timings are aggregated every two
seconds, and the normal launcher does not enable them.

The executable reports product identity through `--version`, `--build-info`,
the window title, and the startup log. Source revision and dirty status are
captured during compilation. Archive, network authentication, and stock Lua
compatibility identifiers remain tied to client 3.3.5 build 12340.

## Commit descriptions

Every commit has a Conventional Commit title and a body stating the concrete
purpose, resulting behavior, and relevant validation. Explain the reason for
the change and any material limitation. For example:

```text
fix(rendering): reproduce stock particle depth fog

Particles faded too quickly away from the center of the screen because
their fog used radial distance and omitted the authored exponent. Use the
original shader's eye-depth calculation for both camera projections.

The previous shader fails the new off-axis framebuffer regression. All
16 corrected cases and the required workspace checks pass.
```

Past shared history stays intact; this convention applies to new commits.
