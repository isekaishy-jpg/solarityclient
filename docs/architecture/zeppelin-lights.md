# Authored zeppelin light colors

The installed `TRANSPORT_HORDE_ZEPPELIN` WMO references
`HordeZeppelinAnimation/HordeZepAnimation.mdx`. Its color tracks 0 and 1 both
contain RGB `(185, 134, 248)` with full alpha. SKIN section 12 uses color 0,
`Particles/Gradient64B.blp`, additive blending, and material flags `0x17`
(unlit, unfogged, two sided, no depth writes). Section 13 uses color 1 with
`HordeZeppelinAnimation/Glow.blp` and flags `0x03` (unlit, unfogged).

These purple beam/lens colors are authored model inputs. The same WMO separately
contains yellow maintenance lights. The other `TRANSPORT_ZEPPELIN` model uses a
different animation doodad. No recoloring was applied. Run
`inspect_zeppelin_materials <Data root>` to inspect the exact active archive inputs.
