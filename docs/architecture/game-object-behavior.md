# GameObject behavior evidence for dynamic collision

The shared GameObject scene now owns visible resource generations and independent
renderer playback. Stable generic states request Closed/Opened/Destroyed poses
(147/149/151), apply the model-dependent selector below, and use the native
CM2Model timer backend. Transition progress, completion, and collision eligibility
still require the retained behavior owner described here.

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
The fraction currently remains in the dense field table and needs a typed
consumer that preserves field-update events and native consumption semantics.

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
(`1.5259021893143654e-05`), starts at the resulting sequence offset, and sets a
completion clock. It also handles the animation clock flag in `GAMEOBJECT_FLAGS`.
This requires actual model sequence metadata and the retained playback clock.

Completion callback `0x0070D7E0` advances internal 2 to 3, 4/7 to 1, and 5 to 6.
Stable states 1/3/6 re-enter sequence selection. `0x0070D510` handles animation
reversal for internal 2/4 and 5/7 transitions when progress is absent. The door
override `0x0070D8D0` sets collision eligibility only when internal state is 1;
its initial eligibility virtual `0x00712550` uses the same predicate. A closing
door with supplied progress is therefore not solid until completion reaches 1.
Checking only replicated state byte 1 would make it solid too soon.

`0x00710460` is a separate behavior controlling a WMO handle and passenger
detachment. It selects Close/Open (146/148), changes the retained collision flag,
and updates the WMO collision state. It is not evidence for a generic M2
stable-state selector. Animated transport types 7/11 additionally require their
path/animation clocks; the generic table cannot substitute for those providers.

## Integration still required

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

Shared resource residency and fresh object lifetimes are implemented in
[GameObject placement](game-object-placement.md). Retained behavior and animation
state must next drive both presentation and collision eligibility, including
supplied progress, reversals, completion callbacks, and behavior-specific clocks.

Dynamic references also belong in native MCNK and WMO-group collection order:
`0x007A5A60` reaches the chunk's dynamic list through `0x007A5240` after its
terrain faces and MDDF references. Appending every GameObject after all static
geometry would not preserve candidate order. Reference registration, moving
bounds, disabled states, alternate/destructible resources, and the general WMO
root registration lifecycle remain work for that owner.
