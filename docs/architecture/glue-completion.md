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
