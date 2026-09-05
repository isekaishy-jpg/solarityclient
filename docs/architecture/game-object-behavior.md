# GameObject behavior evidence for dynamic collision

The shared GameObject scene owns visible resource generations, retained generic
behavior, and one CPU model timer shared with rendering. Generic transitions
consume progress in native notification order and complete through that timer's
scene callbacks, including while GPU placement is pending. The behavior also
retains the native collision flag and current M2 collision placement. Dynamic
MCNK/WMO reference registration and movement collection remain to be connected.

The build-12340 dynamic geometry callback is registered by `0x004FA5F0` through
`0x0077F2B0` as `0x004F6560` in `DAT_00CE04B0`. It resolves the exact GUID,
calls the object's eligibility virtual, requires the GameObject category bit,
and reads the source, bounds, scale, full matrix, and parent GUID. A nonzero
parent replaces the emitted face provenance. A visible loaded mesh alone does
not establish collision eligibility.

`0x0070F550` excludes door objects for query bit `0x8000`; otherwise it returns
the GameObject's retained collision flag at `+0x20C`. Initial/load admission
(`0x00712F30`, `0x00713F50`) also requires the behavior's eligibility virtual and
a strict positive extent on every bounds axis (`0x0070BD20`).

## Generic M2 state and door eligibility

Generic behavior initialization/field refresh `0x0070D600` reads the cached
GameObject state and the upper half of absolute update word 14,
`GAMEOBJECT_DYNAMIC`. That ushort is a sequence progress fraction; `0xFFFF`
means no supplied fraction. It is distinct from the byte in `GAMEOBJECT_BYTES_1`.
`GameObjectPresentation::sequence_progress` projects this ushort separately from
the BYTES_1 animation byte. `ActiveWorld::consume_game_object_sequence_progress`
sets it to `0xFFFF` in both the dense and typed views, retaining the low dynamic
flags and issuing no notification. Runtime dispatch reads live fields between
ordered notifications, so an earlier handler can consume progress before a later
handler compares its mirrored value.

| Replicated state | No supplied fraction | Supplied fraction |
| --- | --- | --- |
| 0 | Internal 3, Opened | Internal 2, Open |
| 1 | Internal 1, Closed | Internal 4, Close |
| 2 | Internal 6, Destroyed | Internal 5, Destroy |

The first eight animation IDs in native table `0x00ADA938` are
`145, 147, 148, 149, 146, 150, 151, 152`. They correspond to internal states
0 through 7. `0x0070D1E0` selects animations with model-dependent fallbacks,
including frozen endpoint sequences when authored open/close clips are missing.
It consumes supplied progress using the float constant at `0x00A339E0`
(`1.5259021893143654e-05`), starts at the resulting sequence offset, and records
an estimated wall-clock deadline. That deadline does not dispatch completion.
`GameObjectAnimationState` implements the initial, changed, and completed state
tables. The seek and reversal helpers preserve the native x87 arithmetic,
including the single-precision spill and ties-to-even integer conversion.

State notifications use a different table (`0x0070D690`) from initialization.
Closed-to-open always enters Opening, even without progress; open-to-closed
enters Closing. Closed-to-destroyed enters Destroying; destroyed-to-closed enters
Rebuilding unless supplied progress forces Closing. A progress notification
(`0x0070FD10`) reselects the current internal state's animation instead of
recomputing an initial state.

Completion callback `0x0070D7E0` advances internal 2 to 3, 4/7 to 1, and 5 to 6.
Stable states 1/3/6 re-enter sequence selection. `0x0070D510` handles animation
reversal for internal 2/4 and 5/7 transitions when progress is absent. The door
override `0x0070D8D0` sets collision eligibility only when internal state is 1;
its initial eligibility virtual `0x00712550` uses the same predicate. A closing
door with supplied progress is therefore not solid until completion reaches 1.
Checking only replicated state byte 1 would make it solid too soon.

Collision eligibility is retained independently of geometry. Model admission
writes the strict positive-extent result only when the behavior's initial
eligibility virtual allows it. Door state virtual `0x0070D8D0` writes the
Closed predicate after the generic setter, even when the state was unchanged or
the collision box has zero extent. Later matrix updates do not recompute this
flag. Runtime retains this ordering and exposes the flag separately from model
and spatial residency; it is not yet a complete movement admission result.

`DecodedM2Model::collision_bounds` preserves header +0xBC even when the model
contains no collision faces. `PlacedM2Collision` retains this transformed box,
the separate +0xA0 render box, and the transformed collision-box center. Dynamic
registration uses the render box for overlap and the collision center for its
floor probe. Model-box transforms now execute the axis-product accumulation
and float spills from `0x007F9430` / `0x00984860`, also used by static M2/WMO
placements. Moving a retained M2 updates its matrices and boxes without
re-decoding its source or restarting playback.

