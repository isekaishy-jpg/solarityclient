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
