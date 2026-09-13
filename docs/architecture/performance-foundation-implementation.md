# Performance foundation implementation

The September 13 architectural audit establishes five required workstreams.
The purpose is to make feature work inherit bounded, retained work rather than
requiring another broad performance pass as content grows. Preserve existing
stock behavior, including visibility, ordering, animation and error contracts.

## Required contracts and progress

- [x] Local UI updates and stable owner storage: scrolling avoids full snapshots
  and document-wide glyph scans; changing text length leaves unrelated geometry
  in place. Extend retained publication and partial buffer updates.
- [x] Persistent font and glyph coverage: retain exact font metrics and glyph
  rasterization across text owners; adding glyphs preserves existing coverage.
- [x] Change-driven placement and appearance: transient effects do not rebuild
  static scenery metadata; unchanged local-player inputs reuse appearance plans.
  Local-player admission and independent pose updates have separate modules.
- [x] Shared draw preparation and instancing: retain validated immutable draw
  templates and batch compatible scenery/shadows using compact instance records,
  with simulation, pose and render work requirements kept explicit.
- [x] GPU lifetime and growth: retire superseded resources after their final
  submitted use; ordinary growth does not drain the device. Account for retained
  resources and test repeated content changes for generation accumulation.

Split affected implementations into folder-backed responsibility modules. This
is not a repository-wide cleanup. Glyph instancing, broad animated-character
batching, aggressive pose sharing, indirect/bindless rendering and GPU animation
remain later work; the interfaces should permit them.

## Validation and delivery

Exercise ownership/invalidation and scaling contracts through representative
consumers and regression tests. Run required formatting, workspace Clippy and
tests. Compare Soap in matched city scenes and exercise EULA scrolling at the
same resolution and settings; report frame times and limits of the evidence.
Finish with a numbered Testing package, independently reviewable commits and
push to `perf/critical-frame-work`. Build 130 is the installed starting point.

## Completed source checkpoints

`08840c61` retains local appearance inputs and publishes effect-tail metadata
without rebuilding ordinary placement membership or the scenery index. Source
compaction remaps compact references directly. Five equipment tests, 20 related
placement tests and two effect tests passed (one manual benchmark ignored).

The UI checkpoint separates physical quad allocation from draw order, reuses
vacant spans, and updates only changed source geometry. Clipping indexes short
ordered blocks and skips offscreen document ranges. Scroll dispatch journals
name their owners; range caching follows layout/visibility changes and explicit
child-rect publication. Nineteen renderer UI tests and 161 UI integration tests
passed, including repeated growth, 20,000-line clipping and a no-full-snapshot
wheel test. UI/runtime Clippy passed with all targets/features.

`82ae6a54` shares exact font faces, metrics, kerning and rasterized glyph coverage
between measurement and rendering, scoped to the mounted archive identity.
`933a12e4` appends missing coverage into stable atlas pages. Existing glyph UVs
and texels survive new characters and text owners. Full UI rebuilds retain the
coverage bank; region uploads are queue ordered and do not wait on the host.
Twenty-eight UI unit tests and 162 integration tests passed at this checkpoint.

M2 source admission now retains validated mesh/material draw templates. Frame
preparation supplies transforms, material state and palette offsets without
revalidating immutable resource joins. Source CPU preparation, GPU admission,
and frame preparation have separate folder modules; visibility, pose, attachments,
lighting and effects retain their existing activity requirements and ordering.

Normal and shadow pipelines consume a packed 336-byte instance stream. Adjacent
compatible opaque/alpha-test draws share an indexed submission. Distinct receiver
indices may share a batch when their resolved lighting is equal. Geometry,
pipeline, texture, lighting or effect-order boundaries end the batch. This does
not globally reorder visible geometry or batch transparent effects. The GPU
regression compares 16 instances in one submission against 16 separate draws,
requiring visible geometry and identical captured pixels.

Resident model sources and prepared UI frames pin their GPU resources. Last-owner
release queues invalidation and a graphics completion fence. Retired images,
meshes and dependent descriptors remain allocated until that fence completes;
live registries remove obsolete metadata instead of accumulating tombstones.
Character descriptor pools reuse completed slots. Authored BLP path caches and
finite pipeline/sampler caches remain shared common resources. On-demand usage
reports count live resources and payload capacity, plus pending retirement batches;
the byte totals exclude retired batches and driver allocation overhead.

UI mesh growth queues an allocation transfer and retires old buffers at completion.
M2-only/portrait frame storage grows on each reused frame slot after its ordinary
fence. Unified world frame storage already used this slot-local growth policy;
it was retained. Surface/depth recreation can still require device idle.

Required formatting and workspace Clippy (all targets/features, warnings denied)
passed. The all-feature workspace suite passed 1,380 tests with 28 intentionally
ignored. This includes 192 renderer integration tests and the runtime's shadow,
portal, minimap and placement-publication consumers. Retained UI upload tests
also cover simultaneous prefix changes and appended bytes; the generic single-
source replacement contract remains separate from multi-page glyph replacement.

Matched live measurements, packaging and push remain required. No new live
performance improvement is claimed yet.
