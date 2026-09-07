# Water completion slice

Water completion includes camera behavior at the surface, underwater effects,
surface ripples and splashes, and water lighting. Lighting and sky work is the
last stage: the incomplete general lighting/sky renderer may require a broader
implementation. Existing swimming and submerged-light code is a starting point,
not evidence that these presentation requirements are complete.

The remaining integration and investigation order is:

1. Connect the native ripple/splash emitter, retained liquid triangles, and
   frame vertex preparation to actual unit notifications and world rendering.
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
culling and uses source-alpha blending. `footstepBias`, registered at
`0x0078E400` with default 0.125, is multiplied by the exact float at `0x00A3FAC8`
(`0x3A800080`). D3D state submission at `0x006A4C6F`/`0x006A8C0F` negates that
value before setting state 195 (depth bias). Vulkan integration must preserve
the resulting depth offset, rather than moving the world geometry upward.

## Reproducible comparisons

The scripts under `tools/ghidra/` map only the fingerprinted PE into Unicorn;
they do not launch the game or run an OS entry point.

| Oracle | Portable fixture coverage |
| --- | --- |
| `water_ripple_emission_oracle.py` | 1,033 emission, random-stream, and deadline cases |
| `water_ripple_lifecycle_oracle.py` | 252 lifetimes, 546 frame decisions, 270 slot insertions |
| `water_ripple_projection_oracle.py` | 262 matrices and 7,860 packed vertex/alpha cases |

The lifecycle oracle substitutes allocation and triangle collection, with
active-list insertion substituted only in the scalar lifetime capture. The
pool capture executes original list insertion. Projection skips only the
unused camera query in absolute-world mode; matrix functions, trigonometry,
float stores, UV evaluation, and alpha conversion run original instructions.

Current tests establish these boundaries and area inheritance. Unit/runtime
wiring, GPU submission, camera audit, underwater effects audit, and the final
lighting/sky stage remain required before declaring the water slice complete.
