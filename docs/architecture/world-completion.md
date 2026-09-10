# World rendering and core UI completion scope

This is the active slice tracker, reconciled with the user's scope decisions on
2026-09-10. It tracks user reports and gaps recovered from the pinned build-12340
client together. The current product version is 0.0.3a; this document does not
authorize a version change or declare the slice complete.

## Priorities and completion rules

World rendering comes first. Terrain, world contents, and their appearance while
moving through the world must be correct before moving on to the core UI stage.
Implementation remains the priority throughout the slice.

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
| Item, NPC/creature, and other entity fading | Native entry interpolation and ordinary detached disappearance are connected for unit/GameObject M2 hierarchies, including attached equipment. Camera/vehicle multipliers, exceptional owner/visibility policies, publication timing and combined travel validation remain open; see [entity opacity](entity-opacity.md). Static MDDF/MODD scenery fading has a separate policy. Keep this user-reported gap open. |
| World and terrain lighting | Complete the remaining lighting consumers and shader/material variants, including terrain point-light/specular paths and specialized entity callbacks. The user reports continuing differences from stock; compare the combined presentation at matching camera, time, and settings. |
| Environment shaders | Audit native material selection and the environment shader families exercised by actual world assets. The user suspects some are entirely missing; identify confirmed omissions and distinguish them from incorrect inputs or unconnected consumers. |
| Buildings and portals | Surface, liquid, attached-doodad and sky admission/fog consumers are integrated with native and controlled GPU coverage. Validate their combined live presentation while entering, leaving and moving through buildings. |
| Shadows | Primary unit shadows and terrain/ground-detail receivers are implemented. Remaining work includes higher-quality environment maps, static casters, cascade transitions, liquid receivers, the quality-zero projected entity-shadow path, and exceptional registration behavior. |
| Weather and sky | Complete precipitation presentation and retirement, weather ambience, remaining small/POT screen-effect allocation policies, and sound consumers. Ghost composition, ordinary glow, player blur, underwater distortion, invisibility and the procedural Special owner have native producer and GPU comparisons; real-archive replays verify dry/wet transitions. WMO sky visibility and replacement, global screen-effect lighting/skybox selection, callback-driven manual fog, weather palette/cloud-light transitions and authored skyboxes have implemented paths. Combined world appearance remains open. |
| Particles, spells, and aura visuals | Complete the remaining specialized particle paths and general spell/aura visual consumers and lifecycles. Validate attachment, ordering, visibility, and retirement together. Track checks blocked by missing spell or other gameplay implementation explicitly. |
| Distant world and occlusion | WDL terrain, static scenery fading, and terrain ground detail are implemented. Far WMO placements and native terrain-horizon/sphere-occluder rejection remain. Verify actual distant presentation and travel transitions. |
| Travel and streaming | Validate admission, disappearance, lighting/fog transitions, and complete scene publication while moving through the world. Trace publication and first-use costs; fix critical costs discovered. Stationary offline timing does not establish populated-world behavior. |

Existing evidence and implementation boundaries:
[scenery fading](scenery-distance.md), [terrain lighting](terrain-lighting.md),
[scene lighting](world-scene-lights.md), [WMO visibility](world-model-batch-visibility.md),
[attached WMO doodads](world-model-doodad-visibility.md),
[fog](world-fog.md), [shadows](world-shadows.md), [sky](world-sky.md),
[M2 effects](m2-effects.md), [aura state](unit-auras.md),
[water effects](unit-water-effects.md), [distant terrain](terrain-low-detail.md),
[ground detail](terrain-ground-detail.md), and [world costs](world-performance.md).

## Core UI stage

Core UI means the UI normally visible on screen and the menus/panels it opens.
This scope does not declare every underlying gameplay system implemented.

| Area | Required work |
| --- | --- |
| Persistent on-screen UI and its menus | Inventory the stock elements, implement their appearance and behavior, and follow their controls into the menus/panels they open. Verify state updates and interaction, not just initial drawing. |
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
