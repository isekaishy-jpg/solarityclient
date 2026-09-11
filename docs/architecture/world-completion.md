# World rendering and core UI completion scope

This is the active slice tracker, reconciled with the user's scope decisions on
2026-09-11. It tracks user reports and gaps recovered from the pinned build-12340
client together. The current product version is 0.0.3a; this document does not
authorize a version change or declare the slice complete.

## Priorities and completion rules

The user's explicit implementation order on 2026-09-11 is:

1. Finish NPC equipment.
2. Correct UI scale.
3. Complete world shaders and lighting end to end, addressing the substantial
   omissions visible in the screenshot comparison.
4. Complete remaining world rendering items.
5. Complete core UI.

Implementation remains the priority throughout the slice. UI scale is the
explicit early UI task; the full core UI stage follows world rendering.

The user's 2026-09-11 Durotar screenshot comparison reasserts this gate. Visible
terrain detail and lighting/contrast still differ substantially. The user confirms
both captures use the same position and maximum zoom, and subsequently notes
that the saved camera position may differ slightly. The current Soap character
files both store camera distance 15, with stock pitch 9.849859 degrees and
Solarity pitch 12.907174 degrees. Match these inputs before diagnosing a
camera/projection/zoom-policy defect; world scaling is unproven from these images.
The comparison also shows missing or different entity equipment/attachments and
different world UI scale. Preserve each as a separate investigation and visual
validation item. The immediate work includes the terrain material/sampling
and world-lighting paths exercised by that scene. Further mount/vehicle animation
expansion is deferred; its isolated correctness tests do not establish world
rendering completion. Preserve the existing vehicle work while returning
implementation priority to these visible world differences.

The same report identifies loss of functionality under 100% CPU load and
intermittent screenshot failure. Treat these as reliability defects alongside
the visual work. Queue saturation must defer work without losing requests or
ownership; validate recovery after capacity returns. The missing vehicle UI
prevents user testing of that functionality, regardless of isolated native or
scene-test results. No vehicle completion claim is justified by those tests.

Discovering costs across the client is in scope, including CPU and GPU rendering,
streaming, scene preparation, UI, and other substantial work encountered during
implementation. Identified critical costs should be fixed. General optimization
must not indefinitely displace implementation; 1,200 FPS remains aspirational,
not a completion gate for this slice.

Stock behavior and exact stock data govern implementation. A reported difference
is an investigation item until its cause is established. Suspected missing
shaders must be checked against the native selection and rendering paths before
being described as confirmed omissions.

Keep implementation and validation status separate. For each investigated item,
record the native evidence, implemented consumers, outstanding integration,
automated/visual checks, and dependencies that prevent testing. A working helper
or a panel that merely opens does not establish end-to-end completion. Preserve
the user's closure of the recent reported bug-fix batch; new scope does not
automatically reopen those fixes.

## World implementation and validation

