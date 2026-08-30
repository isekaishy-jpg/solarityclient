# SDL platform boundary

## Decision

The runtime owns exactly one SDL context, video subsystem, Vulkan-capable
primary window, and event pump on the process main thread. SDL is the native
window, input, clipboard, audio-device, and related operating-system facade;
it is not the renderer. The rendering crate remains an explicit Vulkan 1.3
implementation through `ash`.

The window is created hidden. It is revealed only after the renderer owns a
presentable swapchain, preventing an uninitialized surface from flashing
during startup. Logical window dimensions are kept separate from the physical
pixel extent so high-density displays do not silently distort UI coordinates
or swapchain sizing.

Window width, height, and mode are required startup inputs. The runtime does
not infer a desktop resolution or silently choose windowed/fullscreen policy.
The title `Solarity` is product identity rather than machine-dependent policy.

## Stock boundary

The recovered build-12340 responsibilities are distributed across
`Client.cpp`, `OsCall.cpp`, `InputControl.cpp`, `InputControl.h`,
`OsClipboard.cpp`, and `OsIME.cpp`. The stock executable used Win32 and
DirectInput directly. Solarity intentionally replaces those operating-system
calls with SDL3 while retaining the same architectural boundary:

- the composition root owns process and window lifecycle;
- renderer code consumes window and drawable changes;
- input code receives ordered keyboard and mouse transitions;
- UI text fields receive committed UTF-8 and IME composition separately;
- application focus/background changes remain explicit events.

This is an implementation substitution, not evidence that stock used SDL.
Ghidra names and recovered addresses remain the source for behavioral work;
SDL supplies the modern 64-bit operating-system adapter.

## Event admission

SDL types stop at `runtime/platform`. The public runtime vocabulary admits:

- process quit and application foreground/background transitions;
- window visibility, position, logical size, physical pixel size, focus,
  minimize/maximize/restore, display migration, and close requests;
- physical scancodes and layout-resolved keycodes with modifier and repeat
  state;
- committed text and in-progress IME composition;
- high-resolution mouse motion, five desktop buttons, click counts, and wheel
  direction;
- clipboard change notification.

Controller, touch, pen, camera, SDL renderer, and custom-user events are not
silently treated as stock input. They remain outside the admitted vocabulary
until a stock-backed or explicit product requirement assigns them behavior.
Polling returns one admitted event at a time and does not allocate a per-frame
event batch.

## Ownership and shutdown

Field order makes teardown reviewable: event pump, window, video subsystem,
then process SDL context. CPU and network workers never own or poll SDL's main
thread objects. The application drains its CPU and network executors before
the composition root drops the platform owner.

The external runtime test constructs the actual hidden Vulkan window, pushes a
pixel-size event through SDL's process queue, observes the SDL-independent
translation, and then performs orderly shutdown. It does not select a fake
video driver or developer-machine fallback.

On 64-bit MSVC debug builds, SDL's vendored CMake output embeds a redundant
`MSVCRTD` default-library directive while Rust already selects the process CRT.
The target-specific linker configuration excludes only that directive. A clean
relink proves the SDL and SDL_mixer static objects have no unresolved debug-CRT
symbols; all other linker diagnostics remain enabled.
