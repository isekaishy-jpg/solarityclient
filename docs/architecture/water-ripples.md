# Water completion slice

Water completion includes camera behavior at the surface, underwater effects,
surface ripples and splashes, and water lighting. Lighting and sky work is the
last stage: the incomplete general lighting/sky renderer may require a broader
implementation. Existing swimming and submerged-light code is a starting point,
not evidence that these presentation requirements are complete.

The remaining integration and investigation order is:

1. Validate the integrated ripple/splash emitter, GPU pass and creature-template
   suppression rule together with their existing movement and cache consumers.
2. Audit the complete camera path, including water collision settings, pivot
   and eye queries, terrain versus WMO selection, transitions, and obstruction
   ordering. Compare existing camera code against the pinned executable.
3. Audit underwater presentation separately from swimming: camera-driven
   transitions, authored effects, sound, and surface draw ordering. Establish
   which effects the original client actually uses before adding any.
4. Complete water surface/underwater lighting and its general lighting and sky
   dependencies. Preserve the existing empty-band crash regression throughout.

## Native ripple boundaries

Evidence uses the pinned build-12340 Wow.exe with SHA-256
`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`.

`0x0071CBA0` evaluates unit notification zero periodically and notification
`0xC9` on immersion crossings. Registration and resolved liquid flag 1 admit
emission before the shared Blizzard random stream is consumed. Radius,
lifetime, growth, depth attenuation, directional yaw, and the next wrapping
millisecond deadline follow the original arithmetic and draw order.

Runtime consumes the existing local and remote movement liquid samples. Each
immersion notification precedes that unit's periodic notification, using the
shared Blizzard random stream. World replacement clears scene effects; unit
departure retires its clock while emitted geometry retains its native lifetime.
The two archive textures load during service construction; projected frame
banks and fenced GPU streams reuse their storage.

`0x007370D0` selects the mount model when present (`0x006E6F80`), otherwise
the unit body. `0x008254F0` starts from the M2 header render box and recursively
expands it by child extents. Each candidate expands the original parent box;
siblings are unioned, not accumulated. Attachment transforms and collision
height do not replace these bounds. `0x00780240` transforms positive-volume
boxes with `0x00984860`; degenerate or unavailable boxes use unit translation.
`0x007A1BC0` admits the surface only when it is at or below the registered top.
Runtime applies this gate before querying area policy or consuming randomness.

The additional registration flag `0x2000` comes from creature-template flags
bit 22 (`0x00715D90`, cached record at unit + `0x964`, populated through
`0x0067B6A0`/`0x0072CDE0`). It is not a `CreatureModelData` flag. Runtime now
queries the creature cache by entry/full GUID (`0x60`) and handles complete
or high-bit missing-entry replies (`0x61`, `0x0067B840`, `0x0098D4C0`).
The decoder retains six bounded byte strings, ten full-width words, two floats,
a normalized leader byte, six quest-item IDs and the final movement ID.
Absent native cache records leave the suppression flag clear.

Creature and GameObject caches share the same entry coalescing and weak callback
owner. Unit callbacks register after object-update dispatch and retire by exact
world/entity lifetime; steady frames only drain pending writer requests. Missing
replies finish existing callbacks empty, while a later new admission may retry.
Map replacement retains completed definitions and retires old unit callbacks.
Connection retirement clears the cache and queued requests. The suppression
flag gates new ripple emissions before randomness without removing old ripples.

Area substitution is shared by movement sounds and ripple admission through
`AreaTableCatalog::liquid_flags`. `0x009905C0` applies only to liquid IDs 1..20,
uses `(id - 1) & 3`, and checks exactly one parent when the area's override is
zero. Missing area rows do not initiate inheritance. A nonzero replacement
whose liquid row is absent remains absent.

`0x0079D460` normalizes strength before `0x0079D180` and `0x0079CF40`
initialize the retained record. Local-player emissions cycle through 32 slots;
other units share 96 slots. `0x006DED60` moves a reused slot to the active-list
tail even when another slot is vacant. Geometry is collected once from the
water-only terrain and WMO query over the final growth bounds.

