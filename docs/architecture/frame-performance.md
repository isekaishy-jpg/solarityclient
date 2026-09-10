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
stock Lua and previously spent a further 13 ms publishing a monolithic glyph/mesh
generation. Caching inherited geometry per text owner reduced that event's
10,889-glyph resolve phase from about 4.0 ms to 2.7 ms. Carrying opacity and
ScrollFrame draw state with each resolved owner then removed duplicate
per-glyph geometry queries and reduced it to about 1.8 ms. Fixed-capacity stock
anchor scratch storage reduced global geometry resolution from about 1.4 ms to
0.7 ms. A POD vertex ABI now exposes the typed mesh as borrowed bytes instead
of maintaining a second serialized copy, reducing CharacterCreate mesh
serialization from about 1.4 ms to 0.9 ms. Native publication is about 10.8 ms;
the transition path remains the principal UI architectural debt.

Character preview publication had two independent presentation-thread waits:
dynamic atlas upload and authored BLP batch upload. Both now submit before the
following draw on the same graphics queue and retain their staging resources
behind retirement fences. Queue order preserves transfer-to-sample safety
without a host wait. Character screen identity is also independent of worker
readiness, so an obsolete CharacterSelect body or failed worker generation
cannot become the authority for CharacterCreate.

Screen changes now use two reusable UI frame slots and publish the replacement
UI, backdrop, character, attachments, and effects as one transaction. Input is
held on the last presented screen while that transaction is pending. Worker
queue saturation is normal backpressure: a character with more equipment
sources than the finite queue retains its admitted prefix and retries the
remainder instead of turning `AtCapacity` into a failed or stuck transition.
Precompiled character driver pipelines are admitted one at a time while the
previous complete scene remains visible; this replaces the measured 22-27 ms
first-use pipeline burst with bounded individual frames.

Object-local color changes carry a bounded mesh-revision journal from the UI
plan into renderer residency. The renderer copies and records only the merged
changed vertex span when its retained identity is an ancestor of the current
plan; independently rebuilt or expired generations safely fall back to a full
payload comparison. This removes the hidden whole-mesh byte scan that remained
after Lua and layout had already reduced hover updates to one object.

A recoverable Lua callback is also contained inside the UI frame boundary.
The failing `OnUpdate` member is retired and reported once through the bounded
developer-console mailbox, while healthy callbacks, retained mutation
publication, and swapchain presentation continue. Previously the outer event
loop caught the same error after `GlueManager::update` aborted, skipped the
present, and retried it roughly every 50 ms. That produced a false 20 FPS
"renderer" failure and stale/disappearing UI even though no rendering work was
the cause.

Pointer `OnEnter` and `OnLeave` failures use the same bounded callback mailbox,
while completing the current mutation journal and hover transition. A missing
bag-tooltip API previously aborted `OnLeave` before the manager released its
old hover target, causing every later mouse event to retry that callback.
The regression verifies a failed tooltip's visibility changes, transfer to a
healthy button, and the next click. The archive lifecycle validator also
replays `CharacterBag0Slot`, subsequent pointer events, and camera bindings.

Ordinary Testing installs omit `-FrameTimings`. Explicit timing sessions retain
all application samples but emit one summary per scope every two seconds,
including frame counts, means, maxima, phase timings, and the worst frame.
The earlier five-millisecond threshold generated nearly 25,000 detailed frame
records in a four-minute run through the synchronous console/log pipe.
Expected model-sound exclusivity and channel-capacity rejections are quiet;
invalid content and unexpected sound failures remain warnings.

World FrameXML requires its own measurement; the Glue results above do not
establish World throughput. The `validate_frame_lifecycle <Data> <locale>
[benchmark frames]` example runs the installed FrameXML with a fixture player,
warms up 32 updates, and reports CPU update percentiles and requested presentation
uploads. It excludes GPU upload, World simulation, and rendering. The initial
1,000-update run averaged 20.406 ms, with every update requesting publication.
The typed journal identified `BuffFrame` clearing and restoring the same anchors
on each update, causing global layout, presentation, mesh, and pointer rebuilds.

Topology-stable layout journals now compare their final width, height, scale,
and complete anchor slices with the published state before invalidating native
plans. Lua still observes every setter and every intermediate anchor state.
Pending automatic text measurement and mixed content journals use their normal
publisher. Changed anchor targets still invalidate dependent regions. A layout
transaction that ends unchanged reports no presentation change, while concurrent
visual mutations still publish independently.

