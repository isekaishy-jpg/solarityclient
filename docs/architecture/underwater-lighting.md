# Underwater environment lighting

The pinned build-12340 executable selects underwater light in `0x007F3230`.
`0x00780620` returns the resolved camera liquid and signed camera-to-surface
depth. This is independent of the mover's swimming flag: an orbiting camera
can enter water while the player remains on shore, or remain above a swimmer.

Without a nonzero `LiquidType.LightID`, global selection and local volume
composition use zero-based Light.dbc parameter bank **one**. Exterior lighting
uses bank zero. `0x007EE510` applies the same selection to local volumes.
Before bank selection, `0x007F2790` calls `0x007ECB30` to build the map's
light list. The last zero-position row for the map occupies its global slot;
when absent, the slot uses **Light.dbc ID 1**, irrespective of that row's map.
Local rows still belong to the active map. This fallback is required by RFC
(map 389): treating its absent map-global row as fatal terminated world entry.
It applies to both exterior and underwater banks. The empty-list constants in
`0x007F3230` are a different branch: normal map-list construction reserves the
global slot even when the map has no authored rows.

`0x004F8501` obtains the light-volume position from the camera's followed
object, so ordinary player-follow cameras retain player-position weights.
Camera submersion alone selects the underwater bank and depth.

A nonzero `LiquidType.LightID` is a **LightParams.dbc** identifier. The lookup
at `0x007F32EA` bypasses world-volume composition and samples that parameter
directly through `0x007ECD80`/`0x007EBFF0`. `LightCatalog::sample_parameter`
joins the same color, scalar, skybox, and material channels used by ordinary
volume sampling. It does not construct a synthetic Light.dbc row.

Zero-key bands are valid sampled inputs. `0x007EB070` returns packed opaque
black for an empty color band; `0x007EAEF0` returns zero for an empty scalar
band. Neither samples the padded value storage. Underwater parameter 213's
specular band 3826 exercises this path: rejecting empty bands terminated the
client after camera submersion. Missing rows remain distinct from empty rows.
`tools/ghidra/light_empty_band_oracle.py` executes both original samplers for
all channel ordinals and four times with nonzero padding. The portable test
covers exterior/underwater/exterior transitions and direct parameter/model
sampling. `validate_underwater_light` checks every authored underwater bank
and liquid override across the installed tables and a complete day.

For positive MaxDarkenDepth, `0x007F364F` clamps camera depth between the
surface and that maximum, then applies the authored fog, ambient, and direct
factors independently. `0x007ED790` converts each packed color to HSV, scales
its value, converts back to RGB, and packs through `0x009851A0`. Float stores
and nearest-even integer rounding matter at 8-bit boundaries. Directly scaling
RGB is observably different. Sky, water palette, fog range, and other sampled
channels retain the selected bank's values.

`RuntimeWorldEnvironment::resolve_liquid` runs after camera collision and the
terrain/WMO submerged query. Normal presentation and benchmark replay both
pass its completed frame to terrain, WMO, M2, sky, fog, and water rendering.
The exterior snapshot remains available for camera construction; depth never
accumulates across frames, and an unsubmerged camera uses the exterior snapshot.
The same admitted liquid continues to drive zone sound and transparent-pass
ordering.

`tools/ghidra/liquid_light_depth_oracle.py` executes original `0x007F3230`
with a controlled composed color at the `0x007EE750` boundary and stops after
the three depth results. The liquid query, table lookup, clamp, factors, and
RGB/HSV conversion all execute original instructions. The checked-in fixture
contains 3,584 cases. Tests load real WDBC tables and compare every resulting
fog, ambient, and direct color exactly. Runtime integration separately covers
global/local banks, negative and maximum depth, direct parameter overrides,
missing rows, and restoration of the exterior frame.
