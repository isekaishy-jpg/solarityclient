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
opcodes. Event admission uses `0x006EBC70` and `0x006EC090` before the movement
update/dispatch owner at `0x007B5020`.

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