`tools/ghidra/game_object_collision_oracle.py` reproduces 16 exact box-transform
cases and 264 collision-flag cases from the fingerprinted original executable.
The latter execute model admission `0x00712F30`, the door setter, and query-mask
checks; external presentation/resource operations are controlled inputs. The
door setter fixtures deliberately have no active model, so they do not prove
timer behavior. Separate live CPU integration tests verify opening/closing
completion, motion, absent collision faces, zero-extent admission, and model
reload against the retained owner. These checks do not yet establish native
floor-probe selection, spatial reference ordering, or complete player movement.

## Model callbacks, metadata, and pause clocks

`0x00712F30` installs callback `0x0070CB10` through `CM2Model::0x00823FE0`.
Ordinary completion enters behavior virtual `+0x48` (`0x0070D7E0`); interruption
enters `+0x44` (`0x0070C430`). The callback clears the last-started state before
selecting the next state or restarting a stable state.

The model scan `0x00832260` queues looping callbacks at the final millisecond
of each authored duration and terminal callbacks at the primary timer's end.
An already overdue terminal timer still receives its first callback. Held or
zero-duration sequences do not receive this scan's callbacks. `0x00831FC0`
marks terminal timers finished before invoking the user callback. Automatic
weighted variation selection follows only if the callback leaves the primary
sequence index and start tick unchanged, the sequence loops, and variations
are enabled. Stable GameObjects therefore still receive callbacks when a model
has only one sequence. A behavior callback starts its replacement at the current
scene tick, without the outside-update one-millisecond adjustment or automatic
variation's carried overdue offset.

Progress metadata comes from `0x0082CED0` with variation ordinal zero. The asset
API `model_animation_duration_ms` applies Model fallback and reads the authored
lookup head's duration. It does not choose the weighted playback variation,
search for variation-ID zero, or follow an alias to its track payload duration.
External payload availability does not affect this metadata lookup.

`GAMEOBJECT_FLAGS & 0x80` pauses only transition states 2/4/5/7. Sequence setup
and the flags notification (`0x0070D160`) store the current scene tick, or 1 when
that tick is zero, in model `+0x64`. A nonzero value suppresses callback scanning.
Pose update `0x0082F0F0` advances that pause marker and shifts both primary and
secondary timer start/end ticks by the elapsed pause duration. The global scene
clock and secondary blend envelope continue advancing. The shared runtime model
owner now applies these shifts and invokes the generic completion handler.

## Field notification order

`0x004D73A0` processes the entire update packet in two passes. `0x004D7050`
applies raw creates/values first, then the packet cursor is restored and
`0x004D7100` dispatches post-initialization and field notifications. This is not
an immediate callback after each individual raw word or update block.

`0x004D53C0` saves watched old values in the mirror and applies the raw update.
The notification pass (`0x004D5550`) walks update words in ascending order and
calls `0x004D5150` for touched words. That function compares the watched byte
range in the current raw fields against its mirror at dispatch time. Handler
flag `+0x2E` skips the comparison. `0x007140A0` registers the GameObject state
handler with this flag set; the flags and sequence-progress handlers compare
their watched values normally.

Consequently flags (absolute word 9) precede sequence progress (upper word 14),
which precedes state (byte zero of word 17). Progress consumption by one handler
is visible to the later state handler. `0x00711050` additionally compares the
current replicated state with cached state `+0x204`, except for type 15, which
always enters the state virtual. Multiple update blocks and packets must retain
their notification boundaries and the mirror values used for comparison. A
renderer-only comparison of the final per-frame presentation cannot reproduce
this behavior or its random-number consumption.

`0x00710460` is a separate behavior controlling a WMO handle and passenger
detachment. It selects Close/Open (146/148), changes the retained collision flag,
and updates the WMO collision state. It is not evidence for a generic M2
stable-state selector. Animated transport types 7/11 additionally require their
path/animation clocks; the generic table cannot substitute for those providers.

## Runtime integration and remaining work

`GameObjectAnimationRequest` implements `0x0070D1E0`'s authored-presence checks
and missing Open/Close substitutions before `AnimationData` fallback. Presence
uses `0x00825E00` semantics independently of external payload availability.
Missing stable poses can select a frozen authored transition clip. The selector
also preserves an already active request on the native missing-clip branch;
the comparison uses the requested ID before CM2Model's DBC fallback.

Runtime shares the existing `AnimationDataCatalog` and CM2Model primary timer
implementation with the Model widget path. New GameObject requests consume a
weighted-variation roll followed by the cycle-count roll, even when variation
zero exists. Unchanged replicated state preserves its timer and RNG position.
Both native entry points validate the primary bone before sequence setup;
bone-less or sequence-less models retain static geometry without an invented
animation selection. Frozen substitutions hold the authored initial pose.

