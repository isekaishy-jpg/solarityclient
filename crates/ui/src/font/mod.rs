//! Font objects, glyph rasterization, string layout, and text draw preparation.
//!
//! `CSimpleFont.cpp`, `GxuFontString.cpp`, `GxuFontUtil.cpp`, and
//! `IGxuFontGlyph.cpp` establish the font family. FreeType supplies rasterized
//! glyphs; UI retains stock font-object and layout semantics.

mod c_simple_font;
mod gxu_font_misc_classes;
mod gxu_font_string;
mod gxu_font_util;
mod i_gxu_font_glyph;
