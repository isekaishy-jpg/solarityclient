# UI draw order

The native frame-level renderer (`00494AF0`) visits all eligible frames once
for each draw layer, from BACKGROUND through HIGHLIGHT. The shared texture and
glyph packet order now places that layer before the owning frame sequence.
Widget skin ordering and region sublevels remain inside each frame's layer.

Font strings retain the enclosing XML layer and authored sublevel during both
initial loading and runtime template construction. Previously every XML font
string started in ARTWORK, which discarded MirrorTimer's OVERLAY label. The
status bar lowers itself to its parent's frame level, so both defects could
place the bar fill over the label.

`ui_frame_layer_oracle.py` captures 128 sequences from the unchanged native
loop, intercepting only graphics bucket boundaries and virtual region emission.
The portable ordering test compares every sequence. XML tests cover static and
dynamic font inheritance. The archive-dependent mirror timer test runs the
stock XML and Lua, then checks that both Breath and Fatigue glyphs follow their
fill quads in the final renderer mesh. This is not a complete visual UI audit.
