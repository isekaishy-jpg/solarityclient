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
mod status;

pub use c_simple_font::{
    FontCatalog, FontColor, FontDefinition, FontOutline, FontShadow, HorizontalJustification,
    VerticalJustification,
};
pub(crate) use gxu_font_string::wrap_line;
pub use gxu_font_string::{UiGlyphAtlasPlan, UiGlyphQuad, UiNativeTextStyle};
pub use gxu_font_util::{FontRasterization, FontSystem};
pub use i_gxu_font_glyph::RasterizedGlyph;
pub use status::FontError;