`0x0079D5E0` advances radius and the rise/fall envelope once per scene frame.
Expiry or nonpositive alpha retires the record, including a zero-duration
first frame. The rise-to-fall crossing retains the original consumed-time
calculation. Circular and directional texture passes preserve active order
within each pass.

`0x007E2D60` derives projection from rounded world bounds, then multiplies
translation, reciprocal width, the authored -pi/2 axis rotation, and the
stored negated yaw. Replacing the residual cosine with an exact axis swap
changes native results. `0x004C21B0` projects each retained world vertex on the
CPU. `0x0079DDBC` stores opacity times 255 as a float before nearest-even FISTP.
`WaterRippleRenderVertex::project_into` appends those packed 24-byte PCT
vertices into caller-owned storage that can be reused across frames.

The world path `0x004F8EA0` places `0x0077F020` between the camera-dependent M2
transparent passes. Its target `0x00790A80` runs the transparent water queue
before `0x0079D5E0`. Ripple drawing disables lighting, fog, depth writes, and
culling and uses additive source-alpha blending (GX blend 3: source alpha,
destination one). GX state 7 takes alpha reference 1 from `0x00AD8B7C`;
the D3D9/Ex initializers at `0x006A3AA0`/`0x006A7A40` use greater-or-equal.
`footstepBias`, registered at
`0x0078E400` with default 0.125, is multiplied by the exact float at `0x00A3FAC8`
(`0x3A800080`). D3D state submission at `0x006A4C6F`/`0x006A8C0F` negates that
value before setting state 195 (depth bias). Vulkan integration must preserve
the resulting depth offset, rather than moving the world geometry upward.

`0x006A4900` maps texture flag `0x201` through row 1 of `0x00AD8F40`:
min/mag linear, no mip sampling, clamp U/V, anisotropy one. The Ex backend
uses the identical row in `0x00AD8FE0`. `0x004B9760` receives override flag 1,
so global texture filtering does not replace the ripple's authored sampler.

The dedicated Vulkan pass borrows two projected PCT vertex banks, streams
them through reusable world-frame slots, and records directly after the
transparent water queue. Its draw count preserves the original low-16-bit
vertex-count wrap. Buffer growth and descriptor updates occur only after
the corresponding slot fence retires; there are no per-ripple mesh uploads.
The fragment shader applies the direct normalized depth offset, preserving
the [D3D9 depth-bias contract](https://learn.microsoft.com/en-us/windows/win32/direct3d9/depth-bias)
across Vulkan depth formats. This is the shader-side representation described
by [Khronos](https://docs.vulkan.org/features/latest/features/proposals/VK_EXT_depth_bias_control.html);
it avoids adding a required device extension, with the documented cost to
early depth-test optimization on this small effect pass.

## Reproducible comparisons

The scripts under `tools/ghidra/` map only the fingerprinted PE into Unicorn;
they do not launch the game or run an OS entry point.

| Oracle | Portable fixture coverage |
| --- | --- |
| `water_ripple_emission_oracle.py` | 1,033 emission, random-stream, and deadline cases |
| `water_ripple_lifecycle_oracle.py` | 252 lifetimes, 546 frame decisions, 270 slot insertions |
| `water_ripple_projection_oracle.py` | 262 matrices and 7,860 packed vertex/alpha cases |
| `unit_render_bounds_oracle.py` | 256 recursive five-model render boxes |

The lifecycle oracle substitutes allocation and triangle collection, with
active-list insertion substituted only in the scalar lifetime capture. The
pool capture executes original list insertion. Projection skips only the
unused camera query in absolute-world mode; matrix functions, trigonometry,
float stores, UV evaluation, and alpha conversion run original instructions.

The offscreen Vulkan regression exercises additive blending, depth-test and
depth-write state, normalized depth bias, no-mip sampling under minification,
descriptor replacement, growing frame streams, and native vertex-count wrap.
The camera audit, underwater effects audit, and final lighting/sky stage are
required before declaring water complete.
