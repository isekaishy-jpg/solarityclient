# Login-to-world completion scope

The current slice covers login, character selection and creation, and the
initial loading transition into a ready world. Stock-correct effects, lighting,
animation, randomization, failure behavior, and code/architecture are part of
completion. The working performance target is approximately 1,200 completed
frames per second (0.833 ms), with particular attention to visible stalls while
switching characters or changing customization. A fast idle average does not
establish transition performance.

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
| Character and customization transitions | Phase timings for cold and warm changes; complete scene publication; no ordinary interaction causing recurrent long frames |
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
