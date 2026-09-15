# Stock performance operating model: build 12340

## Purpose and evidence boundary

This review precedes further performance changes. The question is how stock
limits the work and memory needed to keep an ordinary world scene running,
including movement and loading, rather than how to accelerate each existing
Solarity loop independently.

The local `Wow.exe` SHA-256 was verified again:
`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`.
The executable is the 32-bit 3.3.5a build 12340 image. Addresses below are
virtual addresses in that image. Evidence consists of fresh Ghidra decompilation
and disassembly, existing focused exports, and a new native-instruction lifetime
oracle. Decompiled parameter types and names are not authoritative; assembly
was checked for the important age, flag, list, allocation and scheduling gates.

This is an end-to-end review of performance-relevant operating boundaries, not
a claim to have understood all 27,733 recovered functions, all gameplay, or all
vendor internals. No stock live allocation profile or matched stock CPU trace
was captured during this review. Unresolved policy and quantitative questions
are listed explicitly below. No runtime behavior changes are included.

Solarity comparisons refer to Build 137 source at
`6daf1c8ba6cf0b7833ec34ebd6479cf49e8713fa` on `perf/critical-frame-work`.

The user reports roughly 5-7% CPU and 500 MB memory in stock while standing in
the world, versus about 25% and 1,700 MB in Solarity. These observations warrant
investigation; they do not identify allocation owners or establish matched
CPU-time-per-frame measurements. The existing Solarity movement capture is
useful evidence about Solarity, not a stock trace.

## Frame scheduling and background work

Stock does not treat lack of keyboard/mouse motion as a reason to stop the world.
It schedules registered event handlers and timers, animates admitted scenes,
updates relevant objects and submits frames. It also has explicit waiting and
frame pacing rather than requiring every idle thread to spin.

- `0047F230` runs scheduler iterations through `0047EFF0`.
- `0047EFF0` finds the next scheduled context, computes its deadline against
  `0086AE20`, and waits through `00774690`, or the alternate `0086F1C0` path.
  On timeout it dispatches the context's update/event stages and reschedules it.
  `0047D3C0` registers a callback for an event, rather than creating a new
  per-frame handler object. This is not evidence of a fixed world simulation Hz.
- `0076A630` registers `maxFPS`, `maxFPSBk` and `gxVSync`. The pinned default
  strings are **200**, **30**, and **1**, respectively. Callbacks
  `00769830/00769860` clamp positive values below eight to eight; zero survives.
- `006836D0` reads the foreground/background limits through `00681780/006817A0`.
  Zero becomes an unlimited sentinel. The background branch selects the more
  restrictive limit. It computes a millisecond interval and calls `0086B280`
  when the next deadline is still ahead. `0086B280` wraps `Sleep`.
- `004BA680`, the async file-read worker, removes requests from owned queues,
  reads their bytes and transfers them to a completion queue. Its empty-work
  path sleeps for one millisecond. This is polling with a wait, not an assertion
  that all stock workers use event-only wakeups.

The saved stock profile has `gxVSync=0`, Ultra `extShadowQuality=5`,
`farclip=1277`, and 2560x1440. The Testing profile also has VSync off, Ultra
shadows and the same far clip. Neither saved file explicitly sets `maxFPS`.
Stock therefore has a registered default that must be considered when measuring
it. Saved `gxFixLag` differs (stock 0, Testing 1); a saved value alone does not
prove that Solarity implements its native effect. Shadow level 0 is **not**
evidence that stock Ultra shadows are disabled: `extShadowQuality` is 5.

**Comparison:** Solarity's active world loop runs continuously; its minimized
path has a separate sleep. It uses bounded CPU pools, so unused total CPU is not
proof that the main thread can make progress. Stock's pacing can lower CPU when
it reaches a cap, but this review does not attribute the reported multi-fold
difference to a cap. FPS, charged CPU time, focus and effective settings need
to be measured together.

## Loading and completion publication

Stock separates file reads, completion callbacks, ordinary streaming and a
blocking loading barrier.

- `004B9B20` takes a finished request off the completion list under the lock,
  drops the lock, and invokes that request's completion callback. It checks
  elapsed milliseconds between callbacks against `00AC3444`.
