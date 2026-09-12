"""Generate the original rectangle-outline font used by tooltip publication tests.

Requires fonttools==4.59.2. No third-party font data or outlines are used.
The checked-in TTF is the test input; generation is not needed to run tests.
"""
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

characters = list(range(32, 127)) + [0x03A9]
names = {code: f"uni{code:04X}" for code in characters}
order = [".notdef", *names.values()]
builder = FontBuilder(1000, isTTF=True)
builder.setupGlyphOrder(order)
builder.setupCharacterMap(names)
glyphs, metrics = {}, {}
for index, name in enumerate(order):
    advance = 500 + (index % 4) * 50
    pen = TTGlyphPen(None)
    if name != names[32]:
        pen.moveTo((50, 0))
        pen.lineTo((advance - 50, 0))
        pen.lineTo((advance - 50, 700))
        pen.lineTo((50, 700))
        pen.closePath()
    glyphs[name] = pen.glyph()
    metrics[name] = (advance, 50 if name != names[32] else 0)
builder.setupGlyf(glyphs)
builder.setupHorizontalMetrics(metrics)
builder.setupHorizontalHeader(ascent=800, descent=-200)
builder.setupNameTable({
    "familyName": "Solarity Tooltip Fixture",
    "styleName": "Regular",
    "uniqueFontIdentifier": "Solarity-Tooltip-Fixture-1",
    "fullName": "Solarity Tooltip Fixture Regular",
    "psName": "SolarityTooltipFixture-Regular",
    "version": "Version 1.0",
})
builder.setupOS2(sTypoAscender=800, sTypoDescender=-200, usWinAscent=800, usWinDescent=200)
builder.setupPost()
builder.font["head"].created = 2082844800
builder.font["head"].modified = 2082844800
builder.font.recalcTimestamp = False
builder.save(Path(__file__).with_name("tooltip_fixture.ttf"))
