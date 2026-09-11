# Static scenery distance fading

Build 12340's `CMapObj` scene submission at `00791CB0` fades and rejects whole
M2 placements. It does not select another SKIN companion. Terrain-owned MDDF
and WMO MODD placements now use this policy before mesh/effect packet assembly.
Replicated WMO attachments use their current parent transform through the
[WMO doodad admission path](world-model-doodad-visibility.md). Replicated units,
ordinary GameObjects, attached equipment and Glue models retain their separate
visibility paths.

`007BDB10` reads the M2 render box through `004F5E20`, transforms its bounds
with `007F9430`, and classifies the largest world-space box dimension. Equality
belongs to the smaller class. The distance center comes from `004F5E80`'s
render-box midpoint transformed by the placement matrix; collision bounds and
the placement origin are not substitutes.

An entirely reversed render box is the native empty-box sentinel. `007BDB10`
replaces it with a point at the placement origin before classifying size.
Partially reversed boxes still pass through the axis-product transform; each
axis contribution is stored as float after x87 arithmetic. The runtime uses
these same rules for scenery distance and shadow registration. Orgrimmar's
`KL_AUCTIONHOUSECOLLIDE.M2` uses minimum `FLT_MAX`, maximum `-FLT_MAX`, and radius
zero. Treating those bounds as an enormous box previously let its shadow query
reach invalid unit-style registration during the elevated travel replay.

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
Another 294 direct `007BDB10` executions cover every reversed-axis combination,
the actual empty-box sentinel, signed/scaled transforms, and exact class results.
The offscreen scenery regression also exercises the sentinel through real Vulkan
packet preparation.

The broader world renderer still needs independent work on far WMO placements,
other entity fog consumers, shadows and streaming costs.