- That variable's initial image value is **100 ms**. The check is between
  callbacks and can overshoot by one callback; it is not a hard low-latency
  frame guarantee. Runtime changes to this budget were not fully traced.
- `004BAE10` is a wait-for-completion path. It repeatedly pumps completion,
  reports progress and sleeps one millisecond until the wait condition clears.
- `007B6B00` runs terrain/WMO residency stages normally, but its explicit loading
  branch performs additional drains/waits. One branch waits for the required
  terrain area's request, reports progress, and sleeps ten milliseconds between
  polls. Other required dependencies are then completed in stages.

**Comparison:** background parsing alone is insufficient. Publication callbacks
can still dominate the main thread. Solarity must distinguish necessary
world-entry dependencies from nearby/speculative work and account for admission,
GPU upload, registration and destruction separately. Stock also waits before
world entry; the existence of a wait is not itself a bug. The present evidence
does not yet establish which Solarity prerequisite prolongs its final hang.

## Terrain, WMO and spatial ownership

Stock has a persistent map grid, area/chunk owners, reference lists and separate
scene queries. It does not represent the entire streamed world solely as one
dense vector that must be renumbered when one area leaves.

- `007831A0` updates the camera-derived window through `00780860` before calling
  the streaming driver `007B6B00`.
- `007B5950` derives the current area window, examines pending area references,
  removes or retains requests according to current bounds and in-flight state,
  registers missing WDT owners and sorts demand by distance. It uses the
  persistent grid rooted at `00CE48D0` and intrusive owner lists. It still scans
  and sorts relevant requests; stock does not make streaming cost disappear.
- `007B6110` independently admits WMO/group residency from world bounds and
  distance, with loading-mode distinctions.
- `007A5DD0` maps a spatial query to relevant grid regions. `007A50C0` traverses
  a region's references, applies participation/readiness/bounds tests and checks
  the object's visitation stamp against `00CE04C4`. An object referenced by
  multiple chunks need not be processed repeatedly for that collection.
- The WMO portal, group, exterior and depth gates remain additional visibility
  rules. Resource retention does not authorize bypassing them. Existing native
  fixtures cover those gates, including the previously investigated Orgrimmar
  horizon behavior.

**Comparison:** Solarity already has spatial queries, shared static owner
references and dynamic-only remapping. Genuine static additions/removals still
rebuild its global M2 spatial index in
[`m2/frame_work.rs`](../../crates/runtime/src/application/terrain_frame/m2/frame_work.rs).
The latest live run
showed repeated rebuilds of about 23,000-26,000 entries during residency churn.
This is a structural publication difference from the local grid/reference
operations inspected in stock, not proof that every stock spatial operation is
constant time.

There is also a concrete ordering discrepancy in
[`terrain_coordinator/streaming.rs::synchronize_streaming_with_admission`](../../crates/runtime/src/application/terrain_coordinator/streaming.rs):
Solarity polls and admits a completed tile before installing the incoming
window. Eligibility checks consult the previous stored demand. Stock's world
driver updates the current window first. This warrants a focused regression
test; counts alone do not prove that it caused every observed arrival/departure.

## Shared M2 resources and animation loading

Stock separates shared asset data from model instances and outstanding sequence
requests.

- `0081C390` normalizes the model name, searches a hashed shared-resource table
  and increments an existing resource's reference through `00835970` on a hit.
  Lookup behavior depends on flags; this is not an assertion that all filenames
  with the same basename are universally interchangeable.
- `0081F8F0` obtains that shared resource and constructs a model instance through
  `00834810`. Shared body/SKIN/sequence data and per-instance state are distinct.
- `00827190` requests the matching animation and its variation chain, checking
  sequence state before invoking `0083DA10`. An unready model can queue that
  request for later processing.
- `00831C30` joins an existing pending sequence record when possible and retains
  model-specific waiting consumers. Otherwise it requests that sequence.
- `0083DA10` resolves aliases, constructs the selected external animation path,
  allocates a request and queues an asynchronous file read. It sets loading
  state on the relevant shared sequence records. This is demand-specific
  loading, including explicit prefetch of a variation chain, not an unconditional
  read/decode of every possible external animation on model creation.