`native_game_object_animation_requests.txt` records 2,048 executions of the
original selector across every subset of animation IDs 145–152 and all eight
generic internal states, plus a second call with the selected request already
active. Native authored lookup runs unchanged. The harness supplies the current
request, sequence metadata, and final application endpoint; it does not test
completion deadlines or transition progress. Portable tests compare requests,
frozen state, and preservation decisions. Runtime archive tests additionally
check DBC fallback, timer offsets, held endpoints, and RNG order.

`game-object-transition-native.txt` adds 232 original-code cases for the initial
and changed state tables, progress offsets, and reversed fractions. Field tests
verify consumed fractions remain consumed after unrelated sparse updates.
`native_sequence_completions.txt` adds 1,080 original `0x00832260` scan cases for
looping and terminal sequences, seeks, held speed, paused/finished owners,
zero/one-millisecond durations, overdue timers, and scene-tick wrap. Its harness
isolates authored-event enumeration and queue insertion; it does not test the
GameObject user callback or the full scene traversal. Rendering tests separately
check that pause shifts preserve pose while advancing the blend envelope.

Shared resource residency and fresh object lifetimes are implemented in
[GameObject placement](game-object-placement.md). `GameObjectBehavior` retains
the cached replicated state, internal state, pre-fallback request, and shared
`M2Playback` timer for each exact object lifetime. CPU model completion attaches
the timer independently of GPU placement. Packet admission runs a raw pass and
then ordered notifications before admitting the next packet. Progress writes
update both raw and typed ECS fields. A display change retires the old model
before a subsequent state or progress notification can animate it.

The scene advances every loaded generic object in retained object order before
the GPU placement traversal, including objects with an unresolved placement.
It publishes the pose clock, expired sequence tails, and current event interval
for rendering to consume once. Completion changes the internal state returned
by `RuntimeGameObjectPresentation::animation_state` and the door's retained
collision flag. `collision_eligible` checks that flag against the query mask
and the current ECS type byte for the exact object lifetime. Dynamic collision
registration is not yet connected to that boundary.

The generic owner is restricted to the constructor families that enter native
`0x007124B0`: types 0–3, 5–6, 8–10, 12, 16–19, 22–27, 29–30, and 34. Other
families retain the previous stable presentation path while their specialized
providers remain unimplemented. WMO state, path transport clocks, reference-GUID
interruption, and actor flags controlling other startup actions still require
their own native behavior integration. Scene advancement currently begins with
world rendering; traversal order relative to other model families and callbacks
during loading still need verification. Events from an object without drawable
placement are not retained for later spatial dispatch.

Runtime tests exercise CPU-only transition completion, single-sequence stable
restarts, shared timer retention, pause/resume, reversal, boneless progress
consumption, progress-before-state decisions, and display replacement. An
encrypted packet fixture checks whole-packet raw admission, repeated-block
mirrors, ascending field handlers, live progress comparison, and state's
always-notify registration. These integration tests supplement the native
function fixtures; they do not establish full native scene traversal parity.

Dynamic references also belong in native MCNK and WMO-group collection order:
`0x007A5A60` reaches the chunk's dynamic list through `0x007A5240` after its
terrain faces and MDDF references. Appending every GameObject after all static
geometry would not preserve candidate order. Reference registration,
alternate/destructible resources, and the general WMO root registration
lifecycle remain work for that owner.

## Spatial registration evidence still to integrate

GameObject map-owner creation uses flags `0xB` at `0x00781A10`, selecting the
special registration path `0x007C2E70` through `0x007C2F80`. That path probes
from the collision center plus four Z units (capped by render maximum Z plus
0.1) down to the collision center minus 1,000. Its separate group-containment
point is the collision center plus 0.15 Z. The probe combines terrain height,
WMO BSP faces, and portal crossings; camera ray eligibility is a different query.

`0x007C25D0` filters with root MOGI group flags (`0x007AE7B0`), while
`0x007C1DC0` derives the selected interior bit from loaded MOGP flags. These
two flag sources must remain distinct. The latter samples BSP faces through
`0x007CB260`, then tests interior portals through `0x007AF520`; a qualifying
portal can choose a neighboring group even when there is no floor face. The
current asset boundary retains MOGI boxes and MOGP flags but discards MOGI
flags and the root MOPV/MOPT/MOPR tables. Those tables must be retained before
this registration probe can be implemented completely.

An interior result registers the chosen group first and overlapping eligible
interior groups in the same root (`0x007C2D30`), without terrain references.
An exterior result visits overlapping exterior WMO groups (`0x007C2BF0`) and
loaded MCNKs (`0x007C2040`). The MCNK loop also requires chunk minimum Z to be
at or below render maximum Z. Dynamic references enter the site's front list
through `0x007B5020`, so registration and re-registration order affect later
first-visit collection. Replacing this process with a final all-object overlap
pass would lose both membership and candidate order.
