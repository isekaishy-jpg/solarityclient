# Creature body scale

Build 12340 stores an authored body multiplier at `Unit_C + 0xB3C`.
The world creature renderer previously applied only `OBJECT_FIELD_SCALE_X`,
which made models with non-unit authored scales appear at the wrong size.

`resolve_unit_body_scale` reproduces these original executable paths:

- `0x722AE0` joins the active CreatureDisplayInfo and CreatureModelData rows.
  Either missing row returns one without consulting the creature family.
- `0x71C050` follows CreatureDisplayInfoExtra to ChrRaces and its male/female
  default display. That display's scale multiplies the NPC's authored scale.
  Missing providers return one; other gender values look up display zero.
- `0x71C110` multiplies race-display, active-display, and model scales, replacing
  a non-positive product with one. A bound CreatureFamily supplies an
  interpolated scale. Ordinary creatures take the larger scale; a nonzero
  `UNIT_FIELD_PETNUMBER` selects the family scale even when smaller.
- The family calculation uses signed, ordered clamps and a zero fraction for
  equal endpoints. Reversed endpoints are preserved. This differs from the
  separate character-selection pet-preview calculation.
- `0x73FCC0` stores the body multiplier when installing a model. `0x71C0E0`
  combines it with the independent object-instance multipliers for rendering.

The systems implementation reads absolute update words 67 (active display),
54 (level), and 75 (pet number). Native unit-relative offsets omit the six
common object words. Its calculation retains wider intermediates until the
body float store. The runtime supplies the family from the template bound to
the exact world/entity/GUID lifetime, then combines the result with the
server's object scale. That server field retains its original meaning.
Late template replies, level changes, and pet-number changes therefore update
creature residency and the model matrix.

`tools/ghidra/unit_body_scale_oracle.py` executes the three scale functions
without hooks against the SHA-256-pinned 12340 executable. Its 329 captured
cases include missing providers, male/female/other genders, ordinary creatures,
numbered pets, non-positive authored products, equal/reversed family intervals,
signed levels, and deterministic fractional inputs. The portable regression
loads equivalent real WDBC catalogs and compares the final float bits through
the ECS API. Runtime tests also exercise seven successive scale states through
Vulkan creature placement and encrypted template replies across unit lifetimes.

The authored multiplier also applies to local and remote player bodies.
The player constructor (`0x6E6B40`, vtable `0xA326C8`) and unit constructor
(`0x73F660`, vtable `0xA34D90`) both select `0x71C0E0` at vtable offset `0x7C`.
Their ordinary instance scale is initialized from `OBJECT_FIELD_SCALE_X` by
`0x745E60`; the separate `+0x9C` multiplier starts at one.

Mounted players use the rider body's multiplier. `0x73D5D0` joins the mount
display/model rows, then copies only the display's `+0x10` scale to unit
`+0x990`. The mount model-data scale does not enter the render product.
`0x71C0E0` retains `body * instance * mount-display` until its final float
store. Mount loading also stores `1 / mount-display` on the rider's local
matrix, so composing attachment zero does not resize the rider. The runtime
retains that reciprocal through GPU residency and applies it after the
animated mount attachment, before resolving the rider's own attachments.

`tools/ghidra/unit_mount_scale_oracle.py` runs the original DBC lookup/store
prefix (`0x73D5D0..0x73D666`), reciprocal block (`0x73D7DA..0x73D7E9`),
and complete model/scale getters (`0x6E6F80`, `0x71C0E0`) without instruction
hooks. Sixteen captured records cover distinct display/model scales, object
scales, mounted and unmounted model selection. Runtime regression tests feed
these records through real catalogs and local/remote player residency, then
check the resulting mount and rider matrices across mounts and dismounts.

The native `+0x9C` auxiliary scale and `0x72CBB0` object-scale compensation
during asynchronous model replacement retain separate, unimplemented
lifecycles. Mounted NPC residency and mounted scene-registration/terrain-tilt
routing also remain separate work.
