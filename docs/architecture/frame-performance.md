# Interactive frame performance

Solarity treats frame time as an ownership contract rather than a collection
of scene-specific exceptions. Idle Glue, loading, and World frames target a
3.33 ms complete-frame budget for 300 FPS. A 0.83 ms stretch budget identifies
work that prevents 1,200 FPS at low resolution, but visual correctness and
completed presentation remain mandatory; skipping stock effects or counting
loops that did not present is not a valid result.

## Measured baseline

`SOLARITY_FRAME_TIMINGS=1` separates retained Glue preparation from Vulkan
presentation. Release measurements on the current test machine establish two
different limits:

| Surface | Complete FPS | CPU prepare | Vulkan record | Submit/present |
| --- | ---: | ---: | ---: | ---: |
| 1280x720 windowed | 1,150-1,186 | 0.09-0.18 ms | 0.06-0.07 ms | 0.09-0.10 ms |
| 2560x1440 borderless | 337-433 | 0.14-0.22 ms | 0.07-0.10 ms | 1.8-2.1 ms |

Turning off the Glue glow chain did not materially change the 1440p result.
The steady borderless ceiling is therefore currently presentation/GPU
back-pressure, not the Lua idle tick. UI and residency mutations are a separate
source of long individual frames and visible FPS oscillation.

The real 3,240-object Glue archive now measures 0.067 ms mean per idle UI
update (0.290 ms p99) and 0.034 ms mean per held-hover color update. A
CharacterCreate screen event is not an idle frame: it executes about 13 ms of
stock Lua and spends a further 13 ms publishing a monolithic glyph/mesh
generation. Caching inherited geometry per text owner reduced that event's
10,889-glyph resolve phase from about 4.0 ms to 2.7 ms, but the transition path
remains the principal UI architectural debt.

Character preview publication had two independent presentation-thread waits:
dynamic atlas upload and authored BLP batch upload. Both now submit before the
following draw on the same graphics queue and retain their staging resources
behind retirement fences. Queue order preserves transfer-to-sample safety
without a host wait. Character screen identity is also independent of worker
readiness, so an obsolete CharacterSelect body or failed worker generation
cannot become the authority for CharacterCreate.

## Permanent ownership model

The build-12340 executable and archived GlueXML/FrameXML remain the behavioral
oracle. The stock client module evidence is catalogued in
[stock-client-module-map.md](stock-client-module-map.md), while the rendering
and UI evidence is recorded in the focused M2 and content-loading documents.
Stock behavior does not require reproducing its allocator, thread, or graphics
API choices.

The permanent runtime follows these boundaries:

1. Lua owns observable script execution and arbitrary addon fields. Native
   typed state owns engine properties, dirty masks, hierarchy, and resolved
   query results.
2. One mutation batch is committed after a synchronous event/update. Each
   consumer receives the same revision; no consumer independently snapshots
   the full Lua arena.
3. Layout invalidation follows parent, anchor, content, and scroll reverse
   indexes. Work is proportional to the affected dependency island.
4. Presentation is retained in independently replaceable ordered packets.
   Color, alpha, translation, caret, and widget-state changes patch exact
   ranges. Text, material, or geometry changes rebuild only their packets.
5. The renderer retains packet resources per in-flight frame slot and uploads
   merged dirty ranges. A late or incomplete multi-packet transition preserves
   the last complete revision instead of exposing partial UI.
6. Asset decode and immutable mesh preparation run on bounded workers.
   Interactive work has admission priority; speculative prewarming cannot fill
   every worker lane. Vulkan creation and publication remain on the
   presentation owner and cross at most one bounded generation per frame.
7. Scene and loading generations are prepared behind an existing movie or
   authentication cover. A transition becomes visible only when its complete
   UI, environment, character, and required material set is ready.

The SolCL retained-runtime implementation confirms two details that matter to
the next cut: resource assignments are subscribed when setters run even while
their objects are hidden, and renderer publication consumes an atomic revision
of retained chunks. Solarity should copy those ownership properties, not its
language or class layout. Building every hidden widget into the active draw
mesh would merely move the stall to startup; setter-time residency plus a
screen-revision commit is the required boundary.

This is compatible with the corresponding SolCL design: typed dirty
dependencies, cached compositor order, retained render chunks, scoped task
mailboxes, and explicit scheduler policies. Solarity will migrate by replacing
measured monolithic boundaries, not by adding a second UI tree beside them.

## Migration order

The current retained implementation already avoids idle snapshots, coalesces
character requests, keeps encountered hidden presentation slots resident,
directly patches hover colors, retains compatible prepared Vulkan draws and
command bindings, defers character texture transfers, and reserves CPU capacity
for interactive residency. The next structural slices are:

1. Replace whole-arena geometry resolution with indexed dependency-island
   publication and remove resolved engine state from Lua shadow fields.
2. Split the single UI mesh into revisioned ordered packets with atomic
   multi-packet screen transitions.
3. Retain glyph ranges and material descriptors by packet so changing one
   label or texture cannot recreate unrelated draw state.
4. Separate queue submission from presentation timing and record GPU
   timestamps. Use those measurements to choose the correct 1440p swapchain,
   offscreen, and post-processing changes.
5. Apply the same retained-generation and priority rules to loading/world
   residency before raising the minimum idle target beyond 300 FPS.

## Validation

Performance changes require both contract tests and real-archive release
measurements. The minimum checks are:

```powershell
cargo test -p solarity-ui --test stock_seed
cargo test -p solarity-runtime --test stock_seed application
cargo run --release -p solarity-ui --example benchmark_ui_idle -- <Data> enUS 10000
$env:SOLARITY_UI_TIMINGS = '1'
cargo run --release -p solarity-ui --example validate_glue_scene -- <Data> enUS charcreate
$env:SOLARITY_FRAME_TIMINGS = '1'
```

Reports must distinguish idle mean/p95/p99, synchronous event cost, worker
residency latency, renderer record time, and submit/present time. A faster mean
does not excuse a transition that publishes incomplete UI or a long frame that
can recur during ordinary interaction.
