# Replicated entity opacity

This work belongs to the open entity-fading row in
[world completion](world-completion.md). Replicated object opacity has a
different owner and policy from [static scenery distance fading](scenery-distance.md).
Entry interpolation is connected; detached disappearance and exceptional owner
policies remain in progress. This document does not close the reported gap.

## Native contract

The fingerprinted build-12340 executable owns entry opacity in `CObject` bytes
`+C8` (current), `+C9` (from), `+CA` (target), and `+CB` (independent multiplier),
with the clock and duration at `+C0/+C4`. `744030` quantizes the selected float
to a byte before comparing it with current opacity. Equal-current selection
cancels interpolation; zero duration publishes immediately. `743E10` and the
unit override `71AC30` use signed wrapping integer time and truncated byte
interpolation, then publish both byte factors through the stored float reciprocal
of 255 squared. This update precedes camera visibility rejection.

`744A50` selects target alpha and either 1000 or zero milliseconds when the
model is admitted. The unit target comes from the signed
`CreatureDisplayInfo.ModelAlpha` column (`715B50`); missing display data yields
one. `716650` rejects primary flag `0x2`, secondary flag `0x20`, and `Bytes1`
bit `0x00020000`. A unit transport additionally requires a resolved parent with
no active opacity transition, unless its `VehicleSeat.Flags` sign bit permits
entry interpolation. A GameObject transport does not use that unit-parent gate.
`73FCC0` finishes the initial transition for the resolved Birth behavior (127).

GameObject virtuals `70B9E0/70B9A0` delegate to the selected behavior. Generic
Spawn state skips entry interpolation; the specialized families allow it. The
target is one except for FishingHole (type 31): `70DD50` returns one when dynamic
bit `0x2` is set and one half otherwise. These are model-admission decisions;
repeated frame preparation does not retarget the same admitted display.

On disappearance, `743D50` can transfer a model scene hierarchy to `783630`,
retaining the primary byte divided by 255 independently of `+CB`. Unattached
roots receive a final pose update before this transfer.
Admission requires a ready model, initial opacity at least 0.01, and no scene
flag `0x4`. `782F20` keeps it until signed elapsed time exceeds 2000 ms, or
recursive model readiness fails. Its scalar is
`smoothstep(clamp((1 - elapsed * float(0.0005)) * initialOpacity, 0, 1))`.
The exact 2000-ms endpoint remains resident. A surviving GameObject transport
updates the saved relative transform; losing that parent freezes the last
transform. The live gameplay object itself is not retained by this list.

## Implemented consumers

`solarity_systems::EntityOpacity` reproduces the byte state, native entry gate,
and model scalar. `EntityRetirement` reproduces the detached time envelope.
The runtime unit animation owner retains one `EntityOpacityOwner` across model,
material and GPU generation changes for the same `WorldObjectIdentity`; a new
identity gets independent state. The unit scene advances this state before
culling. Model admission reads the target and eligibility once per display.

Character publication shares that owner with the body, mount, equipment and
enchant models. The M2 frame multiplies the scalar into mesh materials,
particles, ribbons, authored lights and the existing alpha-based shadow path.
It uses the existing translucent pipeline selection for opaque material batches
during a fade. GameObject M2 instances own and advance the same state through
their scene update. WMO GameObjects retain their separate renderer path.

## Evidence and remaining work

`tools/ghidra/entity_opacity_oracle.py` executes the pinned native routines under
Unicorn. Its checked-in capture metadata identifies the executable and fixture
hashes. The systems tests compare 1,189 setter/update cases against **both**
native owner updates, 352 retirement samples, and 560 entry eligibility cases.
All 2,101 native cases pass, including wrapped clocks, target quantization,
missing models/parents, exact endpoints and the vehicle-seat exception.

Runtime checks pass for model replacement, GUID reuse, initial versus later
Birth animations, FishingHole target retention/reselection, and shared equipment
and enchant opacity through material replacement. The equipment test also
verifies the scalar reaches GPU mesh material packets. The complete runtime
library suite passes (263 tests; 18 archive-dependent tests ignored). The full
locked workspace suite passes (1,215 tests; 23 ignored), as does workspace
Clippy for all targets with warnings denied. After replacing fixture-test
unwraps with explicit diagnostics and optional-value comparisons, the three
native opacity tests were rerun successfully. The pure
retirement envelope is not yet a runtime disappearance consumer. Remaining
integration and validation are:

- Transfer removed model hierarchies into independent scene lifetimes, preserving
  animation, attachments, effects and transport transforms without GUID reuse
  collisions. Verify the native unit removal-visibility exclusions and lighting
  callback handoff before treating retirement as complete.
- Project Vehicle/VehicleSeat presentation into the unit owner; the exact
  native seat policy is tested but the runtime currently has no seat-record input.
- Connect the camera/vehicle `+CB` multiplier and specialized visual-kit model
  ownership. These are separate from far-distance scenery fading.
- Verify when native model readiness starts interpolation relative to asynchronous
  resource publication, and compare combined appearance/disappearance during
  populated-world travel. CPU arithmetic and controlled packet tests do not
  establish that live visual result.
- Trace any additional replicated-distance or effect-owner policies from their
  native callers instead of borrowing static MDDF/MODD distance classes.

## Testing package

Build **000087** (`0.0.3a`) installs source revision
`45658f53e403ba107abbe774fd93d238a1896e52` through the persistent Testing launcher.
The executable reports that revision and build number; its dirty marker records
the packaging reservation in `BUILD_NUMBER`. Installed and compiled executable
SHA-256 hashes match:
`9c02c02ce453663f70cf5684cccebc5ab6db54af4cb7c1118314cbcf72c6b093`.
This package contains the entry consumers above; it does not establish live
populated-world appearance or implement the remaining disappearance consumer.
