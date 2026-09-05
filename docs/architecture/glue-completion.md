# Login-to-world completion scope

The current slice covers login, character selection and creation, and the
initial loading transition into a ready world. Stock-correct effects, lighting,
animation, randomization, failure behavior, and code/architecture are part of
completion. The working performance target is approximately 1,200 completed
frames per second (0.833 ms), with particular attention to visible stalls while
switching characters or changing customization. A fast idle average does not
establish transition performance.

On September 5, after testing installed revision `bfa15ad`, the user reported
that performance "seems good enough." This accepts the current performance
for the active slice. Further throughput tuning is no longer a completion
gate; reopen it only for a new reported or reproduced regression. Historical
measurements and their unresolved outliers below remain diagnostic evidence,
not a reason to disregard that acceptance.

The pinned build-12340 executable and exact stock data remain the behavioral
authority. SolCL is an implementation reference that can be incomplete or
incorrect. Fallbacks require stock evidence unless an explicit product decision
authorizes a divergence.

## Reconciled testing baseline

The user tested revision `43014404ea9320171e1e5989914da0ed3ad3165a` and reported:

- text, tooltips, and typing working without observed artifacts or noticeable
  performance drops at roughly 1,000–1,100 FPS;
- working scrollbars and the correct login dragon sounds;
- a working movie → EULA → login → character selection → world sequence;
- character creation generally working and close to stock;
- noticeable stalls when switching selected characters or changing
  race/class/customization;
- missing Night Elf scene effects, uncertain scene lighting, and incomplete
  animation selection, looping, and pet behavior;
- roughly 800 FPS in the Blood Elf scene versus 1,000–1,100 in the Night Elf
  scene, without a perceptible steady-state difference.

These observations close the reported defects in the tested cases. Older
handoff lists must not reopen them without a reproduction. Conversely, a
commit title, API implementation, or passing narrow test does not establish
complete parity. Character-creation randomization still needs comparison with
stock beyond the recovered contracts in
[character creation](character-creation.md), and previously unimplemented world
features were not tested.

## Remaining evidence and implementation work

| Area | Required completion evidence |
| --- | --- |
| Character and customization transitions | Performance accepted by the user on `bfa15ad`; retain complete scene publication and investigate new regressions only |
| Scene effects | Authored emitter/ribbon coverage and recovered simulation/render rules, including the missing Night Elf ground effect |
| Lighting and cameras | Stock scene inputs, light-bank assignment, FOV, placement, and material behavior checked against the actual presentation |
| Animation | Stock sequence selection, repetition, variation, blend, and pet behavior; preservation of timelines across compatible appearance changes |
| Creation randomization | Recovered randomization rules, PRNG consumption order, selection persistence, and class/race constraints |
| Initial world entry | Correct map/loading-art selection and stock-derived missing-input behavior; readiness includes the complete first world presentation |
| Engineering | Coherent ownership and dependency boundaries, bounded work, appropriate stock-contract tests, formatting, Clippy, and workspace tests |

After this slice, continue the identified world-transition and in-world work:
return/logout and menus, world UI, locomotion and animation blending, swimming
and shoreline transitions, camera behavior, and NPC movement/state correction.
Manual testing is scheduled only when the user says they are ready; autonomous
evidence recovery, implementation, and automated validation continue meanwhile.

## M2 transfer ownership

The September 4 testing logs contain Glue character GPU-publication spikes of
approximately 20 ms. BLP and mesh registries already deduplicate resource
identities, but admission of a new M2 mesh synchronously waited for its transfer
fence. That wait also includes earlier work on the same graphics queue.

