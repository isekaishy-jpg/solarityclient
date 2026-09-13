# Critical performance work, September 13, 2026

This pass starts at `08f29e0d` (Testing Build 129). Its scope is structural
millisecond costs, preserving the current stock behavior and graphics settings.
It does not establish completion of the broader rendering or FrameXML slices.

## Changes and behavior boundaries

- Stationary unit floor/liquid registrations survive unrelated moving WMO roots.
  A bounded history contains both departure and arrival world bounds; overlapping
  XY probes, residency changes, or expired history execute the original query.
  Native Unit registration `7C2A70` uses vertical segments and the `7C25D0` world
  root-box gate. MapObject box registration keeps global invalidation because its
  transformed-box contract is different. Doodad membership uses residency identity.
- M2 attachment ancestry is rebuilt in one ordered pass. It retains the last
  preceding matching mount/body/item/animation allocation, including duplicate
  owners and missing parents. Retirement's point queries still observe each
  intervening owner rename. Static scenery does not enter the attachment hash.
- Scene lighting clears only previously published receiver slots. Ordered WMO
  doodad membership is retained across unrelated dynamic placement changes;
  remapping, removal, or changed light ownership rebuilds the lookup.
  Its current indexed extent remains separate from retained capacity, preserving
  the original bounded parent walk even after a large scene shrinks.
- Targeted text layout returns metadata only for requested owners, including
  explicit empty results. Initial full layout writes directly to its indexed
  vectors through the same shaping kernel. Glyph metrics and draw rules remain
  unchanged.
- Frame level and strata mutations publish the affected ordering dependencies
  without snapshotting every Lua object. Descendant glyph/texture ordering and
  pointer hit order are covered by retained-versus-fresh publication tests.
  `Raise` uses maintained per-strata level counts instead of scanning the Lua
  registry. Counts preserve duplicate maxima, hidden frames, lowered levels,
  dynamic registration, and the original bounded descendant level delta.

Lighting invalidation, attachment lookup, doodad membership, live text layout,
and frame raising have focused modules. Optional placement metadata and UI phase
timings separate discovery, copying, and publication costs. No animation cadence,
shadow quality, visibility gate, or graphics setting is reduced.

## Measurement method

The test machine has an Intel Core i5-9600K and NVIDIA GeForce GTX 1070.
Live measurements use Soap, Orgrimmar, 2560 by 1440, far clip 1277,
environment detail 1.5, shadow quality 5, anisotropic filtering 5, VSync off,
and the same camera and diagnostic profile. The city position is
`1515.34, -4417.27, 18.0499`, orientation `0.190609`; saved camera distance
is `8.480558` and pitch `12.604312`. The scene has 49 resident tiles and
22,728 static model placements. Live NPC/effect populations and daylight vary.

Each timed run settles for 20 seconds after world entry, captures the native
framebuffer, then enters the ordinary client loop. Results use 30 two-second
reports, ending 10 through 70 seconds after that transition. Frame times are
weighted by frame count. GPU samples occur once per 16 frames and overlap CPU
work; nested CPU scopes must not be added together. Verbose UI timing runs are
separate from steady FPS comparisons. No compilation or tests run concurrently.

The automated diagnostic resumes the existing authenticated local Soap session
and uses ordinary character selection/world entry. The temporary adapter and
credentials are excluded from production source and the numbered package.

## Evidence collected

| Run | FPS | Mean frame | Interpretation |
| --- | ---: | ---: | --- |
| Initial baseline city | 109.11 | 9.165 ms | Build 129 source |
| Final baseline city | 109.57 | 9.127 ms | Immediate comparison baseline |
| Final candidate city | 117.35 | 8.522 ms | About 7.1% higher FPS in this pair |
| Repeat baseline city | 109.77 | 9.110 ms | Baseline returns to its earlier range |
| Final baseline gate | 150.88 | 6.628 ms | Original gate position |
| Final candidate gate | 156.36 | 6.396 ms | About 3.6% higher FPS in this pair |

The final gate pair uses `1292.53, -4384.89, 26.2765`, orientation `6.18856`,
with the same normal camera and graphics settings. Each contains 30 reports
covering about 60 seconds. Admission and placement counts match; particle phase
varies. The candidate's worst observed frame was 33.04 ms versus 17.59 ms in
the baseline, so the average improvement does not establish a tail-latency gain.

