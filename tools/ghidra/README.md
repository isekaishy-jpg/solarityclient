# Stock client architecture analysis

Mount request admission is captured from `739113..73917C`, with only the
`735820` submission intercepted:

```powershell
python tools/ghidra/unit_mount_request_oracle.py <path-to-Wow.exe> crates/runtime/tests/fixtures/unit_mount_request_native.txt
```

The 240 cases cover missing/disabled mounts, missing/changed/unchanged IDs,
both sides of the strict 0.01 rate tolerance, and submitted offset/blend values.
This is the final mount commit gate; upstream behavior routing and completion
are outside its scope. Runtime tests combine admitted requests with native M2
timers and random consumption, then exercise stride metadata through local,
remote-player and creature residency/update paths. The independent stride math
is captured by `unit_movement_speed_oracle.py`.

Mounted Unit_C routing and current-record normalization can be reproduced with:

```powershell
python tools/ghidra/unit_mount_owner_oracle.py <path-to-Wow.exe> crates/runtime/tests/fixtures/unit_mount_owner_native.txt
```

The 504 cases execute `7385C0`, `7173F0`, `6E6F80` and the original behavior
predicates for ordinary, mount-only and body-only masks, live/dead admission,
and active/finished/exhausted mount records. The request flag preserves the
supplied weapon-ready ID; weapon selection is outside this capture. Model
readiness, current sequence records, identity tier resolution, metadata and
inactive combat/passenger providers are supplied. `735820`/`737EF0` submissions
are captured rather than executed; post-commit direction/effect hooks are no-ops.
Runtime tests compare all 56 ordinary active-record cases, including separate
upper/body timers and dead rejection without random draws. Separate runtime
tests cover queued movement, body replacement and ordinary mount completion in
the shared callback scan; the capture does not prove full vehicle-control policy.

Seated vehicle ownership has a separate 150-case capture:

```powershell
python tools/ghidra/vehicle_animation_owner_oracle.py <path-to-Wow.exe> crates/runtime/tests/fixtures/vehicle_animation_owner_native.txt
```

It executes registration (`756D10`), the controlled-key predicate (`756CD0`),
completion (`757280`) and the original seated consumer (`747980`). GUID lookup
supplies resident or missing passengers. Model replay, key release and Unit_C
resume are captured boundaries. Tests cover masks, first-free registration,
capacity, missing owners, normal completion and interruption. This capture does
not execute the model setter or the entry/exit action consumer and its spell
release vector. Decoded model and renderer regressions exercise the implemented
seated model consumers separately.

The mount/body effect-binding probe executes the original authored adapter,
unit event switch and breath factory. All live cases query the body's attachment
17/19, independently of the emitting model and the mount's attachments:

```powershell
python tools/ghidra/unit_model_effect_binding_oracle.py <path-to-Wow.exe> --output target/unit-model-effect-binding.json
```

The 48 cases include missing unit lifetimes. Model readiness, attachment lookup,
allocation, finite-coordinate validation and constructor calls are controlled
providers; this does not validate bone posing, constructor RNG, mount completion
or effect rendering. A separate renderer regression checks the callback bridge
and attached effect placement with distinct body and mount assets.

The settled vehicle-seat fixture executes original `7490F0` arithmetic and
matrix routines. Model attachment lookup and unit virtual getters supply its
inputs:

```powershell
python tools/ghidra/vehicle_seat_pose_oracle.py <path-to-Wow.exe> --output crates/systems/tests/fixtures/vehicle-seat-pose-native.txt
```

Its 512 records cover attachment presence, passenger anchors, seat rotations,
offsets, scale cancellation and the missing-attachment world-frame fallback.
All matrix words compare exactly. Runtime tests separately exercise animated
bone ancestry, mounted riders, visibility and lighting/shadow inheritance.
Boarding/exit state transitions and vehicle camera dispatch are outside this
fixture; see [vehicle presentation](../../docs/architecture/vehicle-presentation.md).

The unit passenger matrix fixture executes the original `4C3380`, `4C3290`,
`4C1F00` and `4C2370` without hooks:

```powershell
python tools/ghidra/unit_passenger_frame_oracle.py <path-to-Wow.exe> --output crates/systems/tests/fixtures/unit-passenger-frame-native.txt
```

Its 512 records cover signed zero, ordinary yaw, and nested pitched/scaled parent
matrices. The runtime separately tests Unit_C/Vehicle_C selection, missing parent
matrix retention, lifetime replacement and local/remote publication order.
This does not establish animated seat attachment or vehicle camera policy.

The ordinary camera-opacity fixture executes the original `606F90` fragment
from `6077D0` through `6079FD`, including the complete `8CA080` cosine function:

```powershell
python tools/ghidra/camera_opacity_oracle.py <path-to-Wow.exe> --output crates/systems/tests/fixtures/camera_opacity_native.txt
```

