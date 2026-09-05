# Resident terrain streaming

The runtime admits neighboring ADTs around the resolved world camera and keeps
their terrain, M2, WMO, collision, and liquid inputs together. Crossing into an
already resident tile promotes it without loading its assets again or replacing
the world renderer's animation/effect owners.

## Native window and request order

Evidence refers to the fingerprinted build-12340 executable in
[`tools/ghidra/README.md`](../../tools/ghidra/README.md).

`0x00780860`, beginning at `0x00780A14` after camera resolution, derives the
loading radius from the eighth world-space frustum corner's distance from zero.
It uses 1.25 times the effective far distance when that corner is closer than
the far distance; otherwise it caps the corner distance at twice the far
distance. This is not a fixed neighborhood size or distance from the camera.

The native negative grid multiplier, stored rounding boundaries, truncation,
and half-cell conversion produce the center and radius in global MCNK space.
The inner range aligns to pairs of chunks. The outer range adds two chunks,
applies the native minimum-width expansion, and clamps to the 1024-by-1024
world domain. `TerrainStreamingWindow` retains both ranges and enumerates ADT
addresses using the native world-axis/ADT-axis transpose.

`0x004FAADF` supplies the followed object's position in ordinary follow mode,
or the camera eye when no object is followed. The runtime shares its final
obstruction-resolved camera between streaming demand and presentation. Its
existing Vulkan camera matrices supply the corresponding far corner; those
matrix operations are not claimed to reproduce native x87 inversion bit for bit.

`0x007B5950` visits existing ADT registrations before constructing missing WDT
entries. `0x007D9A8A` appends new references through `0x006DED60`; registration
order survives load completion and primary-tile changes. The runtime retains
these registrations separately from completed CPU generations. Distance keys
come from the native ADT box at `0x007D9A70` and closest-XY query at `0x007B4830`.
The CRT quicksort at `0x0040BE50`, including its eight-element shortsort, defines
equal-distance ordering. A stable language-library sort would change that order.

## CPU and renderer ownership

One terrain worker owns its mounted archive stack and decoded texture/model
caches. Initial entry, prewarming, and neighboring loads share this worker and
the bounded CPU executor. A declared tile remains unavailable until its full
ADT and static dependencies have prepared successfully. Map retirement marks
pending work ineligible while retaining its task for a later join; returning
to an equal map/tile key cannot publish that older generation.

The camera window retires distant neighboring CPU generations. Camera rays,
terrain support, and liquid queries now consider the retained neighborhood.
Selected MDDF/MODF records use the same unique-ID consistency checks within an
ADT and across overlapping ADTs. Unreferenced records create no live owner.

The renderer shares each immutable terrain plan for culling and uploads added
tiles incrementally. MDDF IDs and `(MODF ID, MODD index)` pairs identify M2
placements; MODF IDs identify WMO placements. Additional references retain the
existing transform, playback, particles, ribbons, and random-stream position.
An owner departs when its last resident ADT reference leaves. Dynamic player,
creature, equipment, and transport placements keep their separate ownership.
New static M2 local sequences start against the existing scene clock rather
than aging from world entry time zero (`0x00826B00`).

Terrain buffers, material atlases, and terrain descriptor handles invalidate
immediately on retirement. An empty graphics submission fences their earlier
uses across all in-flight frames. Presentation polls that fence before freeing
the allocations, without waiting for device idle. Registry slots are never
reused, so uploading an earlier plan cannot revive a stale handle. Mixed-atlas
descriptor pools remain allocated until their final set retires. The fence
scope follows the Vulkan specification's
[queue-submission fence ordering](https://docs.vulkan.org/spec/latest/chapters/synchronization.html#synchronization-fences-signaling).

## Validation and remaining integration

`terrain-streaming-native.txt` records 1,096 executions of the original window
instructions, including grid boundaries and clamping. `terrain-priority-native.txt`
records 261 original box/distance/CRT-sort executions, including equal keys and
both sides of the shortsort threshold. Portable tests compare the resulting
integer windows and complete tile order exactly.

Runtime archive fixtures exercise asynchronous neighboring loads, collision in
the added tile, promotion without worker availability, window retirement, stale
jobs across disconnect/reentry, shared placement IDs, and conflicting selected
records. A playback regression admits a model one minute into a scene and
checks its initial sequence time, sound-event window, and random consumption.

Local Vulkan smoke checks also used the mounted Northrend and Stormwind data.
The Northrend pair submitted 512 terrain draws and retained all 822 existing M2
owners while increasing to 1,430 unique M2 placements. The Stormwind pair
retained 6,210 existing M2 owners while increasing to 6,473 M2 and five WMO
placements. Both checked promotion, animation/effect-state retention, immediate
handle invalidation, queued resource retirement, and reupload with fresh handles.
The combined Stormwind submission also exercised 2,875 WMO draws, 7,553 M2
draws, and live particle geometry. Its orthographic capture used diagnostic
lighting rather than the normal world environment.
These local checks supplement the portable fixtures; they are not frame-rate
or complete world-behavior certification.

Native loading admits individual chunks and has staged inner-window readiness
for terrain, WMOs, and M2s. The runtime currently admits a whole ADT generation
and still completes the transition card from the primary scene. Static-model
GPU buffers/descriptors and shared diffuse images still use their existing
renderer cache lifetime. Incremental asynchronous GPU transfers, native loading
readiness, dynamic movement references/resource IDs, and local ground/fall/input
ownership remain separate work. Static movement now joins the resident
neighborhood while retaining WMO registrations and MCRF/MODR references. See
[`movement-world-geometry.md`](movement-world-geometry.md) for the movement
collector boundary and its remaining integration.
