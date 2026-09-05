# Input control ownership

The runtime owns one main-thread `InputControl` between SDL event translation
and all UI, binding, camera, and player-control consumers. Polling an event now
commits its physical state before the event is returned or interpreted. No
downstream system polls SDL independently.

## Stock evidence

The fingerprinted build-12340 executable contains the connected source names
`InputControl.cpp` and `InputControl.h`, imports `DirectInput8Create`, and keeps
the movement/camera command family together: `MoveForwardStart/Stop`,
`TurnLeftStart/Stop`, `StrafeLeftStart/Stop`, `PitchUpStart/Stop`, camera view
movement, zoom, mouselook, and vehicle aim. This supports one raw-input owner
feeding later named-command resolution rather than placing key state in the
player, UI, or renderer.

`InputBindingRouter` is the next boundary after retained state. It converts a
known physical location to the exact stock chord text, looks that chord up in
the selected assignment set, and returns the authored UI action. It does not
hard-code a gameplay command or synthesize a binding when FrameXML has not
declared one.

## State and frame behavior

- Only events for the primary client window enter keyboard and pointer state.
- Physical scancodes, not layout-dependent keycodes, own held-key identity.
- Repeated key-down events remain available to UI/binding dispatch but do not
  create another held transition or revision.
- Focus loss and application backgrounding release keys, mouse buttons, and
  modifiers without fabricating platform events.
- Absolute pointer position persists; relative motion and wheel values
  accumulate until one consumer frame takes them.
- SDL's known flipped wheel direction is normalized. An unknown later direction
  is not guessed into the stock convention.

The held-key hash table reserves a small process-lifetime capacity and is
cleared without being reallocated. Mouse buttons use a fixed mask corresponding
to FrameXML `BUTTON1` through `BUTTON5`; unknown buttons are not substituted.

FrameXML reads one side-specific projection of the retained modifier mask.
`IsShiftKeyDown`, `IsControlKeyDown`, and `IsAltKeyDown` combine their left and
right physical keys; the six side-specific native queries read those exact
bits. The all-released initial image is authoritative before the first input
event, and focus loss publishes that same cleared image.

## Binding transitions

Keyboard chords use physical SDL scancodes internally but expose no SDL type.
Generic modifiers serialize in the stock `ALT-CTRL-SHIFT` order. Key names,
mouse `BUTTON1` through `BUTTON5`, and vertical `MOUSEWHEELUP`/
`MOUSEWHEELDOWN` names are assembled in a fixed stack buffer and probe the
assignment hash table without allocating.

Repeated key-down events do not execute a second binding transition. A named
command is admitted only when the catalog contains its declaration and its
platform gate permits Windows. Dynamic spell, item, macro, and click actions
remain typed assignments and do not pretend to own a binding declaration.

A `runOnUp` press retains the compact assignment index against the physical
control. Release therefore invokes the exact pressed action even if Control,
Shift, or Alt was released first. Focus loss and application backgrounding
drain all such releases in press order, preventing movement or camera commands
from sticking. Wheel actions have no fabricated release.

The locally installed build-12340 profile validates 147 Windows-routable
default chords through this physical translation. Six serialized defaults are
correctly inactive at this catalog boundary: four are Mac-only movie commands,
and two name commands not declared by the currently loaded binding documents.
No Windows fallback is substituted for them.

## Gameplay command boundary under implementation

Physical binding delivery is present; player locomotion and camera command
consumers are still absent. The following build-12340 executable evidence
bounds the next implementation. These addresses come from the original Lua
registration tables and their native callees, not the C++ client's controller.

| Authored Lua call | Native entry | Immediate owner |
| --- | --- | --- |
| `MoveForwardStart` / `MoveForwardStop` | `0x005FC200` / `0x005FC250` | InputControl held bit `0x10` |
| `TurnLeftStart` / `TurnLeftStop` | `0x005FC320` / `0x005FC360` | InputControl held bit `0x100` |
| `MouselookStart` / `MouselookStop` | `0x005FCC10` / `0x005FC890` | InputControl held bit `1`, pointer/camera lifetime |
| `CameraZoomIn` / `CameraZoomOut` | `0x006017E0` / `0x00601840` | Persistent camera's timed zoom state |

`0x005FA170` and `0x005FA450` admit held-bit edges and suppress repeated
starts/stops. Their camera, cursor, and autorun side effects are part of that
boundary. Movement Lua entries first pass the native protected-action check
at `0x005191C0`; arbitrary script execution must not acquire hardware-input
authority merely because it calls the same function.

`0x005FBBC0` resolves the current controlled unit and separately gates
translation and turning. The ordinary forward/back resolver at `0x005FAE70`
adds autorun, forward, and the paired mouse-button forward contribution, then
subtracts backward. It emits no new transition when the resulting direction
is unchanged. Strafe resolution at `0x005FAFB0` incorporates turn keys during
mouselook; turn resolution at `0x005FB0B0` owns the corresponding suppression.
These held-command masks are distinct from the unit's network movement flags.