Its 1,864 cases cover distance, principal height/pitch, near clip, fade endpoints,
steep downward views and deterministic randomized inputs. Vehicle subjects,
timed camera flags, the reduced-range global and final dispatch are excluded.
The shared emulator rejects any executable outside the pinned build-12340 hash.

The related player visibility and removal-admission capture executes the ordinary
player override and its native unit/base delegates. Only local-GUID lookup is hooked:

```powershell
python tools/ghidra/player_visibility_oracle.py <path-to-Wow.exe> --output crates/systems/tests/fixtures/player_visibility_native.txt
```

The 1,152 cases vary PlayerFlags, current map lookup/type, camera visibility,
local identity and scene masks. Async appearance publication, vehicle seats,
hidden-root child activation and alternate effect owners are outside this bank.

The vehicle-seat capture executes `5D3340/756EC0`, `716650` and `74B8B0` with
native-layout vehicle/seat tables. Only world GUID lookup and the transport
virtual getter are supplied; the seat lookup itself runs unmodified:

```powershell
python tools/ghidra/vehicle_seat_oracle.py <path-to-Wow.exe> --output crates/runtime/tests/fixtures/vehicle_seat_native.txt
```

The 3,072 cases include every seat byte and sparse/missing row ownership. They
distinguish the signed attachment ID at seat row `+8` from the flags at `+4`.
The earlier entity-entry fixture's `seat_flags` column actually captures this
attachment word. Vehicle movement, animated seat pose and camera dispatch are
outside this comparison; see [vehicle presentation](../../docs/architecture/vehicle-presentation.md).

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

## Isolated model effect clock oracle

`model_effect_clock_oracle.py` executes model scene construction at `0x00834810`
and effect update at `0x00828A00`. Hooks replace resource/scene registration and
capture particle dispatch; timestamp initialization, unsigned subtraction, and
millisecond conversion execute original instructions. The eight probes cover
creation at a nonzero tick, repeated ticks, a skipped update, and wraparound.
They do not test the particle simulator or model visibility admission.

The optional global output executes `0x0082F0F0` through its unsigned global
sequence writes, stopping before camera/bone evaluation. Its 240 probes vary
creation and elapsed ticks and authored duration, including zero duration,
wraparound, and elapsed values beyond exact float representation.

```text
python tools/ghidra/model_effect_clock_oracle.py <path-to-Wow.exe> target/model-effect-clock-native.txt --global-output target/model-global-clock-native.txt
```

## Default model sequence oracle