| Area | Open work and current boundary |
| --- | --- |
| NPC equipment and attachments | Extra-row armor and all three virtual held-item entries reach the shared unit renderer, including attached visuals, bone placement, opacity, environment lighting, disarm/readiness filters and retained component relocation. Effective sheath state follows actual body AnimationData flags, ranged latching, posture changes and server template restrictions. Native state/selection cases, live residency transitions and a controlled combined Durotar capture pass. The exact reported scene remains part of the broader world comparison. See [NPC equipment](npc-equipment.md). Do not infer a different NPC identity from missing geometry. |
| World UI scale | Native initial UIParent scaling and live useUiScale/uiScale callbacks are implemented, including saved settings, scaled screen queries, callback ordering and normal layout/input propagation. The default 1440p scene now uses 0.9. Original-code policy cases, FrameXML geometry/input tests, installed-archive lifecycle validation and combined Durotar captures pass. General window resizing and graphics-settings controls retain their separate owners. See [world UI scale](world-ui-scale.md). |
| Item, NPC/creature, and other entity fading | Native entry interpolation (including vehicle seat eligibility), ordinary detached disappearance, camera-subject fading and ordinary Player_C visibility are connected, including attached equipment. Vehicle/special camera modes, exceptional owner/visibility policies, publication timing and combined travel validation remain open; see [entity opacity](entity-opacity.md). Static MDDF/MODD scenery fading has a separate policy. Keep this user-reported gap open. |
| World and terrain lighting | Complete the remaining lighting consumers and shader/material variants, including terrain point-light/specular paths and specialized entity callbacks. The user reports continuing differences from stock; compare the combined presentation at matching camera, time, and settings. |
| Environment shaders | Audit native material selection and the environment shader families exercised by actual world assets. The user suspects some are entirely missing; identify confirmed omissions and distinguish them from incorrect inputs or unconnected consumers. |
| Buildings and portals | Surface, liquid, attached-doodad and sky admission/fog consumers are integrated with native and controlled GPU coverage. Validate their combined live presentation while entering, leaving and moving through buildings. |
| Shadows | Primary unit shadows and terrain/ground-detail receivers are implemented. Remaining work includes higher-quality environment maps, static casters, cascade transitions, liquid receivers, the quality-zero projected entity-shadow path, and exceptional registration behavior. |
| Weather and sky | Complete precipitation presentation and retirement, weather ambience, remaining small/POT screen-effect allocation policies, and sound consumers. Ghost composition, ordinary glow, player blur, underwater distortion, invisibility and the procedural Special owner have native producer and GPU comparisons; real-archive replays verify dry/wet transitions. WMO sky visibility and replacement, global screen-effect lighting/skybox selection, callback-driven manual fog, weather palette/cloud-light transitions and authored skyboxes have implemented paths. Combined world appearance remains open. |
| Particles, spells, and aura visuals | Complete the remaining specialized particle paths and general spell/aura visual consumers and lifecycles. Validate attachment, ordering, visibility, and retirement together. Track checks blocked by missing spell or other gameplay implementation explicitly. |
| Distant world and occlusion | WDL terrain, static scenery fading, and terrain ground detail are implemented. Far WMO placements and native terrain-horizon/sphere-occluder rejection remain. Verify actual distant presentation and travel transitions. |
| Travel and streaming | Unit/vehicle base passenger frames, final local/remote projections, settled animated seats and local/remote server-path boarding/exit transitions are connected, with nested-parent, mounted-rider, execution-order and lifetime tests. Local input observes passenger delay/travel phases and active path completion uses the native acknowledgment envelope. Unloaded passengers initialize travel from resident parent bones and retain the timer on model arrival. Seated body/upper animation slots share native callback ordering and retain independent completion bits. Vehicle active-mover selection, transfer/destruction callbacks, special cameras and combined live travel remain open. Validate admission, disappearance, lighting/fog transitions, and complete scene publication while moving through the world. Trace publication and first-use costs; fix critical costs discovered. Stationary offline timing does not establish populated-world behavior. |

Existing evidence and implementation boundaries:
[scenery fading](scenery-distance.md), [terrain lighting](terrain-lighting.md),
[scene lighting](world-scene-lights.md), [WMO visibility](world-model-batch-visibility.md),
[attached WMO doodads](world-model-doodad-visibility.md),
[fog](world-fog.md), [shadows](world-shadows.md), [sky](world-sky.md),
[M2 effects](m2-effects.md), [aura state](unit-auras.md),
[water effects](unit-water-effects.md), [distant terrain](terrain-low-detail.md),
[vehicle presentation](vehicle-presentation.md),
[ground detail](terrain-ground-detail.md), and [world costs](world-performance.md).

## Core UI stage

Core UI means the UI normally visible on screen and the menus/panels it opens.
This scope does not declare every underlying gameplay system implemented.

| Area | Required work |
| --- | --- |
| Persistent on-screen UI and its menus | Inventory the stock elements, implement their appearance and behavior, and follow their controls into the menus/panels they open. Verify state updates and interaction, not just initial drawing. |
| Action bars, microbar, quest bar, bags, and chat | Implement each named area, its controls, state updates, and reachable panels. Identify dependencies on later gameplay slices per control. |
| Unit frames, selection, and cast bar | Implement targeting/selection presentation, unit frame updates, and cast presentation. Record combat or spell dependencies without treating inactive controls as complete. |
| Esc menu and every panel reachable from it | Inventory every stock entry and reachable panel. Make their presentation, navigation, controls, and implemented settings/actions behave correctly. Document each missing hookup and its owning dependency rather than silently treating the panel as complete. |
| In-game mouse cursors | Implement the stock custom cursor assets and context-dependent selection, transitions, hotspots, and interaction behavior. Recover the contracts from stock; track contexts blocked by missing gameplay systems. |
| Blocked validation | Keep a per-control or per-context record of the missing implementation, which behavior cannot yet be exercised, what can already be checked, and the condition for resuming the check. Spell-dependent behavior is one known category. |

When the UI inventory is performed, expand these rows into concrete stock
elements and panels with individual statuses. Do not invent the inventory from
generic game UI conventions or mark missing hookups as successful validation.

## Tracking subsequent findings

Add new user reports and native-client discoveries to the relevant area above,
with their evidence and status, as work proceeds. Keep completed behavior
identified beside its remaining consumers so historical architecture documents
do not turn into a second, contradictory backlog. Record confirmed critical
costs with the triggering workload, measured phase, correction, and validation;
retain ordinary cost findings for later focused optimization.
