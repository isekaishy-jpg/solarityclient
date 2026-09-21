# Unit effect source continuation

Unit water and environmental effects now use the same namespace M2 request
authority as appearance, terrain and sky sources. The previous loader constructed
an entire effect bank in one worker operation using a local model cache. A joined
model request now supplies a readiness dependency, releasing the worker until its
producer publishes. A new producer still decodes in the admitted source turn.

The operation retains its mounted archive, texture cache and selected effect
records across turns. It yields after definition discovery, model resolution and each derived effect build.
The five authored water names precede ascending unique environmental visual IDs,
as in the replaced loader. Missing definitions and absent model paths retain their
existing omission behavior. Model path errors remain terminal; model/derived
source failures keep the existing warning and omission policy. Executor failures,
abandoned producers and encoded/decoded storage refusals remain terminal pipeline
errors; they never silently omit authored effects. Simulation,
animation callbacks, RNG and effect retirement are unchanged.

Only a complete CPU source bank reaches GPU warmup. Warmup keeps its existing
one-pipeline-per-service-call progression and publishes the bank only when all
sources have been processed. Required metadata and completion-record allocations
carry CPU reservations; completion storage stays charged through GPU warmup.
Archive-returned encoded bytes retain the previously connected read admission.
The shared M2 authority also charges retained decoded model payloads. Texture cache
allocations and nested definition strings still need accounting, and one source
decode or derived build remains indivisible.

Cancellation of a suspended consumer drops that consumer's dependency without
abandoning a producer owned elsewhere. Controlled one-worker archive fixtures
exercise this suspension, unrelated-job progress, alias identity, withdrawal and
completion-storage release, abandoned-producer failure and decoded-storage refusal.
A frozen serial fixture checks effect order and source identity. Existing water,
mount and environmental effect scene
fixtures enter through the production continuation as well.

The runtime folder separates world registration, gameplay events and source
publication. The renderer effect folder owns the preparation cursor and GPU
warmup. Tests remain under the runtime tests tree.

Formatting and full workspace Clippy with warnings denied pass. The full workspace
test suite passes 1,694 tests, with zero failures and 33 existing ignored tests;
doc tests pass as well. Test evidence is in ignored
`target/effects-test.{stdout,stderr}.log`. Subsequent combined-source linting also
passes, recorded in `target/source-integration-clippy.{stdout,stderr}.log`.
This source has no measured FPS gain or numbered Testing package yet.

The single-branch consolidation passes formatting, full workspace Clippy with
warnings denied and all 1,706 workspace tests (zero failures, 33 existing ignores),
including the original-producer failure and decoded-admission regression tests.
Final evidence is in `target/consolidation-test-final.{stdout,stderr}.log`.
