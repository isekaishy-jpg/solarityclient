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

The current unit deliberately stops at the physical boundary. It does not
hard-code default bindings or synthesize a binding when FrameXML has not
declared one. Binding catalog loading and command routing can therefore consume
the same retained state without turning SDL scancodes into gameplay policy.

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