**Confirmed Solarity discrepancy:** `M2AnimationSet::load` in
[`crates/asset/src/model/animation/mod.rs`](../../crates/asset/src/model/animation/mod.rs)
loops over all sequences, reads every
available external payload, and decodes all bone, attachment, material, camera,
event, light, ribbon and particle tracks before the model is ready. Tracks own
separate timestamp/value vectors for their channels. This can increase archive
work, cold-load latency, temporary memory, retained decoded memory and allocation
count. It is a substantial design difference worth measuring before adding
caches. The temporary encoded payload vector is dropped after decoding; it
must not be counted as permanently retained without separate evidence.

This does **not** mean every decoded sequence is sampled every frame. A change
to demand loading must preserve sequence readiness, variation/RNG behavior,
callbacks and missing-companion behavior. Its steady-state FPS benefit remains
unmeasured.

## M2 lifetime: cache residence is not scene membership

The complete inspected shared-resource lifecycle is:

1. `0083DC90` decrements the reference count. On final release, a cache-qualified
   resource is timestamped at `+0x38` and linked into the cache's released list.
   Resources failing the cache-owner/qualification checks are destroyed directly.
2. `00835970` reacquires a zero-reference entry by unlinking it from the released
   list, clearing its retirement fields and incrementing its reference count.
3. `0081C290` normally destroys released entries when their elapsed age reaches
   **10,000 ms**. Its explicit force argument bypasses the age condition.
4. `0081C790` separately visits an aged graphics-resource list using a 10,000 ms
   threshold. `008368B0` releases retained graphics buffers/views and clears their
   fields. Qualification/insertion for every graphics-list path is not fully
   recovered here; do not treat it as a universal ten-second GPU lifetime.
5. `0083D5B0` releases shared allocations and dependencies at destruction.

The new `model_cache_lifetime_oracle.py` executes steps 1-3 using the original
instructions. **36 cases passed**, covering 9,999/10,000/10,001 ms, reacquisition,
forced collection, remaining live references, cache qualification and ordinary
32-bit clock wraparound. Clock, destructor and allocator-free calls are supplied
boundaries. This verifies list/refcount/age behavior, not actual GPU freeing or
the full live cache working set.

**Comparison:** [`M2ModelCache::collect_unused`](../../crates/asset/src/cache/m2_model.rs)
removes cache-only entries
immediately when invoked. Other owners can keep entries alive longer. That is
not the same policy as stock's explicit released-resource grace period. Adding
unbounded retention is also not stock's policy. Scene/collision/visibility
membership must end at the correct gameplay boundary even while shared assets
remain available for reuse.

## Model preparation, callbacks and effects

Stock does not have one undifferentiated "update every resident M2" operation.
The important inspected chain is:

```text
scene time / registered model callbacks: 81C9C0 -> 832450 -> 832260
scene render registration:              823F10
registered root preparation:            821A20 -> 82F0F0 or 82E140
registered root effects:                821A20 -> 828A00
registered root receiver lighting:      821A20 -> 831AF0
prepared lists / sorting / submission:  remainder of 821A20 and draw consumers
```

- `00832450` performs enabled-model callback traversal, including child-enable
  gates and temporary lifetime protection. `00832260` processes active sequence
  event/completion intervals under its state flags. This can be required without
  a visible mesh.
- `00823F10` adds/removes a model from a scene list and rejects duplicate insertion
  using its membership link. `00821A20` works from registered roots, not the
  shared resource cache, and drains that registration list after preparation.
- `00821A20` has a conditional two-lane root-preparation path: `0081CE70` visits
  alternating roots while the caller visits the other lane; `0081BFA0` signals
  the worker and `0081BFD0` joins it. The resource flag selects this path. Its
  presence is verified; whether it was enabled in the user's stock session is
  not measured.
- Effects are then updated through `00828A00`; receiver lighting runs through
  `00831AF0`, including inheritance and registered callbacks. Attached children
  retain their own traversal rules. Offscreen callbacks, lights, effects and
  shadow casters cannot all be suppressed by a camera-only test.