The following additional captures cover passenger animation policy and shared
bone callback ownership. They execute the original selectors/model code; their
scripts identify supplied application callbacks and palette providers. The
[animation boundary](../../docs/architecture/m2-animation.md#shared-active-bone-callback-scan)
records runtime consumers and remaining event-delivery work.

```text
python tools/ghidra/vehicle_animation_oracle.py <path-to-Wow.exe> --output crates/systems/tests/fixtures/vehicle-animation-native.txt
python tools/ghidra/model_bone_callbacks_oracle.py <path-to-Wow.exe> --output crates/rendering/tests/fixtures/native_model_bone_callbacks.txt
python tools/ghidra/model_scene_order_oracle.py <path-to-Wow.exe> --output crates/runtime/tests/fixtures/native_model_scene_order.txt
python tools/ghidra/model_bone_playback_oracle.py <path-to-Wow.exe> --output crates/runtime/tests/fixtures/native_model_bone_playback.txt
```

`model_default_sequence_oracle.py` executes `0x00834540` through its original
fallback, weighted selection, and timer constructors. Only old-scene removal
and CRT `rand` are replaced. Eight probes cover zero-weight variation zero,
nonzero variation metadata, animation 147 and first-record fallback, reverse
and held modes, and absent bones. Resources are already resident; asynchronous
load completion and gameplay request ordering are outside this probe.

```text
python tools/ghidra/model_default_sequence_oracle.py <path-to-Wow.exe> target/model-default-sequence-native.txt
```

## Initial transport map-model facing oracle

`transport_initial_oracle.py` executes `0x0070C310`, `0x004F4630`, quaternion
decoding/composition, and `0x004F42A0` for 656 synthetic packed rotations. The
parent quaternion and facing lookup providers return controlled resident data.
The fixture covers unparented and parented initial map handles; allocation,
resource admission, and map registration are outside this probe.

```text
python tools/ghidra/transport_initial_oracle.py <path-to-Wow.exe> crates/systems/tests/fixtures/transport-pose-native.txt target/transport-initial-native.txt
```

## Environmental damage and feedback oracles

`environmental_damage_oracle.py` executes the original `756800` packet receiver,
combat-log allocation/classification/formatting, predicted-health reducer and
unit-combat dispatch. Resident identity/name providers, allocation, Lua output
and visual submission are controlled boundaries. The JSON output also writes a
text fixture beside it. Visual kits are verified separately against installed
DBC/model assets and runtime scene tests.

`combat_classification_oracle.py` executes `74DCB0` and `715440`. Controlled
group, directed reaction and selection providers cover the flag branches;
owner resolution, GUID family tests and faction-table fallback execute native
code. It writes classification rows and a sibling `.factions.txt` fixture.

`environmental_tint_oracle.py` executes `7265C0`, `71A9A0`, `6ACC50` and
`720DB0` with resident model access. It captures special-effect 13's float
conversion, packed color, hold/fade boundaries and timestamp wraparound.

`environmental_sound_oracle.py` executes `6F9840`, `6F7B00`, `4C5990` and
the original attachment/scale chain. Resident model/unit/setting providers and
audio submission are controlled. Its 32 probes cover model readiness, the
kit suppression flag, local priority, listener centering and one-shot versus
entry-authored looping. Mixer tests cover retained handles and audible tails.
These tools map only the fingerprinted PE into Unicorn; no client or OS entry
point runs.

```text
python tools/ghidra/environmental_damage_oracle.py <path-to-Wow.exe> target/environmental_damage_native.json
python tools/ghidra/combat_classification_oracle.py <path-to-Wow.exe> target/combat_classification_native.txt
python tools/ghidra/environmental_tint_oracle.py <path-to-Wow.exe> target/environmental_tint_native.txt
python tools/ghidra/environmental_sound_oracle.py <path-to-Wow.exe> target/environmental_sound_native.txt
```

## Terrain specular lighting

The terrain lighting companion `terrain_specular_shader_oracle.py` renders 48
frames with the original Terrain/Terrain1 bytecode over a full flat MCNK grid.
It captures colored specular lighting at power 20, three diffuse BLP alpha
values, MCCV, shadow endpoints, two light directions, and the disabled branch.
The ADT/BLP Vulkan integration compares nine locations per captured frame.

```text
python tools/ghidra/terrain_specular_shader_oracle.py target/world-shadow-shaders crates/rendering/tests/fixtures/terrain_specular_shader_native.txt
```

## NPC virtual equipment

`npc_virtual_items_oracle.py` captures 1,536 settled Unit_C equipment cases.
Original code resolves Item.dbc entries, filters disarmed hands, classifies the
current body through AnimationData, adjusts off-hand readiness, selects components, and
chooses actual attachment links. The capture intercepts asset-cache and model
engine services; it ends `4EACD0` at the authored-link query and does not execute
full M2 creation or animated sheath callbacks. The renderer test also verifies
weapon/shield paths, textures, item visuals, and particle-color metadata.

```text
python tools/ghidra/npc_virtual_items_oracle.py <path-to-Wow.exe> crates/rendering/tests/fixtures/npc_virtual_items_native.txt
```

The input sheath value is the model's effective state. The separate
`npc_weapon_state_oracle.py` runs original `738180` and `736D30` for 10,368
ordinary non-local state transitions, including ranged latching, animation flags,
missing animation rows, attack targets, hand disarms, and template restrictions.
Body-engine queries and completed component relocation calls are intercepted;
spell providers, forced poses, and local-player behavior are excluded.

```text
python tools/ghidra/npc_weapon_state_oracle.py <path-to-Wow.exe> crates/rendering/tests/fixtures/npc_weapon_state_native.txt
```

## World UI scale

`ui_scale_oracle.py` executes 504 original FrameXML root-scale cases across
initialization, both CVar callbacks, seven viewport sizes, widescreen settings
and manual scale values. Root propagation and event delivery are intercepted;
the Rust scene tests separately exercise layout and pointer input.
See [world UI scale](../../docs/architecture/world-ui-scale.md).

```text
python tools/ghidra/ui_scale_oracle.py <path-to-Wow.exe> crates/ui/tests/fixtures/ui_scale_native.txt
```

## World file texture filtering

`world_texture_filter_oracle.py` captures 960 native filtering/cache-prefix cases.
It substitutes the device-capability query and stops before texture-cache lookup
or allocation. See [world texture sampling](../../docs/architecture/world-texture-sampling.md)
for the applicable Vulkan comparisons and the graphics-settings boundary.

```text
python tools/ghidra/world_texture_filter_oracle.py <path-to-Wow.exe> crates/rendering/tests/fixtures/world_texture_filter_native.txt
```

## WMO local batch visibility

The following capture executes the native corner transform and normal renderer
batch-selection loop, with GPU submission skipped after native acceptance.
[The visibility boundary](../../docs/architecture/world-model-batch-visibility.md)
records the covered instructions and remaining runtime integration.

```text
python tools/ghidra/world_model_batch_visibility_oracle.py <path-to-Wow.exe> crates/systems/tests/fixtures/world_scene_projection_native.txt target/world_model_local_frusta_native.txt target/world_model_batch_visibility_native.txt
```