Animation clocks likewise advance and deliver callbacks before their composed
contributions are compared with retained values. Timing-only groups such as
FrameXML's `AnimTimerFrame` no longer republish identical transforms every tick.
Visual journals also reuse pointer target facts: hit testing already consults
updated geometry for visibility, alpha, and transformed bounds. These changes
preserve real fades, movement, timer completion, and pointer eligibility.

With these changes, a 10,000-update run averaged 0.401 ms (0.309 ms median,
0.765 ms p95, 1.637 ms p99, 3.873 ms maximum), with 227 requested publications.
Both runs used elapsed wall time as the authored update interval; this is a
windowless CPU measurement, not an equal-duration replay or proof of complete
World FPS. Full World performance remains to be measured after installation.

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

SolCL's split frame/background compute lanes remain the correct direction for
transition latency, but they do not explain or fix idle Glue throughput. The
current idle UI cost is already far below the 0.83 ms stretch budget, whereas
native 1440p time is dominated by graphics queue submission/presentation and
scales with pixel count. Worker-lane changes therefore follow correctness and
backpressure fixes; they must not be presented as a route to 1,200 FPS at a
GPU-bound surface.

## Migration order

The current retained implementation already avoids idle snapshots, coalesces
character requests, keeps encountered hidden presentation slots resident,
directly patches hover colors, retains compatible prepared Vulkan draws and
command bindings, defers character texture transfers, and reserves CPU capacity
for interactive residency. It also enumerates every configured Glue texture
assignment after script initialization and decodes those sources in a private
worker cache while the movie/authentication cover is active. Completed sources
are adopted without replacing newer owner state, and individual missing files
cannot discard the rest of the speculative generation. Sampled-image handles
belong to a persistent pre-world residency owner instead of an individual mesh
revision: covered prewarming uploads each configured source once, and later
screen revisions bind cache hits rather than recreating every visible BLP and
glyph atlas. This is the transition step toward setter-time subscriptions;
dynamic assignments still need to enter the same residency journal when their
setters run. The next structural slices are:

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

## World placement admission, September 10, 2026

The M2 admission loop now reads cached owner flags before touching a rejected
placement's simulation record. WMO attachments resolve their frame inputs only
when visited by a group, except that hidden light owners retain their early
preparation. Accepted indices alone commit fog state. These changes preserve
scene order, distance fades, moving-parent bounds, light publication and hidden
effect clocks; the Vulkan regression covers both lit and unlit attachments.

On the GTX 1070 at 1280x720 with VSync disabled, the optimized offline benchmark
at map 1, `(1100, -4500, 150)`, camera distance 25 and screen effect 0 ran 1,000
frames in each of four phases. Stationary, orbit and pointer phases retained
49 tiles. With capture and frame instrumentation disabled:

| Total frame time | Build 81 baseline, two runs | Admission changes |
| --- | ---: | ---: |
| Stationary median | 3.944 / 3.925 ms | 2.937 ms |
| Orbit median | 4.093 / 4.075 ms | 3.195 ms |
| Pointer median | 4.193 / 4.163 ms | 3.238 ms |
| Stationary p99 | 5.256 / 8.123 ms | 4.395 ms |
| Pointer maximum | 35.302 / 35.469 ms | 34.607 ms |

The stationary median decreased about 25%, corresponding to approximately
340 FPS in this scene. Separately instrumented runs attribute the M2 frame
reduction from 1.580 to 0.711 ms to instance traversal (1.004 to 0.435 ms) and
WMO attachment preparation (0.411 to 0.103 ms), weighted over each run's final
two timing windows. Windows cross phase boundaries and are supporting cost
evidence, not an isolated GPU measurement. Pointer transitions still produce
roughly 35 ms frames, with about 31 ms charged to UI in the instrumented runs.
These measurements do not establish the 1,200 FPS target, freedom from stalls,
populated-server performance or complete world appearance parity.

Local evidence: `target/world-admission-optimized.csv`,
`target/build81-normal-perf{1,2}.csv`, `target/build81-world-profile.log` and
`target/world-lazy-doodad-profile.log`.

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
