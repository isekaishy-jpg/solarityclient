# Replicated entity opacity

This work belongs to the open entity-fading row in
[world completion](world-completion.md). Replicated object opacity has a
different owner and policy from [static scenery distance fading](scenery-distance.md).
Entry interpolation, ordinary detached disappearance and ordinary camera-subject fading are connected;
exceptional owner policies and combined travel validation remain in progress.
This document does not close the reported gap.

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
no active opacity transition, unless its `VehicleSeat.AttachmentID` is negative.
`5D3340/756EC0` resolve the parent's vehicle row and passenger seat byte;
`716650` tests the signed word at seat row `+8`, not the flags word at `+4`.
The earlier pure-policy fixture labels that word `seat_flags`; its numerical
capture remains valid, and the typed consumer now names it correctly.
A GameObject transport does not use that unit-parent gate.
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

The ordinary scene created by `743760 -> 781A10 -> 7C0670` retains vtable
`A3FD90`. The model's light callback (`+2AC/+2B0`) points to `780CD0` and that
scene; it delegates to `7C1730` and the projected-shadow callback `7C10C0`.
`783630` replaces only the scene's visibility callback (`+90/+94`) with
`7823D0` and the scene itself. It preserves the spatial lighting state, scene
category flags, and model callbacks. The different `A40318` scene family and
its `7C1150` callback are not this ordinary CObject allocation path.

## Implemented consumers

`player_camera_opacity` implements the ordinary `606F90` camera branch. Its
distance comes from `605D60`'s constrained output before water eye correction;
the principal height and pitch remain separate from the final camera pivot
tilt. After subtracting the near clip, the ordinary interval is
`0.002777777845..1.831500172615`. A steep downward principal pitch can enlarge
the upper bound when distance is below subject height. `8CA080` supplies cosine
interpolation and `607991` explicitly selects truncation for the output byte.

The shared runtime camera composition publishes that byte to the followed
player's `+CB` owner before model preparation. It applies to the body's shared
mount, equipment and enchant hierarchy without changing primary entry opacity
or affecting other units. A weak subject reference restores the old multiplier
on subject changes or loss, following `6066E0`, without retaining a removed unit.
The settled far-camera path returns immediately without a cosine calculation.

The ordinary Player_C visibility override is also connected. `6DE980` hides
players with `PlayerFlags & 0x80000` when either `0x400000` is set or the current
Map.dbc instance type is Arena (4). Missing map rows take the non-arena branch.
`6E0840` additionally hides the local player when `6CEE50`'s camera-visible
global is zero. These flags remain independent of entry and camera alpha.
`4F8D10` updates unit state and placement before clearing root/attached model
activity. The runtime retains unit updates and attachment samples, then skips
mesh, shadow, authored-light and model-effect publication for hidden hierarchies.
Independent spell/effect owners retain their separate visibility policies.

`714C40` invokes the same visibility override for `743D50`'s removal decision.
Removal therefore records visibility alongside the primary opacity. Releasing
the camera afterward cannot make a previously hidden removed hierarchy appear
in the two-second retirement list. A partially transparent, visible subject
still transfers its primary byte independently of the camera multiplier.

`solarity_systems::EntityOpacity` reproduces the byte state, native entry gate,
and model scalar. `EntityRetirement` reproduces the detached time envelope.
The runtime unit animation owner retains one `EntityOpacityOwner` across model,
material and GPU generation changes for the same `WorldObjectIdentity`; a new
identity gets independent state. The unit scene advances this state before
culling. Model admission reads the target and eligibility once per display.

The runtime now supplies the resolved seat attachment to this admission rule.
The flag-`0x80` create payload retains Vehicle.dbc ID and initial facing in a
unit-owned ECS component, including zero and unresolved IDs. `757FA0` seeds
facing; `7580F0` replaces the row while preserving that angle. Updates without
the payload leave the owner intact, duplicate remote creates remain ignored,
and removal/GUID reuse ends its lifetime. `ClientServices` loads both exact
vehicle tables through the normal asset stack and shares the catalog with
presentation. Seat lookup occurs at model admission, not on every settled frame.
See [vehicle presentation](vehicle-presentation.md) for the remaining movement,
attachment, camera and visibility consumers.

Character publication shares that owner with the body, mount, equipment and
enchant models. The M2 frame multiplies the scalar into mesh materials,
particles, ribbons, authored lights and the existing alpha-based shadow path.
It uses the existing translucent pipeline selection for opaque material batches
during a fade. GameObject M2 instances own and advance the same state through
their scene update. WMO GameObjects retain their separate renderer path.

Removal now marks the exact CPU owner before it leaves the live identity table.
M2 publication transfers its ready placements into a separate retirement group.
Each model receives a group/member key; mounts, riders, equipment and enchant
attachments resolve through those keys instead of a live GUID. Display or
material replacement within a continuing identity does not trigger retirement.
An initial scalar below 0.01 does not enter the detached list.

Retirement copies model timers, variation/event intervals, body-bone overrides,
held-hand pose and wound blend state. It releases the unit animation callback
owner and retains GPU sources, particle/ribbon histories and lighting state.
The saved unit or GameObject lighting category continues its ordinary floor
callback; children inherit their retired parent receiver. Primary unit shadow
admission retains the original category and uses the existing native material
opacity cutoff. This does not implement the separate projected-shadow renderer.