The unit wrappers `0x0072E5D0`, `0x0072E680`, and `0x0072E7E0` feed timed
movement events. Their ordinary event IDs are forward/back/stop `0/1/2`,
strafe-left/right/stop `3/4/5`, and turn-left/right/stop `11/12/13`.
`0x006ECB50`, `0x006ECBB0`, `0x006ECDE0`, `0x006ECE40`, `0x006F0F70`, and
`0x006ECEA0` establish this mapping. These are internal event IDs, not packet
opcodes. Event admission uses `0x006EBC70` and `0x006EC090`; `0x007B5020`
links the movement owner into an intrusive work list. It does not itself
integrate or dispatch movement. The due-event consumer at `0x006EF860`
changes movement state before invoking the unit packet/event path at
`0x007413F0`.

Zoom also requires an update owner: the Lua entries default an absent numeric
argument to one, then `0x005FF950` / `0x005FFA60` derive timed operations from
the camera speed CVar. `0x005FE580` retains direction, start, deadline, and
rate. Directly incrementing the saved distance per wheel event would omit
that native behavior.

Remaining recovery and implementation work includes the movement event
consumer and wire snapshot ordering, local displacement/collision and falling,
server corrections and control changes, timed camera input, and the unit
animation consumer. None of those capabilities is implied by successful
binding dispatch or the now-available world-transfer pipeline.

### Retained movement snapshot

Living object updates now retain the full conditional `MovementInfo` context
through network decoding and ECS projection: the sender timestamp, transport
GUID/relative XYZ/facing/clock/signed seat/optional second clock, conditional
pitch, unconditional fall time, conditional launch values, and spline
elevation. Previously those fields were skipped after the flags and world
transform were read. The existing nine movement speeds remain separate.

The original writer is `0x004F4ED0`; `0x00987140` supplies its snapshot and
`0x00987E30` establishes the direction basis. Falling direction is written as
**cosine then sine**, following vertical launch speed and preceding horizontal
launch speed. Those direction and velocity values describe the launch; fall
time is a separate field and does not replace them with live apex velocity.

Optional presence remains explicit. The second clock requires both transport
and interpolation flags; an interpolation bit alone consumes no extra field.
Pitch is present for swimming, flying, or the secondary always-pitching bit.
FALLING_FAR alone does not introduce the four-float FALLING section. A later
living update replaces the whole context, so old transport and launch values
cannot leak into a new snapshot after their fields disappear.

Authenticated TCP regressions drive 49 field combinations through the real
gameplay pump into ECS, including wrapping timestamps, negative seats, absent
versus present-zero fields, and preservation of opaque high flags. A second
encrypted-session regression truncates a fully populated block at every byte
and then receives a valid packet, proving malformed context cannot read into
the next packet. These tests establish retention and framing; they do not
establish local movement or spline-path simulation.

### Outgoing event snapshots

The network boundary now encodes the 25 ordinary movement event envelopes
used by `0x006EF860`, `0x006F09F0`, `0x006E9380`, `0x006EB0B0`, and
`0x0073D4A0`: translation, strafe, turn, pitch, jump/landing, walk/run,
swimming/ascent/descent, explicit facing/pitch, heartbeat, and transport change.
The application supplies a resolved event-time snapshot; this codec owns its
wire representation, without deciding whether a controlled unit may move.
Teleport and forced-movement acknowledgement envelopes remain separate.

One packet owns at most 97 body bytes, including both maximally populated
packed GUIDs and every conditional MovementInfo section. Construction performs
no allocation. It rejects flags wider than 48 bits and conditional-field
presence mismatches before header encryption. Native scalar bytes remain
unchanged, including signed seat bytes, negative zero, and integer fall time.
The pinned message library labels fall time as `f32`; using a numeric cast
through that declaration would change the native `u32` bytes, so this encoder
uses the original `0x004F4ED0` integer representation directly.

`RuntimeGameplayCoordinator` admits the frozen message to the same bounded
writer queue used by time sync and worldport acknowledgements. Queue saturation
returns a rejection to the caller, which retains that exact event and its
ordering obligation. The asynchronous writer never samples a later ECS
transform to reconstruct an earlier event, and it completes an admitted write
before servicing another command. Connection cancellation retains the existing
policy of terminating the cipher stream rather than resuming partial framing.

Encrypted TCP tests compare all 25 envelopes against a native-layout golden
body, interleave time sync and a worldport ACK, and verify subsequent traffic
through the independent message decoder. A deterministic saturated-queue test
retains the rejected event, admits it once capacity returns, and checks every
timestamp and the following ACK. Minimal and maximum body sizes and every
optional-field mismatch are covered separately.

The remaining input/simulation owner must still resolve controlled-unit gates,
integrate displacement and collision at event timestamps, apply server changes,
and generate these events. Received wire flags cannot simply be reused as
local control flags: full admission at `0x006EB730` merges through mask
`0x77FFFDFF`, while its selective path uses `0x77E00DFF`. `0x006E90E0` and
`0x00987140` rebuild transport/spline wire state from its local owners.
Those are distinct state boundaries.

The first local collision dependency is now present: the native nine-plane
body sweep is implemented and checked against 69 calls to the original x86
routine. [Movement collision](movement-collision.md) records its exact scope,
evidence, arithmetic, and remaining response/world-provider work. Player input
still has no displacement consumer.