- `00832EA0` sizes much of instance state from authored counts into a contiguous
  allocation and separately allocates the aligned bone palette. There is real
  per-instance memory in stock: it does not share every pose or simulate each
  fire type only once globally.

**Comparison:** Solarity has already implemented separate CPU/render bone demand,
ordered callbacks and joined pose/geometry work. Repeating that diagnosis alone
is insufficient. The remaining comparison must measure how many owners reach
each collector, how often they reach it, their scene/attachment dependencies,
and the bytes/state carried per owner. The current live trace does not show
unused unit-pose batches as the main waste.

## Draw preparation, batching and GPU state

Stock uses retained resources, ordered draw categories and compatibility-limited
batching. It also repeats necessary camera, lighting and dynamic work each frame.

- `00821A20` builds separate work lists and sorts prepared entries before draw
  consumers run. The list sizes are reset/reused rather than treating every
  emitted frame as a permanently resident generation.
- The shadow batching routine `00829BA0` handles multiple compatible instances,
  packs their bone transforms and submits a combined bounded batch.
  `00836DF0` grows retained batch capacity in groups of sixteen up to a limit.
- `008360A0/008362B0` retain replicated index/vertex buffers and populate them
  when absent/invalid. This is stock's supported batching strategy; it is not
  proof that all ordinary animated models use modern hardware instancing.
- `00685F50` compares requested render state against the retained state before
  marking/updating it. `0081F450` resolves material texture stages and transforms
  for a prepared draw. State caching does not eliminate all per-draw CPU work.
- BLP format selection and mip loading (`004B5FE0`, `006AFFD0`) retain distinct
  compressed/uncompressed formats and selected levels. Solarity already has
  BC1/BC2/BC3 upload paths; universal RGBA expansion is not an established cause
  of its memory discrepancy.

**Comparison:** Solarity's draw templates, opaque instancing, compressed texture
uploads and fence-based retirement already cover parts of this foundation.
Stock's D3D9 ownership cannot be copied literally into Vulkan. Account for live
resources, staging, in-flight versions, dead-but-retained resources and driver
memory separately before attributing the 1.7 GB observation to any one category.

## UI, text and glyphs

- `004BE9C0` reuses a font resource keyed by font/flags/size-related inputs.
- `006C3FC0` looks up an existing glyph first. A miss reaches rasterization via
  `006C2480/006C8CC0` and placement through `006C5120`. It examines up to eight
  atlas slots, can reclaim existing placement when space is exhausted, and marks
  the affected atlas slot dirty. This is bounded glyph reuse, not "keep every
  historical atlas forever" and not "rasterize every drawn character".
- `00486B20` gates recreation of a string's retained render handle on its
  invalidation bit. The reconstruction path is conditional, rather than an
  unconditional consequence of drawing another frame.
- `00495320` drains queued UI work; `00494EE0` gates layer rebuilding on state.
  The draw phase still visits active render layers. Registered Lua OnUpdate,
  animation and input can remain active without player movement.

**Comparison:** Solarity now has persistent glyph coverage, stable owner geometry,
local UI mutation journals and indexed clipping. Those improvements should be
kept. Stock's bounded atlas policy does not prove a particular Solarity cache
budget, nor does this pass establish stock's complete HTML/scroll invalidation
cost. A visual scroll should not justify a whole-UI snapshot in our architecture.

## Networking, object state, movement, collision and sound

| Boundary | Inspected stock behavior | Evidence and limitation |
| --- | --- | --- |
| Network delivery | Queued connection events are consumed through dispatch tables under explicit connection lifetime protection; consumed queue records/payloads are freed. | `006321A0`, `006334F0`, `00633470`. This is not a stock live packet-rate trace or proof of every Winsock worker's wait policy. |
| Replicated object state | Object-update handling calls the appropriate object behavior and installs model callbacks/representation state at that boundary. | `00712F30` and its `004D63B0` caller; `stock-go-object-update.c`. Full Unit_C dirty-bit policy is broader than this inspected path. |
| Movement | Remote movement advances from timestamped inputs, tests movement/trajectory state, and may integrate multiple intervals. Lack of new network packets does not imply no movement work. | `stock-remote-frame-integrator.c`; existing native movement fixtures. No fixed universal stock update rate inferred. |
| Collision | Spatial grid/owner references and cached query regions reduce candidate work; detailed terrain/WMO/M2 tests remain conditional consumers. | `007A5DD0`, `007A50C0`, `0075F0A0`, existing BSP/sweep-cache native fixtures. Cache validity must include membership and movement changes. |
| Sound | Voices have their own handles/lifetimes and backend state. The engine classifies existing voices and counts categories for admission; sound resources have a destructor that frees owned buffers and closes the archive handle. | `0087EE60`, `00879AE0`, `00877850`. Exact total live sound-cache byte budgeting and vendor mixer cost remain unmeasured. |
| Media | Movie/audio work has an independent lifecycle and cadence, rather than being a property of M2 residency. | Existing movie and sound exports; this pass does not re-reverse-engineer the codec/backend internals. |

