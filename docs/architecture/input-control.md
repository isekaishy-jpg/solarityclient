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