M2 mesh admission now records transfer-to-vertex/index barriers and queues the
copy without a host wait. Its registry retains staging allocations, command
pools, and fences; upload and presentation boundaries poll completed transfers,
and shutdown retires all outstanding work before freeing resources. The mesh
itself stays resident after its staging resources retire. This follows Vulkan's
[submission-order synchronization rules](https://docs.vulkan.org/spec/latest/chapters/synchronization.html),
without changing stock model bytes, effects, or scene content.

`benchmark_m2_uploads` measures cold and cached geometry admission separately
from the following clear-frame presentation, using explicit installed-data and
model paths. It is a mesh-admission benchmark, not a complete Glue-scene FPS or
character-transition benchmark. The GPU integration test also covers drawing
newly admitted meshes between frames and shutdown after an unused admission.

The initial GTX 1070 run reduced Human male cold admission from 7.71 ms to
2.78 ms, while the following clear-frame presentation increased from 0.62 ms
to 4.72 ms. Other cold admissions ranged from 0.13 to 1.01 ms after the change;
cached admissions remained below 0.004 ms. These single-run measurements show
that deferred submission can move work to a later presentation boundary. They
do not establish the complete-frame target or eliminate the need to profile
full character/customization transitions.

## Complete Glue transition replay

`benchmark_glue_transitions <following-frame-count> <output.csv>` followed by
the normal runtime arguments drives stock screen events and actual pointer
clicks through `ClientApplication`. It supplies an offline character directory,
including an equipped hunter and pet, and exercises cold/warm selection,
race/class changes, all five customization axes, and randomization. Use a
separate profile with startup movies/legal dialogs disabled and `gxVsync 0`
for uncapped measurements. It does not connect to authentication or realm
servers. Closing the diagnostic window cancels the replay.

Each CSV sample records input delivery time, elapsed time until the complete
requested scene is presented, every transition frame interval, and the requested
number of following frame intervals. Readiness requires the requested route
and all requested model resources, plus admission of the current music and
ambience; an old resident scene cannot satisfy it.
Measurements end at the normal Vulkan presentation call, not physical display
scanout. The controlled replay excludes network polling and the frame limiter.

The September 4 GTX 1070 comparison used 1280 by 720, audio enabled, and 1,000
following frames per action. Pointer press/release previously took full Lua
snapshots even for one changed checkbox. Those callbacks now use the same typed
mutation journal as events, preserving callback order and a full publication
when scripts create regions or make unclassified changes.

| Interaction | Input before | Input after |
| --- | --- | --- |
| Customization arrows | 40–44 ms | 2.3–3.1 ms |
| Race/class choices | 121–143 ms | 34–50 ms |
| Warm selection | About 56 ms | 3.6–5.6 ms |

These are individual local replay runs, not a hardware-independent guarantee.
Following-frame means after the change ranged from about 1,350 to 2,080 FPS,
with occasional 16–18 ms outliers. Warm selection still had 49–54 ms maximum
transition frames, and race changes still rebuilt the complete UI mesh. The
1,200 FPS target therefore does not establish stall-free transitions. Follow-up
profiling identified full processing of roughly 32,000 retained glyph quads
during otherwise small layout changes as remaining synchronous work.

Regression coverage verifies native checked state, merged hover/click texture
mutations, and immediate hit testing of a button created by an `OnClick` handler.

The subsequent icon-layout pass retains the pressed shadow and bevel geometry,
including dependent texture anchors. The same replay measured race/class input
at about 21–35 ms, with a 4.4 ms unchanged-class click; full content changes still
rebuild the text mesh. Customization remained about 2.4–3.9 ms. The native
checkbox dispatch correction and its executable evidence are described in
[character creation](character-creation.md#native-choice-buttons).

`SOLARITY_FRAME_TIMINGS` now also reports individual Glue audio action costs.
The warm Night Elf and Human switches spent 46.4 and 39.6 ms respectively
inside `PlayGlueAmbience`, accounting for most of their 53.8 and 49.3 ms
maximum transition frames. Cold Tauren ambience admission took 141 ms in the
same run. Per-phase profiling then isolated 38–45 ms of the warm pause in MPQ
extraction, with WAV decoder and backend setup taking about 1–2 ms. Glue music
and ambience now perform archive extraction on the CPU pool with ordered sound
selection and cancellation of replaced requests. The nonblocking executable
evidence and remaining synchronous decoder work are documented in
[audio content loading](audio-content-loading.md).

With worker reads, the same 1280 by 720 GTX 1070 replay measured warm Night Elf
and Human maximum transition frames of 7.9 and 11.4 ms, compared with 53.8 and
49.3 ms before this change. Complete readiness still took 58.7 and 53.3 ms:
the benchmark now explicitly waits for current audio admission while recording
every intervening present. WAV completion took about 1–2 ms on the main thread.
Following-frame means ranged from about 1,415 to 2,110 FPS. Two following-frame
outliers reached 24–28 ms, cold selection still reached 58 ms, and race/class
text publication still produced roughly 25–38 ms frames. These results narrow
the remaining stalls; they do not establish the full transition/FPS goal.

The text publication pass now retains glyph geometry per text owner, skips
hidden owners before traversing their glyphs, and orders contiguous glyph runs
without sorting every quad payload. Mixed text, icon UV, vertex-color, and
backdrop-color updates validate and reuse existing mesh slots. A newly shown
region that needs new slots joins the content publisher's single rebuild.
Profiling had found approximately 224,000 live glyph quads, mostly hidden;
growing one description previously copied that entire arena. The measured
object replacement phase fell from about 9 ms to 0.06–0.21 ms, and packet
ordering fell from about 3.5–5 ms to 0.36–0.59 ms in the diagnostic replay.

The final 1,000-following-frame replay, run separately from compilation, measured
race-choice input at 13.5–20.0 ms and class-choice input at 8.5–18.2 ms; the
repeated Warrior choice took 8.8 ms. Customization arrows took 2.5–3.1 ms.
Race/class maximum transition frames ranged from 11.2 to 23.8 ms, while warm
selection reached 7.6 and 9.6 ms. Following-frame means ranged from about 1,450
to 2,400 FPS. Cold Human selection still reached a 68.7 ms frame and 631 ms
complete readiness; its resource loading and synchronous audio decoder
admission remain separate work. These local results improve recurrent input
cost without closing the no-visible-stall requirement. Regression tests cover
retained glyph capacity, exact stable draw ordering, and mixed texture updates;
the installed-data Glue interaction validator and all 529 workspace tests pass.

SDL decoder admission now also runs on the bounded CPU executor. In the next
1,000-following-frame replay, the WotLK title MP3 took 37.7 ms to prepare on a
worker. Aligning that completion log with the recorded presentation intervals
shows approximately 65 frames across its decode window, with a 1.75 ms maximum;
the previous synchronous 35.1 ms decode occupied one 37.0 ms frame. This is a
phase-specific improvement: the complete login transition still reached
45.3 ms and cold Human selection 67.7 ms in the new run. Cold archive extraction
also varied substantially between runs. Following-frame means ranged from
about 1,710 to 2,440 FPS, and warm selection maximum frames remained 7.7 and
9.5 ms. Full scene readiness still waits for current audio admission. Decoder
cancellation, capacity, residency, playback, and shutdown tests bring the
passing workspace total to 535; formatting and Clippy also pass.

## Shared backdrop archive residency

Cold selection still synchronously loaded its backdrop M2, primary SKIN, and
BLPs through the presentation owner's archive store, even when the separate
racial prewarm worker had decoded that path. The measured Human and Night Elf
archive phases took about 19 and 14 ms respectively.

Selected and speculative backdrop requests now share a worker-private archive
owner and a result cache. That owner mounts the discovered archive catalog once,
retains decode caches, and processes one finite model request at a time. A new
selection preserves the active request's eventual result and takes priority
before the next optional path. Queue pressure returns a pending scene without
blocking or discarding completed assets. Shader preparation remains on the CPU
pool; Vulkan admission and complete scene publication remain on the presentation
thread. The hidden login prewarm before movie playback retains its explicit
startup wait so the cinematic still advances the existing login effects.

The scene retains character replacement intent across archive waits and keeps
the previous complete presentation until the requested generation is ready.
Missing optional backdrop models do not prevent other racial prewarms. Exact
model failures are retained for demand, while authored BLPs and stock white/green
texture fallback resolution use the existing loader contract. Regression tests
cover capacity refusal without a main-thread mount, changed selection, exact
failure retention, and reuse of decoded models and textures. All 542 workspace
tests, formatting, and Clippy pass.

The next isolated GTX 1070 replay at 1280 by 720, with audio and 1,000 following
frames per action, recorded these maximum transition intervals:

| Cold selection | Previous | Shared worker assets |
| --- | --- | --- |
| Human | 67.7 ms | 49.3 ms |
| Night Elf | 27.9 ms | 16.7 ms |
| Blood Elf | 37.3 ms | 13.5 ms |

The log records one archive job for each of the login and eight racial
backdrops, all reused by subsequent selection. Warm selection remained at
8.7 and 10.6 ms maximum frames; following-frame means ranged from about 1,570
to 2,420 FPS. Human complete readiness still took 633 ms, including the stock
screen transition, and creation entry reached a 33.7 ms frame. These single-run
results establish removal of duplicate backdrop reads, not stall-free Glue.
A second complete replay with one CPU worker and capacity one also passed all
28 actions, exercising deferred admission through real installed assets.

## Text residency across visual reveals

The remaining approximately 50 ms Human-selection frame occurred when the
stock fade revealed CharacterSelect. Targeted runtime resource timings measured
only 3.6 ms in GPU resource preparation. The earlier visual-topology rebuild
spent 37.5 ms checking glyph coverage and laying out every text owner again,
including unrelated hidden legal and credits text.

A pure visual journal changes visibility, alpha, and animation transforms while
preserving text and logical bounds. The glyph owner already retains local quads
for hidden text, and mesh resolution applies current presentation transforms
and clipping. First reveal now materializes draw topology using those retained
quads. Content and geometry mutations continue to refresh text through their
existing journals. `SOLARITY_UI_TIMINGS` reports reveal publication separately;
the transition replay also accepts `RUST_LOG`, including a targeted
`solarity_runtime::application::login_ui=debug` directive for GPU resource phases.

The installed-data interaction validator passed after the change, including
glyph bounds, clipping, legal screens, login input, selection labels, and creation
text. All 542 workspace tests, formatting, and Clippy also passed. Two isolated
1,000-following-frame replays measured cold Human maximum frames of 13.7 and
14.1 ms, compared with 49.3 ms before this change. Reveal publication itself took
6.3 and 6.6 ms. The confirmation run used `RUST_LOG=info` to match the earlier
benchmark logging policy and measured following-frame means of about
1,580–2,420 FPS.

The first run also recorded a separate 103 ms login frame, a 19 ms customization
frame, and following-frame outliers up to 15 ms. Those did not repeat in the
confirmation, whose login frame reached 26.5 ms and customization frames
3.8–4.6 ms. Creation entry still reached 29.8 ms and race/class changes up to
23.3 ms. The glyph-reveal improvement is repeatable in these local measurements;
the broader requirement to eliminate noticeable stalls remains open.

## Retained source-run replacement

Race/class publication still rebuilt the complete roughly 34,000-quad UI when
one description grew or an icon selected another texture. The class-description
trace recorded an increase from 3,190 to 3,690 glyph quads, followed by a complete
mesh rebuild costing about 8 ms.

The renderer can now replace a single contiguous object/source run, including
its size or material, while preserving neighboring vertex payloads and batch
order. Resizing relocates later quad offsets and object lookups, updates the
canonical index prefix, and invalidates old byte-range revisions. Adjacent
state batches can merge or split within that source run, including the reserved
caret slot used by initially empty labels. Interrupted source ranges and
multi-source replacements retain the complete-publication path. UI text uses
this operation when fixed slots no longer fit; text batch changes and
single-source icon replacements also refresh the texture-request plan so GPU
publication retains correct batch associations and binds the selected asset.
Layout, draw-layer, and hierarchy changes retain their existing publication rules.

Regression tests compare resized geometry and batches against a freshly built
reference, exercise growth followed by shrinkage and later targeted writes,
verify atomic rejection of invalid/interrupted replacements, merge and split
state batches with subsequent caret and neighboring-object updates, and check
texture-request rebinding after a real Lua icon change.

All 545 workspace tests, Clippy, formatting, and the installed-data interaction
validator passed. The final 28-action replay used 1,000 following frames per
action, the GTX 1070 at 1280x720, four CPU workers, capacity 64, audio enabled,
and `RUST_LOG=info`, matching the preceding baseline. The first Warrior class
change fell from a 22.4 ms maximum frame and 19.3 ms input action to 15.7 ms and
11.6 ms. An earlier single-batch version measured 15.8 ms and 11.9 ms for the
same change; it still rejected an initially empty race label, which the final
version accepts. No glyph-slot rebuild rejection appeared in the final trace.

Other race changes still reach the complete publisher: final Human creation
switching reached 21.3 ms, Blood Elf 23.9 ms, and creation entry 26.7 ms. The
earlier single-batch replay also recorded a 45.5 ms Blood Elf frame, with
25.5 ms spent serializing the complete mesh; that outlier did not recur in the
final run. Final customization/randomize maximum frames were 3.9–4.7 ms,
following-frame means were about 1,610–2,400 FPS, and login reached 43.1 ms.
These measurements verify the class-description improvement and leave the
remaining complete-publication paths and startup stalls open.

## Color-only overlays and race-switch residency

Targeted diagnostics identified the class buttons' ordinary hidden
`DisableTexture` regions as the recurring missing visual slots. The installed
`CharacterCreateIconButtonTemplate` defines these as black `Color` textures
with alpha 0.75. The native plan previously copied that color only into vertex
tint and left the texture without a source, so the overlay never rendered and
every reveal could demand another full publication. Recovered XML loader
behavior and the corrected source/tint ordering are documented in
[UI content loading](ui-content-loading.md#typed-texture-plan).

Color sources now survive XML inheritance, startup registration, and dynamic
templates. Texture children retain zero-opacity slots while their owning frame
is presented, so race availability can toggle the disabled overlays without
discarding geometry. Empty texture regions do not demand nonexistent draw
slots. External tests cover source replacement versus file tint, inherited
empty file attributes, dynamically created color textures, and repeated mixed
visibility/widget callbacks with unchanged mesh bytes and identity. All 547
workspace tests, Clippy, formatting, and the installed-data interaction
validator passed.

Two final 28-action replays used 1,000 following frames, 1280x720 on the GTX
1070, four CPU workers, capacity 64, and audio. Both retained the UI through
every race change; only initial screen reveals needed missing topology.
Human creation switches measured 16.9/16.1 ms maximum frames, Night Elf
12.0/11.2 ms, and Blood Elf 17.9/18.1 ms, versus the preceding run's
21.3/19.2/23.9 ms. Following-frame means remained approximately 1,590–2,440
FPS. The second run additionally enabled targeted UI GPU-publication debug
timings. The stock creation screen randomizes its initial state, so initial
entry and the first race/class change do not have identical resource histories
across these runs; the retained-publication traces establish the removed work.

Remaining outliers are recorded separately: the first run reached 86.0 ms at
login (56.9 ms in full mesh serialization) and 103.6 ms at creation entry. The
confirmation reached 38.7 and 33.6 ms respectively, with creation UI resource
preparation taking 8.4 ms. Both runs recorded a following-frame pause during
the first customization axis's return step, at 25.7 and 21.7 ms, hundreds of
frames after readiness. That pause needs attribution; the confirmed run's
renderer/model phase maxima do not explain its full duration. This change
restores an authored overlay and removes repeated race UI rebuilds, but does
not establish the complete no-stall requirement.

## Slow-frame attribution and unchanged audio policy

`SOLARITY_FRAME_TIMINGS` now attributes individual application frames above
5 ms across event polling, UI updates and uploads, CVar persistence, audio
service, character/model preparation, and presentation. On Windows, slow
scopes also report CPU cycles charged to the calling thread. Cycle counts are
not converted to time or treated as a calibrated CPU frequency. Native SDL
poll diagnostics report whether an event was returned without logging typed
text or other event contents. Fast frames do not allocate profiling records.

Extended replays showed that the later pauses did not have one consistent
application phase: one 28.3 ms frame spent 24.5 ms polling platform events,
another spent 25.2 ms applying audio settings, and a later 23.5 ms frame spent
22.7 ms in presentation. Additional thread accounting measured an 8.4 ms audio
settings scope at about 859,000 cycles and a 9.6 ms empty SDL poll at about
754,000 cycles. This evidence does not support attributing all pauses to Lua
GC or scene publication. An initial ambience-fade timing hypothesis was also
not supported by the current runtime path.

The audio investigation exposed redundant gain/pool work for unchanged CVar
snapshots. Removing that work preserved stopped-voice collection and residency
maintenance. All 547 workspace tests, Clippy, and formatting passed. A final
28-action replay with 4,000 following frames per action measured warm Human
following throughput at 3,574 FPS, compared with the instrumented baseline's
2,395 FPS. Most creation/customization scenes measured 3,400–3,500 FPS, versus
approximately 2,200–2,400 before. This run's maximum following frame was
4.4 ms; creation entry still reached 25.8 ms and race changes up to 17.3 ms.

A 6,000-following-frame confirmation retained the 3,400–3,500 FPS range until
the Blood Elf switch chose Death Knight through the unavailable-class fallback.
That scene added armor/effect work and retained its class backdrop through
later race switches, measuring approximately 2,200–2,400 FPS with over 1,000
particle vertices. It is a different workload from the ordinary race scenes.
The confirmation also recorded a 23.1 ms following frame: its native SDL poll
took 21.6 ms, returned no event, and charged only about 65,000 thread cycles.
The remaining platform wait or scheduling pause is not eliminated by the
mixer optimization. Initial entry, input publication, and the outstanding
stock scene/world requirements remain part of completion.

## Night Elf foreground particle placement

The two foreground `PARTICLES\DUST3X.BLP` emitters were admitted, simulated,
and submitted, but their generated geometry fell outside the authored camera.
The model-owner recovery at build-12340 `0x008309C0` exposed a missing fixed
90-degree generator-basis rotation after the bone, emitter-position, and model
transforms. The shared bone-pose boundary now supplies that exact transform;
it is used by all placed M2 particle emitters, including world placements.
The stock source and composition contract are recorded in
[M2 effects](m2-effects.md#particle-generator-basis).

An installed-data diagnostic advances each emitter at 60 Hz for 20 seconds
with fixed seeds and samples CPU geometry every five seconds. The Night Elf
emitters 10 and 11 retain 54 and 36 live particles at 20 seconds. Their
in-frustum vertex counts change from zero to 147/216 and 124/144 respectively;
both also enter the frustum at 5, 10, and 15 seconds. All generated vertex
positions remain finite. Login, the eight racial backdrops, and Death Knight
complete the same simulation/mesh diagnostic without errors, covering 118
emitters. A whole-triangle check also rejects every original dust triangle
against at least one clip plane at each sample. After correction, 91 and 62
triangles respectively survive that initial rejection at 20 seconds.

All 548 workspace tests, Clippy, formatting, and the optimized build passed.
The 28-action replay completed with 6,000 following frames per action,
1280x720 on the GTX 1070, audio, four CPU workers, and the same frame diagnostics
as the preceding run. Night Elf selection measured 2,477/2,518 FPS cold/warm;
Night Elf creation measured 3,266/3,279 FPS, and ordinary Human customization
approximately 3,284–3,432 FPS. These figures retain throughput above the
target with the corrected particle placement. Creation entry still reached
26.9 ms, race changes up to 20.9 ms, and following-frame outliers up to
19.5 ms. This effect correction does not close the remaining stall work.

These measurements establish the misplaced-emitter defect and its stock-based
correction. They do not establish the final blended appearance or full scene
parity. Automated window capture was unavailable because the computer-use
helper could not connect; visual comparison remains open with lighting,
animation, and the other completion requirements above.

## Authored directional light pose

Build-12340 `0x00828A00` publishes point lights from their authored position
through the owning bone and model placement. Its directional branch at
`0x00828B07` instead reads the negative Z column of the bone matrix and
applies only the placement's linear transform. The previous shared sampler
incorrectly treated the light's position field as a direction. It now follows
the recovered branch for placed M2 inputs; the runtime currently calls this
sampler for the Glue backdrop only. Publishing authored lights from world M2
placements remains an unimplemented world-rendering requirement.
`0x00834AE0` normalizes only when squared length exceeds the float at
`0x009EA27C` (`0x34800000`, twice `f32::EPSILON`); smaller vectors keep their length.
Both stock branches index a real bone without a sentinel check. Sampling an
unbound visible light now returns the existing missing-bone error instead of
silently substituting an identity matrix.

External regressions cover animated bone rotation, translated and rotated
model placement, tiny/zero/scaled directions, animated colors, both unbound
light types, and the existing point-position/storage-reuse behavior. The
installed-data geometry diagnostic now reports authored lights and sampled
direction/color values. All eight distinct racial backdrops, Death Knight,
and login load and sample successfully at 5,000 ms; their light records all
reference real bones. Human, Dwarf, Blood Elf, Draenei, Tauren, and login have
nonzero diffuse directional output at this sample. Night Elf's directional
source has zero diffuse output, so correcting that source's direction alone
does not demonstrate a visual change there. Final appearance, per-model
light-bank composition, and the remaining completion requirements stay open.

## Default ghost lighting

The ModelFFX callback at `0x004E3A20` distinguishes an authored directional
override from an absent ghost override. `0x004E2730` clears directional
accumulation while retaining the collected point lights. The default ghost
branch instead calls `0x00834900`, clearing the entire light accumulator,
then creates a negative-Z D3D directional source. It samples LightParams row
three at time zero through `0x007EBF30`: color channel zero supplies diffuse
and channel one supplies ambient. The renderer receives the inverted +Z
surface-to-light direction.

`LightCatalog::model_light_colors` now provides this direct-parameter sampling
without a world-volume lookup or requirements on unused sky/scalar channels.
The Glue scene loads the default palette once. Each of the background,
character, and pet ghost banks uses a complete replacement when it has no
authored lights; an explicit bank replaces only the directional contribution.
The previous default selected a camera-facing warm character light and kept
the backdrop's point lights. The replacement also selects the one-light
character/pet shader permutation.

External regressions cover exact parameter/channel IDs, time-zero colors,
cyclic band interpolation, missing parameters, directional inversion, and
point-light retention versus complete replacement. The offline transition
replay now includes cold/warm selection of a ghost hunter with a pet and the
return to the live hunter. This exercises production selection dispatch and
Vulkan presentation; it does not by itself establish visual equivalence to a
stock capture.

All 551 workspace tests, Clippy, formatting, and the optimized runtime/replay
build pass. The 31-action replay completes with 6,000 following frames per
action at 1280x720 on the GTX 1070, four CPU workers, audio, and frame timing
instrumentation. The installed default ghost palette is ambient
`(26, 56, 85) / 255` and diffuse `(94, 153, 198) / 255`. Ghost selection
measures 2,283 FPS cold and 2,244 FPS warm; its transition maxima are 6.5 and
6.7 ms. The return to the live hunter completes at 2,410 FPS. Creation entry
still reaches 29.5 ms, and a later following frame reaches 30.6 ms. These
measurements retain throughput above the target without closing the remaining
stall or visual-parity requirements. Palette sampling failures are retained
until a bank actually requests the default, matching the conditional stock
lookup; ordinary login and explicitly authored banks do not require it.

## Rendered framebuffer evidence

`VulkanRenderer::request_frame_capture` and `take_captured_frame` provide an
explicit diagnostic copy of the next successfully presented framebuffer.
Every presentation path copies after its final effects/UI pass and before
presentation. The request owns a host-readable buffer until GPU completion,
handles pending swapchain recreation, and cannot be overwritten by later
frames. Ordinary rendering performs no readback. Collection deliberately
waits for GPU idle; these diagnostic waits must remain outside performance
measurements. The captured RGBA8 channels are the actual stored image, before
desktop composition and display gamma.

Set `SOLARITY_GLUE_CAPTURE_DIR` when running `benchmark_glue_transitions` to
export one numbered PPM after each step's following-frame measurements. The
example prints the step-to-file mapping. Use a separate run without that
variable for performance evidence, because capture waits and file writes alter
the state between steps even though they are outside the recorded intervals.
The exports include the complete framebuffer without scaling or color changes.

A GPU regression presents a known two-dimensional color pattern, then presents
a different frame before collecting the capture. It checks every original
channel and pixel, request exclusivity, pending behavior after rejected input,
one-shot consumption, opaque-black clear capture, and unsubmitted teardown.
This validates actual GPU readback rather than a CPU reconstruction.

The first installed-data run produced all 31 captures at 1280x720, including
login, live/ghost hunters with a pet, Human and Blood Elf selection, character
creation, class/race changes, and customization. Inspection shows the Night Elf
selection head/helmet clipped at the upper framebuffer edge, while Human and
Blood Elf selection fit vertically. Night Elf creation also reaches the upper
edge. This is now a concrete camera/placement audit target; its stock comparison
and cause remain unproven. The live and ghost lighting changes are visible, but
the exports alone do not establish stock equivalence. No stock frame capture
was available from the Windows automation helper during this run.

All 552 workspace tests, Clippy, formatting, and the optimized replay build
pass. A separate capture-disabled run completed all 31 actions with 6,000
following frames each on the same GTX 1070 setup with timing instrumentation.
Following throughput ranged from 2,307 to 3,656 FPS. Creation entry still
reached a 26.6 ms transition frame and a 30.2 ms following frame. The diagnostic
therefore preserves throughput above the target but does not close the stall,
camera, visual-parity, or world-entry requirements.

## Native camera and owning-model transform audit

The stock projection path `0x4bf0c0 -> 0x4becf0 -> 0x6bfe00 -> 0x6bf370`
uses the actual viewport aspect and divides authored diagonal FOV by
`sqrt(1 + aspect * aspect)` before constructing the perspective matrix.
This matches the renderer's existing FOV conversion. An additional installed
capture at 960x720 fits the Night Elf hunter's helmet and pet; the 1280x720
capture clips vertically. That viewport dependence alone does not establish
a placement defect, and it does not justify an arbitrary character scale fix.
Stock image equivalence remains unverified.

A separate, proven mismatch concerned explicit Model widget transforms.
Stock `0x95fba0` forwards widget rotation and scale into the owning model via
`0x8251d0`. Camera publication in `0x828a00` transforms animated eye and target
through the model-view matrix at model `+0xf4`, then the inverse scene view
at scene `+0xc4`, producing world-space camera positions. Roll is published
separately. Camera initialization `0x832ea0` sets authored near/far properties
without multiplying them by model scale.

The runtime now applies the retained widget rotation to backdrop geometry,
and samples both visible and cinematic-covered cameras using the same root
placement transform. Previously rotation was retained only in the generation
key, and scale affected geometry while leaving its authored camera unchanged.
The external camera fixture checks transformed animated eye/target, unchanged
clip distances, and invariant screen placement when camera and geometry are
scaled, rotated, and translated together. Default identity placement retains
the verified projection; this correction does not claim to resolve the Night
Elf framing question or the separate effects/lighting parity requirements.

All 552 workspace tests, Clippy, formatting, and the optimized replay build
pass with this correction. The capture-disabled 31-action replay with 6,000
following frames per action measured 2,156–3,596 FPS on the GTX 1070 setup.
Creation entry reached 29.7 ms, and one customization following frame reached
28.9 ms. These results preserve throughput above the target while retaining
the unresolved stall requirement.

## Native BGRA8 Glue button texture

The configured texture prewarm rejected
`Interface/Buttons/UI-PaidCharacterCustomization-Button.blp` from the installed
`enUS/patch-enUS-2.MPQ`. Its BLP2 header declares direct content, RAW3 pixels,
eight alpha bits, and native pixel format 2. The `wow-blp` 0.7 header parser
names the last field `AlphaType` and rejects 2 before reading any pixels.

Stock `0x6ae900` accepts the BLP2/version-1 header. `0x4b5fe0` maps native
format 2 and format 8 with eight alpha bits to the same BGRA8 output, and
`0x6affd0` can publish RAW3 mip addresses directly. The asset parser now
translates this exact header combination into the dependency's supported
format-8 representation using its owned archive-read buffer. It retains the
authored pixels, mip offsets, archive bytes, and ordinary parser validation.
This does not reinterpret other pixel formats or compressed payloads.

The regression checks fractional and zero alpha, channel order, every pixel
of two mip levels, unchanged archive content, and rejection of truncated or
unsupported input. The common raw-texture fixture now uses native format 2.
An installed-data diagnostic also verified all 21,845 pixels across all eight
mips of the actual 128x128 button against the source BGRA bytes.

All 554 workspace tests, Clippy, formatting, and the optimized replay build
pass. The installed replay's configured Glue prewarm admits 93 textures with
zero failures, including the previously rejected button.
All 31 actions with 6,000 following frames complete. With client-service info
logging enabled to observe admission, following throughput ranges from 1,894
to 2,997 FPS; creation entry reaches 29.7 ms and a later customization frame
reaches 36.5 ms. This is decode/admission evidence, not a measured speedup;
the slower throughput versus the preceding quiet replay and the remaining
long frames still need attribution.

## Empty-label glyph residency

The follow-up UI trace separates creation entry into 9.1 ms of authored
script work and 12.1 ms of publication. Publication resolves and serializes
34,475 glyph quads; mesh preparation alone takes 7.8 ms. Hidden tooltip
templates reserve 128 letters per text field, and other empty FontStrings
reserve 64, multiplying that capacity by outline and shadow passes. These
unused slots are processed again on unrelated screen publications.

Empty labels now seed a minimal source run. The existing source-run mesh
replacement expands it when text first needs more glyphs, preserves unrelated
vertex payloads, and retains that owner's observed peak for later shorter
text. Only EditBoxes reserve their existing typing capacity and caret slot.
No visible glyph, text limit, wrapping, outline, or shadow rule changes.

The installed interaction validator checks every class tooltip twice. Both
passes must show the authored text and preserve vertices outside the tooltip;
the second pass must retain the warmed shared topology. First-run agreements,
login input, character selection, creation layout, and scrollbars also pass.
All 554 workspace tests, Clippy, and formatting pass.

The matching UI diagnostic replay completes all 31 actions. Creation entry
now resolves 9,892 glyph quads; glyph resolution takes 1.56 ms, serialization
1.11 ms, and mesh preparation 2.92 ms. Its complete UI publication takes
6.82 ms. Initial creation randomizes race/class, so different model work and
resource history prevent treating the total ready duration as a controlled
before/after comparison. The reduced unused glyph residency is directly
observed in the mesh count and publication trace.

The separate 31-action, 6,000-following-frame replay disables UI timing output
and retains slow-frame phase attribution. Creation entry reaches a 17.4 ms
transition frame, compared with 26.6 ms in the preceding replay with the same
logging filter. Following throughput ranges from 2,023 to 2,894 FPS. Ordinary
customization transition maxima are 3.6–4.7 ms; Blood Elf creation still reaches
16.7 ms. A separate 40.9 ms following-frame outlier spends 40.1 ms in platform
event polling and charges about 2.5 million thread cycles across the entire
frame. The UI residency improvement does not resolve that platform pause or
establish the complete no-stall requirement.

## Platform-pause scheduling evidence

An attempted Windows CPU trace did not start. Xperf rejected the kernel
flags, and the built-in WPR CPU profile returned `0xc5585011`, reporting that
it could not enable the system-performance profiling policy. No trace logger
remained active. This prevents the context-switch stack trace needed to
identify the complete wait/scheduling chain in this environment.

A separate diagnostic launched the benchmark directly and retained its exact
primary thread ID. It sampled `NtQuerySystemInformation` thread metadata
using the installed Windows SDK layouts, without suspending the process or
changing its priority. The 31-action replay completed with 20,355 snapshots.
Query duration was 1.0 ms at the median, 3.0 ms at p99, and 42.9 ms maximum;
this diagnostic overhead makes the run unsuitable for throughput comparison.

At `03:54:18.422702Z`, an empty native SDL poll took 23.60 ms while charging
75,190 thread cycles. One overlapping snapshot observed the primary thread
in state 1, Ready: runnable rather than waiting on a resource. The state
vocabulary is documented in Microsoft's
[thread-state reference](https://learn.microsoft.com/en-us/dotnet/api/system.diagnostics.threadstate).
Other long frames overlapped gaps in the sampler itself: a 24.20 ms frame
overlapped its 42.88 ms query, and a 17.44 ms native poll had no overlapping
snapshot. A sparse observation does not establish the state throughout an
interval, and wait-reason values are not interpreted outside the waiting
state.

These observations are consistent with broader scheduling delays and do not
isolate a blocking SDL function. They do not justify changing event delivery,
thread priority, or stock timing policy. The platform-pause requirement stays
open; ordinary synchronous UI and model publication remain independent work.

## Retained content geometry dependencies

The content publisher previously re-solved the complete region arena for
every icon press and release, even when the stock callback only moved its
bevel and resized its shadow. It now seeds the existing geometry dependency
resolver from the mutation journal. Parent inheritance and transitive anchor
targets propagate the update; unrelated regions keep their resolved values.
Only affected regions need dimension synchronization and dependent-movement
checks. Material, membership, and non-texture movement still receive the same
complete-publication checks as before.

The icon-button regression covers an anchor chain outside the changed
button's ownership subtree, including a forward reference in arena order.
Repeated press/release cycles verify exact dependent positions, mesh bounds,
Lua geometry queries, retained draw/index slots, and no full runtime snapshot.
Formatting, Clippy, and all 554 workspace tests pass.

The matching 31-action diagnostic completes successfully. Across ten
three-object updates without text mutations, median geometry time decreases
from 0.681 to 0.206 ms and total content publication from 1.366 to 0.864 ms.
Median presentation time remains similar, 0.426 versus 0.433 ms, consistent
with the change being confined to geometry resolution. These figures compare
the existing glyph-residency diagnostic with the dependency-update replay;
they do not establish a controlled improvement in randomized creation model
readiness or resolve the separate platform-pause requirement.

The real-data interaction validator also passes. The separate replay with
UI timing output disabled completes all 31 actions with 6,000 following
frames each. Following throughput ranges from 1,878 to 3,020 FPS, ordinary
customization transition maxima from 2.5 to 4.0 ms, and creation entry reaches
21.8 ms. A following-frame maximum of 40.2 ms remains: its inner benchmark
scope records 38.3 ms, including 31.8 ms in model/UI presentation and 5.7 ms
in platform events, with only 3.28 million thread cycles across that scope.
Other low-cycle pauses appear in presentation and UI upload. The remaining
long frames are not confined to the content geometry publisher.

## Model-file validation during creation callbacks

A call/return hook around the seeded interaction validator's create-button
press and release identifies the remaining large Lua bridge cost. The two
`SetCharCustomizeBackground` calls reach `Model:SetModel`, which previously
read and decompressed the complete archive entry before comparing the live
model path. The bytes were immediately discarded. Their combined native
self time was 3.616 ms; `Show` took 1.065 ms across 61 calls and `SetText`
0.838 ms across 43 calls. The hook includes observer overhead and does not
measure ordinary frame throughput. Its call stack closes without unmatched
returns.

Build 12340's `Script_Model_SetModel` at `0x00960530` dispatches through
virtual slot `0xe8`; both Model and ModelFFX use `0x0095F990`. This reaches
`0x0081F8F0` and the shared resource cache at `0x0081C390`. A matching cached
resource returns before the archive-open call at `0x00424B50`.

At this revision, the script method retained successful file validation within its mounted
asset-store and method-table lifetime. First reads still validate the archive
entry, failed reads do not enter the set, and decoded model resources remain
owned by the renderer's asset pipeline. The repeated background calls now
take 1.913 ms combined in the same seeded hook diagnostic. The full observed
button interval changes from 17.419 to 15.419 ms; that interval includes
publication and profiling overhead, so the native-call attribution is the
more direct evidence. A regression checks repeated selections, path spelling,
different model objects, missing files, and isolation across mounted runtimes.

This trace also identified separate stock behavior that the script bridge
did not yet fully implement: shared-model extension conversion, the
`Spells\\ErrorCube.mdx` load attempted by `0x0081F8F0` after a failed resource
request, and replacement of model instances through virtual slot `0xe4`.
The validation reuse does not establish parity for those behaviors.

Formatting, Clippy, all 555 workspace tests, and the real-data interaction
validator pass. The 31-action replay with 6,000 following frames per action
and detailed UI timing disabled also passes. Warm Human selection takes
1.73 ms for its action; creation class actions take 4.80-6.11 ms and ordinary
customization actions 1.69-2.35 ms. Following throughput ranges from 1,937 to
3,008 FPS. Creation entry still reaches a 16.0 ms transition frame and Blood
Elf creation 29.3 ms, so switching is not yet free of noticeable stalls.
The maximum following-frame interval is 20.5 ms; its inner benchmark scope
charges 2.50 million thread cycles and spends 20.37 ms in model/UI
presentation. Randomized creation state and resource history still prevent
treating full readiness times as a controlled before/after comparison.

## Model lookup and mutable instance lifecycle

Following the model loader beyond its cache lookup shows that shared-resource
initialization (`0x0083D410`) queues the file read. `SetModel` now checks archive
presence and reuses positive results within the mounted store; it no longer
reads and discards even the first model file on the synchronous Lua path.
Decode and GPU preparation remain in the existing asynchronous residency
pipeline. Missing or unsupported model paths attempt `Spells\\ErrorCube.m2`,
matching `0x0081F8F0`'s `.mdx` request after shared-cache extension conversion.
The asset layer's existing `.mdl`/`.mdx` canonicalization is shared with the UI.

Every accepted `SetModel` publishes an instance generation, even for the same
path. The runtime constructs fresh mutable playback while reusing decoded and
prepared GPU sources. Sequence state belongs to that instance and resets on
replacement; widget camera and transform properties survive. `ClearModel`
publishes an explicitly empty viewport, so the transition readiness guard
cannot leave the previous scene indefinitely visible. Visibility and alpha
updates retain that explicit-clear state correctly.

`GetModel` returns the normalized lowercase loaded filename. The null-instance
case retains the unusual build-12340 Lua result: `0x009605D0` returns one value
without pushing, so the last original argument is returned (normally `self`).
The object lookup at `0x004A81B0` leaves the original arguments intact; Lua 5.1's
call-result handling copies the topmost value for this return count. Empty
`SetModel` clears the old instance before reporting an invalid-model error.

Tests exercise both Model and ModelFFX, aliases, same-path replacement,
sequence reset, retained camera/scale, argument errors, ErrorCube lookup,
clearing, visibility/alpha changes, and subsequent restoration. The real-data
lifecycle diagnostic uses an isolated archive overlay with ordinary Lua calls
and pointer actions. Captures show the complete Human scene, an empty model
viewport with the surrounding UI preserved, and correct repeated restoration.
Five clear/restore/repeat steps reach readiness in 2.25-3.05 ms; the repeated
Human instances use one prepared GPU source. This short capture run establishes
lifecycle behavior, not steady-state throughput.

There are explicit remaining limits. The ModelFFX assignment override at
`0x004E5ED0` appears to call `0x00824060` with a null instance when clearing;
that callee dereferences the instance. Solarity deliberately handles that case
safely instead of reproducing the apparent crash. ErrorCube lookup is tested
at the asset/script boundary; at this revision, models without authored cameras
still required the stock default-camera rendering path (`0x0095FC30` -> `0x004BEE60`).
At that revision, `SetSequenceTime` selected the requested sequence only in
published state; the renderer offset is implemented below. Shared-cache
basename collision behavior is also not reproduced by full-path resource keys.
These results do not establish complete Model/ModelFFX parity.

Formatting, Clippy, all 556 workspace tests, and the original real-data
interaction validator pass. The original 31-action replay also passes with
6,000 following frames per action, no captures, and detailed UI timing
disabled. Following throughput is 2,095-2,949 FPS; ordinary customization
transition maxima are 2.5-3.6 ms. Creation entry reaches 16.0 ms and cold Blood
Elf selection 22.8 ms. Long frames remain: creation's following maximum is
35.2 ms, including 10.7 ms in platform events and 24.4 ms in application
presentation within the inner measured scope (3.23 million thread cycles).
Login startup has an 81.3 ms frame, with 80.0 ms in platform events; a later
customization following frame reaches 23.3 ms, also mostly platform events.
The phase placement identifies where elapsed time accumulated, not the
underlying cause of those pauses. Fresh instance behavior changes playback
and resource history, so these numbers are validation of the resulting
behavior, not a controlled end-to-end speedup claim.

## Default Model camera projection

The camera-less Model path is orthographic. `0x0095F9F0` compares the requested
unsigned index with the authored camera count and clears the selected camera
when it is unavailable. `0x0095FC30` then calls `0x004BEE60` with the viewport
rectangle and its bottom-left corner. The projection helper `0x006BF4C0` uses
the centered rectangle and signed native depth bounds of -500 and 500; the
view matrix translates the supplied origin relative to the rectangle center.
The render callback at `0x0095FBA0` applies widget yaw and scales the model by
its local scale, effective region scale, normalized screen height, and 5/3.
`GetEffectiveScale` at `0x0049F790` confirms the region field at offset `0x7c`.
Screen normalization is explicit in `0x0047BF90`: height is
`1 / sqrt(aspect * aspect + 1)`, and width is aspect times that height.

The rendering camera now represents perspective and orthographic projections
explicitly, including asymmetric parallel bounds and signed clipping depths.
Its vertical-FOV query returns `None` for a parallel camera. Frustum side planes
use the selected projection, retaining eye-relative arithmetic for large world
coordinates. Model presentation carries root UI extent and effective region
scale into the common model-camera selector. Valid authored indices retain the
existing animated camera path; unavailable indices, including negative values,
select the orthographic path. Errors in valid authored camera tracks remain
errors. Both visible and hidden Glue advancement use this selector.

The default projection absorbs the native coordinate-unit conversion while
leaving model geometry and effects in their existing authored units. Its eye
remains zero, its up axis is Y, and its origin is the viewport's bottom-left.
Camera selection also moves out of mutable model-instance identity: changing
`SetCamera` updates the view without resetting playback or particle state.

Projection tests compare against the recovered native matrix composition for
full and partial viewports, portrait and landscape roots, inherited UI scales,
model transforms, camera-less files, and unavailable camera indices. Separate
visibility tests cover signed near/far depths, asymmetric bounds, spheres,
oriented boxes, and clipped screen windows. UI publication tests verify that
parent scaling changes the effective camera scale and viewport dimensions
without changing the model-local scale.

The eight-step real-archive capture replay passes. It requests a missing model,
explicitly selects camera zero, displays the stock blue/white ErrorCube at the
default lower-left origin, accepts negative and large camera indices, scales
the owning UI, and restores the authored login scene. The model log records
only three instance activations: initial login, ErrorCube, and the explicit
login-model replacement. Camera changes and the UI scale change do not create
new model instances. Camera-only readiness is 2.0-2.6 ms in this short capture
diagnostic; the UI scaling and restoration actions take 33.0 and 25.3 ms.
Those actions include whole-screen UI mutation and publication; the replay
does not isolate the cause of their longer intervals. The diagnostic is not a
steady-state performance measurement.

This implements the default projection and camera-index switching, not the
entire Model camera API. Explicit camera-object ownership across model
replacement and `SetPosition` still need separate stock parity work. Renderer
playback seeking is implemented in the following slice.

Formatting, Clippy, all 558 workspace tests, and the original real-data
interaction validator pass. The separate 31-action replay with 6,000 following
frames per action, no captures, and detailed UI timing disabled also passes.
Following throughput is 2,512-3,603 FPS; its largest following interval is
2.96 ms. Ordinary customization transition maxima are 2.49-3.15 ms, creation
entry reaches 14.3 ms, and login startup reaches 20.2 ms. This run contains none
of the previously observed long following-frame pauses, but does not explain
their cause or prove their elimination. The frustum change moves side-plane
construction out of individual visibility tests; the replay alone does not
isolate its contribution to the throughput difference.

## Ordered Model sequence playback

The native Model methods restart a timer on every accepted sequence call,
including repeated IDs and offsets. A presentation snapshot could collapse
several calls into one and incorrectly reset the whole GPU instance when the
sequence changed. Glue now consumes typed commands addressed to the mutable
model generation, retaining calls in order across asynchronous loading.
Hidden widgets keep lightweight CPU playback, and activation transfers that
playback into the existing visible compositor. Scale and yaw changes update
the placement transform without replacing the model or its live effects.

The Model-specific resolver follows native AnimationData fallback modes and
selects authored variation ordinals with raw unsigned 32-bit frequencies.
The timer applies signed seek offsets, reverse and held modes, integer tick
wrapping, and native boundary placement. Automatic variation updates preserve
overdue frame time. Event intervals preserve each crossed occurrence in scene
order rather than replaying a prefix after a seek. Source addresses, conversion
rules, and remaining timer/event limitations are recorded in
[`m2-animation.md`](m2-animation.md).

Deterministic tests cover repeated calls and their CRT consumption, queued
instance ownership, numeric conversion, fallback chains, authored variation
heads, incomplete and wide weights, seeks, reverse and held sampling, wrapping
ticks, multiple crossed boundaries, and terminal event deadlines. The original
real-archive Glue interaction validator passes. Formatting, Clippy, and all
565 workspace tests pass.

A thirteen-step renderer diagnostic uses a private archive overlay with a
linear translation added to the stock ErrorCube. Captures verify middle/end
seeks, repeated end seeks, the root-bone clear no-op, restart, ordered multiple
seeks, fallback, and fresh-instance reset. The log contains four activations:
initial login, the probe, explicit probe replacement, and restored login.
Sequence requests and scale/yaw changes create no additional activations.
The ordinary sequence actions reach readiness in 2.1-3.2 ms in this short
capture run. These synthetic captures establish behavior, not throughput.

The separate original 31-action replay passes with 6,000 following frames
per action, no captures, and detailed UI timing disabled. Following throughput
ranges from 2,314 to 3,603 FPS; ordinary customization transition maxima are
2.48-3.34 ms, and creation entry reaches 18.0 ms. Following intervals still
reach 29.8 ms. Several pauses accumulate in platform events, while others
include the model/presentation path; phase timing alone does not establish
their cause. The stall goal remains open.

At that revision, secondary-pose blending on automatic variation remained open;
the following slice implements it. Individual event-position sampling and sound age, hidden model
clock/effect ownership, external-payload completion behavior, and scene-global
track ownership remain. FrameXML Model rendering and the older world playback
path also need their own integration. The transition-card and in-world work
remain part of the active goal.

## Automatic Model variation blending

Automatic variation changes now retain the outgoing timer and blend its pose
into the incoming sequence. The new sequence supplies the fade duration,
and the fade starts at the scene's current tick even after a delayed callback.
An interrupted blend retains its existing previous pose while that pose's
weight exceeds one half. Repeated explicit Lua sequence calls still restart
directly, following the separate native request path.

The shared clock carries both sequence samples into bone, material, light,
ribbon, particle, and camera consumers. Continuous tracks blend before bone
hierarchy and material composition. Step and discrete tracks retain primary
selection; global tracks keep their global time. Rotation between sequence
poses uses the recovered spherical interpolation and its near-collinear
threshold. The previous timer also uses stock's distinct completion flag.

Deterministic tests verify the resulting bone hierarchy, rotation angle,
material opacity, ribbon dimensions and selectors, particle rate and enable
state, global and step behavior, timer wrap, completion flags, blend expiry,
explicit restart, multiple callbacks in one frame, and the exact half-weight
interruption boundary. This supplies blending to the native Model timer path;
world playback, individual callback-position sampling, and hidden effect
ownership remain open.

Formatting, Clippy, and all 571 workspace tests pass. The original 31-action
replay also passes with 6,000 following frames per action. The final paired
run measures 1,956-2,954 FPS with blending, versus 1,569-2,838 FPS for the
previous commit built and run on the same machine. Median frame changes range
from -16.5% to +9.8% across actions; these variable runs do not isolate a
blending cost or establish a performance improvement. Ordinary customization
transition maxima are 2.56-4.15 ms in the current run. Its following intervals
still reach 58.9 ms, and the previous build reaches 88.0 ms. Cold publication
and intermittent frame pauses remain unresolved; average throughput alone
does not satisfy the stall goal.
