# FontString raster sizes and realm labels

The specification is the pinned build-12340 executable, SHA-256
`aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8`.

`0x006C22F0` rounds scalable font requests to physical pixels and limits their
raster height to 2–32 pixels. It computes the face baseline from the font's
ascender divided by ascender plus absolute descender, rounded at that raster
height. The baseline is passed through `0x006C2480` to `0x006C8CC0` and consumed
by `0x006C8C60` when placing bitmap rows in the stock atlas. FreeType's hinted
size ascender is not this value.

Ordinary FontStrings start with the pixel-font flag set (`0x00485240`). Their
displayed height uses the raster height (`0x00482290`, `0x006C74D0`). A changed
`SetTextHeight` clears that flag without recreating the font (`0x00483890`);
glyphs then scale from the retained raster. Assigning another font does not
restore the pixel-font flag. Text measurement and presentation must agree on
these two heights. This distinction fixes the oversized stock ZoneText fonts.

`0x006C6190` uses signed vertical centering even when a FontString is shorter
than its text. Combined with the stock baseline, this raises the 13-unit
`CharSelectRealmName` field above the visible Change Realm button. Its authored
anchors remain the stock `Interface/GlueXML/CharacterSelect.xml` anchors.

`tools/ghidra/font_layout_oracle.py` captures the original size/baseline
arithmetic with a controlled scalable face and executes string translation
without hooks. External font tests cover those captures and the retained-raster
distinction. These checks do not establish complete glyph-atlas equivalence or
pixel snapping during arbitrary animated/scaled UI transforms.

The final ordinary FontString origin now follows `0x006C6190`'s floor in
physical screen pixels after horizontal and vertical justification. The old
centering test applied that floor itself, while the actual glyph renderer
omitted it. `font_translation_oracle.py` captures 216 fractional-position
cases at 720p and 1440p. Face, outline, shadow, and caret quads share one
origin correction; individual glyph bearings keep their fractional values.
Retained object movement updates glyph translation separately from an
EditBox's border. Both the renderer and pointer-to-caret mapping consume the
same correction. This does not claim complete native atlas equivalence.

The text projection is a separate final step. `0x006C0BA0` calls `0x006BF5B0`
with a D3D9 viewport offset of `(-0.5, +0.5)`, projecting an integer glyph
origin half a framebuffer pixel right and down. D3D9 samples at integer
pixel centers, whereas Vulkan samples at half-integer centers
([Direct3D 9 coordinates](https://learn.microsoft.com/en-us/windows/win32/direct3d9/directly-mapping-texels-to-pixels),
[Vulkan framebuffer coordinates](https://github.khronos.org/Vulkan-Site/spec/latest/appendices/glossary.html)).
Preserving the native texel sampled by each output pixel therefore places
the Vulkan origin one physical pixel right and down from the floored origin.
This correction follows UI and owner scaling; it does not change font metrics,
line heights, or authored anchors. `font_projection_oracle.py` executes the
original projection and captures device submission at 720p and 1440p.

A saved original-client login screenshot at 2560x1440
(`WoWScrnShot_080226_040700.jpg`) independently confirms the vertical difference:
the email hint, Login, Options, Credits, Quit, Manage Account, Password, and
account caption were one pixel higher in the pre-correction capture. The
hint's best mask alignment is one pixel right and down. Centered labels also
have horizontal advance/kerning differences; this correction does not claim
to resolve those or make all glyph coverage identical.
After correction, the hint has the same pixel bounds as the reference, and
all seven gold labels have zero residual vertical translation. Login and
status-dialog captures also complete at 1280x720 and 2560x1440.

The login label also depends on the remembered realm. `GetServerName`
(`0x004DD900`, `0x006B0DC0`) reads the `realmName` CVar, including an empty string
when unset; returning nil makes `AccountLogin.lua` hide the region. Runtime
realm publication now keeps that CVar current. Config.wtf parsing retains spaces
inside quoted values, so names such as `GameMap Playerbots Test` survive restarts
and repeated saves.
