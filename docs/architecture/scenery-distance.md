# Static scenery distance fading

Build 12340's `CMapObj` scene submission at `00791CB0` fades and rejects whole
M2 placements. It does not select another SKIN companion. Terrain-owned MDDF
and static WMO MODD placements now use this policy before mesh/effect packet
assembly. Replicated units, GameObjects, attached equipment, Glue models, and
replicated WMO attachments retain their separate visibility paths.

`007BDB10` reads the M2 render box through `004F5E20`, transforms its bounds
with `007F9430`, and classifies the largest world-space box dimension. Equality
belongs to the smaller class. The distance center comes from `004F5E80`'s
render-box midpoint transformed by the placement matrix; collision bounds and
the placement origin are not substitutes.

| Maximum dimension | Far distance at detail 1 | Fade width |
| --- | ---: | ---: |
| <= 1 | 30 | 5 |
| <= 4 | 100 | 10 |
| <= 15 | 200 | 15 |
| <= 100 | 750 | 20 |
| > 100 | 1250 | 50 |

All dimensions and distances are world units. `0078F570` multiplies only the
middle three far distances by `environmentDetail`. Fade widths remain fixed.
The CVar registration at `0078E80F` defaults to 1.0; its `0078DC60` callback
clamps to [0.5, 1.5]. Live and offline world presentation read the existing
FrameXML CVar. `00780F5B` enables the distance-policy bit 0x4000 in the initial
world flags. Developer toggles and entity flag 0x800 bypasses are not exposed by
these static runtime owners.

`00791CB0` compares squared camera-to-center distance against the far radius,
then linearly fades across the final interval. Alpha above 0.99 becomes 1;
alpha at or below 0.01 is rejected. Visible fractions enter the existing
runtime-alpha material pipeline, including opaque/cutout material promotion
and transparent scene sorting. Static bounds/classes are retained across frames;
changing the detail setting does not reload models. This does not reduce ADT
residency or the cost of publishing a new tile's membership.

`tools/ghidra/scenery_distance_oracle.py` executes the pinned executable's
classification, affine bounds, threshold configuration, and admission. Only
the final model-activation side effect is stubbed. Its 486 checked-in cases
cover category boundaries, rotated/translated bounds, all three detail settings,
fade endpoints and opacity snaps. The runtime regression compares resulting
opacity, including exact visible/hidden snaps. No test requires a local client.

The broader world renderer still needs independent work on distant terrain,
ground-effect doodads, indoor fog-bank routing, shadows, and streaming stalls.