The group advances the native two-second envelope before culling and follows a
surviving GameObject's passenger matrix. After parent loss, it cannot reacquire
that GUID. Expiry or a missing child source removes the complete hierarchy and
compacts unreferenced sources. Cached retired-model indices bound settled
readiness work to detached models, rather than scanning scenery per group.
New GameObjects can reuse authored GPU sources held by retired GameObjects;
character material domains remain separate.
World transfer drops the entire terrain frame and its retired scene lifetimes.

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
library suite passed for the entry package (263 tests; 18 archive-dependent tests ignored). The full
locked workspace suite passes (1,215 tests; 23 ignored), as does workspace
Clippy for all targets with warnings denied. After replacing fixture-test
unwraps with explicit diagnostics and optional-value comparisons, the three
native opacity tests were rerun successfully.

The disappearance checks exercise immediate GUID reuse with eight attached
equipment/enchant models, retained mount/rider transforms, preserved effect
histories, the exact retirement endpoint, recursive readiness failure,
below-threshold admission, and transport movement/loss. They also verify that
the old unit animation owner is released. The runtime suite passes with these
three new checks (266 tests, 18 ignored). Existing GPU fixtures also pass for
interior lighting retention and the retired unit shadow cutoff. The complete
locked workspace suite passes (1,218 tests, 23 ignored), followed by all three
retirement scene tests after extending authored-source reuse coverage. Workspace
Clippy checks all targets with warnings denied.

The ordinary camera fragment has 1,864 checked-in native cases covering near
and far endpoints, steep downward pitch, principal height and varied near clip.
All byte results match. Runtime checks verify previous-subject restoration
without altering primary entry timing or retaining removed owners, and actual
GPU material opacity for the local player and eight attached models through
material replacement. Other units retain their opacity. With this camera slice,
the locked workspace suite passes (1,221 tests; 23 ignored), and workspace
Clippy passes for all targets with warnings denied. This does not establish
live populated-world camera/travel presentation or the overall FPS target.

The player override adds 1,152 native comparisons that execute `6E0840`,
`6DE980`, ordinary `730F30/743300`, and `714C40`. They cover local/remote identity,
camera visibility, both player flag masks, arena/non-arena/missing map records,
scene culling and retirement eligibility. All pass. The runtime suite passes
(269 tests; 18 ignored), including hidden/resumed equipment effect clocks,
map/flag changes without model replacement, hidden removal with no reappearance,
camera-release snapshot stability, and suppression of a fully opaque shadow caster.
The runtime all-targets check and workspace all-targets Clippy with warnings
denied also pass for this visibility slice.

Remaining integration and validation are:

- Project the exceptional `730F30` unit removal-visibility inputs: special
  visibility/model-publication state, hidden-model child activation, vehicle flags and the
  alternate effect owner. Ordinary unit retirement is connected; these missing
  presentation systems are not replaced by guessed field mappings. The native
  scene `+7C` flag `0x4` exclusion also has no current ordinary-scene producer.
- Complete vehicle passenger movement/attachment publication and the remaining
  camera/visibility consumers. Vehicle creation state and the seat-record input
  to entry interpolation are connected; this does not establish vehicle travel parity.
- Extend the ordinary camera `+CB` consumer to vehicle bounds/parent dispatch,
  timed camera modes and the barber-shop reduced-range `BD19B8` global.
  The Player_C `A30/18B8` publication override and specialized visual-kit ownership also remain.
  These are separate from far-distance scenery fading.
- Verify when native model readiness starts interpolation relative to asynchronous
  resource publication, final removal-pose timing between movement publication
  and scene transfer, and combined appearance/disappearance during
  populated-world travel. CPU arithmetic and controlled packet tests do not
  establish that live visual result.
- Trace any additional replicated-distance or effect-owner policies from their
  native callers instead of borrowing static MDDF/MODD distance classes.

## Testing packages

Build **000090** (`0.0.3a`) installs source revision
`a28310ae55d9c864a5bafeca46fd57c04236b70b` through the persistent Testing launcher.
The executable reports that revision and build number; its dirty marker records
the packaging reservation in `BUILD_NUMBER`. Installed and compiled executable
SHA-256 hashes match:
`5305ce404c418f12d17a1f7ab5e05cb66667654ff5047b2184a3ccda89163edc`.
It connects ordinary Player_C visibility to model, effect and shadow publication
and excludes hidden removed player hierarchies from disappearance fading.
The exceptional policies and live travel checks listed above remain open.

Build **000089** (`0.0.3a`) installed source revision
`f0c1ee366cd39619e64656726b084234a4f45d9d` through the persistent Testing launcher.
The executable reports that revision and build number; its dirty marker records
the packaging reservation in `BUILD_NUMBER`. Installed and compiled executable
SHA-256 hashes match:
`2eb9e2e6c58514d0ab82c6c996f2675f048975ea22ae407acf79931463f15d96`.
It adds ordinary close-camera fading for the followed player's shared model
hierarchy and restores opacity when the camera releases that subject. Vehicle,
special camera and visibility policies and live travel checks remain open.

Build **000088** (`0.0.3a`) installed source revision
`986c8f023765043fada4084f011df800ca2620d0` through the persistent Testing launcher.
The executable reports that revision and build number; its dirty marker records
the packaging reservation in `BUILD_NUMBER`. Installed and compiled executable
SHA-256 hashes match:
`d99ad5be83111c295d2a2ab6457df8097269959bbf17642e1cabeaff5bc1a827`.
It adds ordinary disappearance, independent attachment lifetimes, retained
lighting/shadow consumers and authored-source reuse. The exceptional policies
and live travel validation listed above remain open.

Build **000087** (`0.0.3a`) installed source revision
`45658f53e403ba107abbe774fd93d238a1896e52` through the persistent Testing launcher.
The executable reports that revision and build number; its dirty marker records
the packaging reservation in `BUILD_NUMBER`. Installed and compiled executable
SHA-256 hashes match:
`9c02c02ce453663f70cf5684cccebc5ab6db54af4cb7c1118314cbcf72c6b093`.
That earlier package contained the entry consumers, before the disappearance
consumer was connected.
