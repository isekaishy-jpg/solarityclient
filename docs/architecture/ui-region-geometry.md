# Region scale, anchors, screen clamping and cursor tooltips

Build-12340 region bounds are screen coordinates expressed in the region's own
units. Effective scale is the product of local scales along the parent chain.
Resolving an anchor therefore converts the target's bounds by
`target_effective_scale / own_effective_scale`, then adds the owning region's
offset. Width and height use the same owning scale. Scaling occurs around the
screen origin. A scaled frame anchored to the full screen continues to cover
the screen; it does not shrink around its previous center.

The retained geometry resolver and live Lua edge queries follow that convention.
`GetEffectiveScale` exposes the product. Render bounds multiply region bounds
by that effective scale, while hit testing uses the resulting screen rectangle.
Existing animation translations are composed separately from static anchors.

`SetClampedToScreen` participates in the layout mutation journal, including its
no-change check. Clamp insets are left, right, top, bottom in the frame's units.
The stock resolver corrects left and bottom first, then right and top, without
resizing. Negative insets can allow overflow. If a frame is larger than the
available rectangle, the final right/top correction wins. Children anchored to
a clamped frame resolve against its corrected bounds.

Cursor tooltip updates use the anonymous screen root, independently of owner and
parent. `ANCHOR_CURSOR` anchors BOTTOM at the cursor and ignores owner offsets.
`ANCHOR_CURSOR_RIGHT` anchors BOTTOMLEFT and adds owner offsets after dividing
the cursor by the tooltip's effective scale. Visible cursor tooltips update
after authored update callbacks, including tooltips without an `OnUpdate`
script. Unchanged anchor values do not dirty retained layout. `SetOwner` clears
previous anchors except for `ANCHOR_PRESERVE`; `ANCHOR_NONE` leaves subsequent
manually authored points in place during Show.

Both stock `GetCursorPosition` implementations return unscaled UI canvas units,
not window pixels. The native conversion reduces to normalized pointer X times
`aspect * 768`, and normalized pointer Y times `768`. This lets Lua divide by a
region's `GetEffectiveScale` before anchoring in that region's units.

## Evidence and checks

The source is the pinned build-12340 executable with SHA-256
`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`.

- `0x004893C0` resolves bounds and clamp corrections; edge routines
  `0x004892A0`, `0x00489100`, `0x00489070`, `0x00489330` use native anchor
  functions `0x0049C900` / `0x0049C9A0` and cached target bounds.
- `0x0049D0B0` converts a region edge to Lua units; `0x0049F790` returns
  effective scale. `0x0048A130` toggles clamping and `0x0048EB00` stores insets.
- `0x0061B040` chooses owner anchors; `0x0061B2E0` updates cursor anchors.
- `0x004DCB60` and `0x00510A10` publish cursor positions, using
  `0x0047BF90`, `0x0047BFF0`, `0x0047BFE0`, and `0x0047C050` conversions.

`tools/ghidra/ui_region_layout_oracle.py` captures 1,320 original region results.
Only virtual width, height, inset getters and the ordinary target's false
origin-rebase query provide fixture facts; anchor and clamp arithmetic executes
unchanged. `tools/ghidra/tooltip_cursor_oracle.py` records 36 original tooltip
SetPoint calls, replacing ordinary frame update with an empty callback and
recording the SetPoint boundary. It also captures 16 cursor positions through
both original Lua APIs, intercepting only the Lua number push.

`crates/ui/tests/stock_seed/region_geometry.rs` compares those fixtures through
production XML, Lua, pointer dispatch and retained geometry. It covers scaled
parents away from the screen origin, all screen edges, live clamp changes and
anchored children, callback-before-cursor ordering, different window sizes and
the stationary-pointer no-change path. These checks establish the documented
geometry operations; they do not establish every animation or tooltip fade
behavior.