Solarity already has ordered network batches, retained unit membership,
movement/collision indexes, shared sound payloads and asynchronous sound loading.
The latest capture does not implicate networking or sound as the dominant
multi-millisecond city cost. Their ownership still belongs in memory accounting;
they should not be declared free merely because their CPU scopes are small.

## What this changes in the performance investigation

The clearest newly established gaps are **eager all-animation loading** and
**shared-resource release policy**. The broad movement concern also has a stock
reference: current-window admission and persistent local spatial ownership,
against Solarity's stale-window hazard and global index rebuilds. These are
concrete operating-policy differences, not another claim that sampling a few
bones faster will produce a five-millisecond gain.

The next implementation decisions should be based on:

1. Bytes and time attributable to eagerly decoded sequence data, with separate
   counts for shared models, channels, key arrays, temporary payloads and actual
   requested sequences. Check whether separate cache owners load the same asset
   more than once; multiple cache structs alone do not prove live duplication.
2. Correct active membership, reuse of qualified released resources, and local
   spatial changes. Preserve shared ownership, clock/RNG state, stock portal
   behavior and independent shadow/callback demand.
3. A matched stock/Solarity frame comparison: active model/particle/shadow
   counts, CPU time per frame, render submissions and effective settings.
4. Memory ownership accounting for decoded assets, instance/scratch state, UI,
   audio, staging, live GPU resources and retired generations. Distinguish
   working set from private commit and VRAM. Stock's 32-bit pointers and our
   64-bit containers can contribute overhead but do not explain a measured
   multi-fold difference without counting the objects involved.

Unresolved: the complete stock allocator census; live default/flag overrides;
all animation prefetch callers; complete texture/audio eviction policies;
stock visibility population in the exact Orgrimmar view; total time in native
driver/vendor libraries; and the exact Solarity pre-world critical path. These
remain research/measurement questions, not invented stock rules. No doubled FPS,
five-millisecond saving or memory reduction is claimed by this review.

## Reproduction and validation

`tools/ghidra/ExportStockPerformanceEvidence.java` writes binary identity,
matching string/index evidence, selected decompilation and assembly into an
external directory. Use `-noanalysis -readOnly` against the pinned Ghidra project.
Missing explicit entries may be recovered in memory; overlapping entries are
reported rather than destructively replaced. String references depend on the
project's analysis state; an absent xref is not evidence of an unused string.

The selected entries for this review are in
`tools/ghidra/stock-performance-functions.txt`. Pass those whitespace-separated
addresses after the output directory to the exporter. Fresh local outputs are
in `target/stock-performance-review`; the complete manifest validation is in
`target/stock-performance-review-verified`. Supporting older exports remain
under `target`. Generated native code and executable bytes are not committed.

Run the focused native oracle with Unicorn 2.1.4 available:

```text
python -B tools/ghidra/model_cache_lifetime_oracle.py <Wow.exe> <output.json>
```

The final exporter compiled and ran in headless Ghidra 12.1.2/JDK 21; all 74
manifest entries produced decompilation and assembly, with no skipped entries
or decompilation failures. The project reported discarded read-only changes.
The lifetime oracle passed 36 cases. Python syntax and documentation links
were also checked.
These checks validate the research tools and focused evidence. Rust runtime
code and the installed Build 137 were unchanged; no new gameplay benchmark or
package was produced.
