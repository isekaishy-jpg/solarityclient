# Runtime configuration and composition

The runtime crate is the only workspace boundary allowed to construct concrete
cross-crate services. The executable mounts client archives, creates the SDL
window, Vulkan renderer, bounded CPU executor, and Tokio network runtime, then
remains in the main-thread SDL event loop until process or primary-window
termination. It finally drains owned services in explicit ownership order.

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
```

Missing, duplicate, unknown, zero, and malformed values fail explicitly. The
configuration layer does not infer worker counts from the machine or silently
select a locale/data directory. Platform capability detection may later
produce suggested values for a launcher, but the typed composition input stays
complete.

## Ownership and shutdown

Construction follows dependency order:

1. discover and mount the validated stock/patch archive catalog;
2. construct the bounded private Rayon CPU executor; and
3. construct a separate multithreaded Tokio runtime for network and timer I/O.

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

`solarity-runtime` presents the stock missing-icon BLP as its initialized
bootstrap frame, reveals the window, and blocks on SDL rather than spinning or
exiting immediately. Global quit, host termination, and a close request for the
primary window end the loop; a close event for another window does not. The
generated integration fixture proves startup, a real queued SDL termination,
and shutdown with all ten required consolidated archives.