In the immediate baseline/candidate pair, M2 preparation fell from 4.543 to
3.999 ms, and its instance traversal from 3.876 to 3.322 ms. Vulkan recording
remained about 0.97–0.98 ms and queue presentation about 0.10 ms. Sampled GPU
totals were 3.533 and 3.447 ms, respectively. The repeat baseline supports a
roughly **7% steady FPS gain** in this city view, with substantial work remaining.

The attachment benchmark uses actual placement records with 23,000 scenery
entries and 300 units, optional mounts, duplicate bodies, equipment, and visuals
(24,035 total). Twenty optimized iterations average **18.059 ms** for repeated
scalar parent queries and **0.896 ms** for indexed rebuilds, with equal results.
This isolates a population-scaling failure; it is not a 17 ms gain on every live
frame. Current live topology publication still contains millisecond work.

The minimap clock's targeted glyph phase fell from approximately 0.70–1.04 ms
to 0.09 ms. Opening the character panel originally caused a full UI publication
of 280–308 ms; targeted publication measured 8.53 ms in the refined run. These
are publication costs, not complete input latency. The existing unimplemented
`UnitPVPName` still interrupts that panel's stock Lua callback. Its separate
input stall remains; the panel is not a completed feature.

Input-only instrumentation measured approximately 525 ms inside the failing
binding before publication. A temporary function-call trace attributed about
550 ms to error propagation around `UnitPVPName` and `CharacterFrame_OnShow`,
10.4 ms to `Raise`, and 1.5 ms to `Show` itself. The hook was removed after
diagnosis. In the pinned `mlua-sys` 0.11.0 Lua 5.1 compatibility code,
`compat53_pushfuncname` searches the global table and nested fields before using
the call-site name. That is a concrete large-registry error-reporting cost,
separate from normal publication. This pass preserves the existing Lua fault
and does not replace the missing function with a guessed return value or alter
dependency traceback semantics.

With the maintained frame-order index, the character panel's `Raise` measured
**1.738 ms**, down from 9.90–10.41 ms; two small startup raises measured
0.012–0.013 ms instead of 6.93–7.22 ms. The final targeted publication measured
8.655 ms. The failing command still spends roughly half a second propagating its
Lua error, so these improvements must not be presented as a complete input-hitch
fix.

## Remaining costs

M2 preparation remains roughly four milliseconds in the city: bone evaluation,
effects, admission, lighting, and packet generation all contribute. Placement
topology still walks large resident records and rebuilds dependent fields.
GPU shadows, world geometry, transparent effects, and screen effects remain
material costs at Ultra. Live GPU variation must not be credited to CPU cache
changes. These measurements do not establish the 1,200 FPS stretch target.

## Visual checks

Separate native framebuffer captures at the city position use pitch `-14.558`
to expose the skyline. Camera collision shortens the distance to `2.696497`.
Baseline and candidate retain the same city walls, buildings, zeppelin, and
open sky gaps; no distant ridge reappeared in this view. Soap's body, equipment,
and effects remain present. These captures have different live idle/effect
phases and slightly different daylight, so they are a visual regression check,
not a pixel-identical stock-parity proof. The normal performance distance and
pitch were restored afterward. Build 129's existing WDL portal window
restriction remains unchanged.

## Validation

The final production source passes:

- `cargo fmt --all -- --check`
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`
- `cargo test --locked --workspace --all-features`: 1,372 passed, 28 ignored

This includes 188 UI tests, the sparse lighting bank's removal/remapping/cycle
regression, and the recovered native rendering, portal, registration, and
callback-order fixtures. The manual optimized ancestry timing is separate from
the deterministic test suite. Diagnostic hooks and session-resume edits are
removed before packaging; the installed launcher leaves frame profiling off.

## Testing package

Solarity `0.0.3a`, **Build 000130**, is installed through the normal Testing
installer and Desktop shortcut. The compiled source revision is
`db0a0e2ed81af2297f72bffc405a86726fc59571`; its dirty flag records the package
script's `BUILD_NUMBER` reservation. Product version and graphics quality are
unchanged. The installed and built executables have the same SHA-256:

`5049E4CC428827D6A117C32F4E415A08E60495AC67B17BD94B84B485389D7406`

The normal installed launcher opened Build 130, initialized its Glue resources
and Vulkan swapchain at 2560 by 1440 on the GTX 1070, and closed normally with
no logged warnings or errors. Its first-run screen was `Movie`; this startup
smoke check is separate from the automated live-world measurements above.
Soap is offline at the measured city position for the next session.
