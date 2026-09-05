# Runtime configuration and composition

The runtime crate is the only workspace boundary allowed to construct concrete
cross-crate services. The executable mounts client archives, creates the SDL
window and Vulkan renderer, executes and retains the archive-backed GlueXML UI,
creates the bounded CPU executor and Tokio network runtime, then remains in the
main-thread SDL event loop until process or primary-window termination. It
finally drains owned services in explicit ownership order.

## Stock evidence

The fingerprinted build-12340 executable contains connected source references
for `Client.cpp` across startup functions beginning at `0x00401390` and for
`ClientServices.cpp` across functions beginning at `0x006b2200`. `Profile.cpp`
is connected to configuration work beginning at `0x004bbea0`. These references
support a process-level composition owner and a separate persistent-profile
responsibility.

The present command-line schema is a Solarity startup interface, not a claim
that stock exposed the same flags. It deliberately requires every value needed
by the implemented services:

```text
--data-root <Data>
--locale <locale>
--cpu-workers <count>
--cpu-capacity <count>
--network-workers <count>
--network-shutdown-ms <milliseconds>
--login-endpoint <host:port>
--login-timezone-minutes <signed-minutes>
--login-client-ip <IPv4>
--window-width <pixels>
--window-height <pixels>
--window-mode <windowed|fullscreen-windowed>
--gpu-index <zero-based-index>
```

Missing, duplicate, unknown, zero, and malformed values fail explicitly. The
configuration layer does not infer worker counts from the machine or silently
select a locale/data directory. Platform capability detection may later
produce suggested values for a launcher, but the typed composition input stays
complete.

`fullscreen-windowed` means a borderless desktop-composited window pinned to
the primary display's full logical bounds. It does not enter SDL exclusive
fullscreen, change the display mode, or minimize when focus moves to another
application. `windowed` retains an ordinary resizable decorated window.

## Ownership and shutdown

Construction follows dependency order:

1. discover and mount the validated stock/patch archive catalog;
2. create the main-thread SDL window and Vulkan 1.3 presentation owner;
3. execute GlueXML and prepare its initial device-local frame resources;
4. construct the bounded private Rayon CPU executor; and
5. construct a separate multithreaded Tokio runtime and idle login coordinator.

Shutdown first closes and drains CPU admission, then consumes Tokio with the
configured timeout. Mounted archives remain owned by the service aggregate
until it is dropped. A final `Drop` implementation repeats the idempotent drain
as a safety boundary, but normal process flow calls the fallible explicit
shutdown path.

Tokio and Rayon are intentionally not interchangeable. Blocking MPQ reads,
HD-asset decompression, parsing, and simulation work belong on the bounded CPU
executor. Socket and timer futures belong on Tokio; they must not perform
blocking asset work on its workers.

## Current executable behavior

`solarity-runtime` constructs and executes the stock GlueXML manifest using the
mounted archive stack, prepares its texture batches, and reveals the window
only after the first Vulkan frame presents. FIFO swapchain presentation paces
subsequent main-thread boundaries. Each boundary drains SDL events, applies the
ordered Glue login-action mailbox, polls the asynchronous Grunt task, and then
presents the retained login generation.

Login configuration does not open a socket at startup. `DefaultServerLogin`
admits one task that connects, proves build 12340 with SRP, and requests the
realm directory. `CancelLogin` aborts only an in-progress task;
`DisconnectFromServer` also releases authenticated ownership. Transport and
login failures remain ordered for the Glue status layer, while a successful
result retains both the authenticated realmd stream and the exact server-order
realm rows for explicit realm selection.

Character entry owns a distinct retained loading presentation rather than
leaving character selection visible. The selected character's authoritative
map joins `Map.dbc` to `LoadingScreens.dbc`; source-art texture references are
canonicalized to their packed BLP paths, widescreen art is selected when
authored, and a generic stock card remains available when optional table or art
data is absent. Its progress generations advance only after world acceptance,
environment availability, player-model residency, and first terrain-frame
residency. Input remains with the transition until one complete final card has
been presented.

Tiled maps retain a [camera-driven ADT neighborhood](terrain-streaming.md).
Worker-completed neighbors enter collision and renderer state together; primary
tile changes preserve retained M2 playback, particles, and ribbons. Shared
placement IDs prevent duplicate buildings and doodads across ADT references.
Departed terrain GPU allocations retire after their queued frame uses complete.

WDT content selects either an ADT-backed terrain generation or the sole global
MODF placement. The global branch retains that placement's transform, doodad
and name sets, root/group WMO resources, selected embedded M2s, collision and
liquid providers, and Map.dbc field-22 base area. It prepares the same world
frame without fabricating an empty ADT, allowing instance loading readiness to
complete on maps such as Stormwind Stockade and the Nexus.

The native top-left performance display uses the archive-backed
`Fonts\FRIZQT__.TTF` face and stock `showfps` default. It samples completed
presentations in quarter-second windows, replaces one stable device mesh when
the displayed one-decimal value changes, and shares the same overlay path on
Glue, loading, and world frames.
