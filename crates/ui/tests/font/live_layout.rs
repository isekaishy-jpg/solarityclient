//! Native text centering and edit-box caret layout invariants.

use super::{
    FontError, GlyphKey, LineFontKey, RasterizedGlyph, UiRuntimeText, edit_box_caret_metrics,
};
use crate::FontRasterization;
use solarity_asset::AssetPath;
use std::collections::HashMap;

/// Supplies controlled advances without depending on an installed font face.
fn glyph(advance: i64) -> RasterizedGlyph {
    RasterizedGlyph {
        width: 1,
        height: 1,
        bearing_x: 0,
        bearing_y: 1,
        advance_x_26_6: advance * 64,
        coverage: vec![255].into(),
    }
}

// Unhooked 0x006C6190, string bottom=697 and one 18px line. The native
// translation floors the final screen coordinate after vertical justification.
#[test]
fn short_font_string_centering_matches_original_translation() {
    use crate::VerticalJustification::{Bottom, Middle, Top};
    for (height, justification, native_y) in [
        (13.0, Top, 710.0),
        (13.0, Middle, 712.0),
        (13.0, Bottom, 715.0),
        (18.0, Top, 715.0),
        (18.0, Middle, 715.0),
        (18.0, Bottom, 715.0),
        (40.0, Top, 737.0),
        (40.0, Middle, 726.0),
        (40.0, Bottom, 715.0),
    ] {
        let offset = super::vertical_block_top(height, 18.0, [0.0; 4], justification);
        assert_eq!((697.0 + height + offset).floor(), native_y);
    }
}

/// Password cells and UTF-8 byte cursors retain their distinct layout domains.
#[test]
fn edit_box_caret_uses_password_cell_and_utf8_cursor_boundary() -> Result<(), FontError> {
    let face = AssetPath::new("Fonts\\FRIZQT__.TTF")?;
    let font = LineFontKey {
        face: face.clone(),
        pixel_height: 12,
        rasterization: FontRasterization::Antialiased,
    };
    let mut glyphs = HashMap::new();
    glyphs.insert(GlyphKey::new(&font, '*'), glyph(7));
    glyphs.insert(GlyphKey::new(&font, ' '), glyph(3));
    let text = UiRuntimeText {
        content: "éx".to_owned(),
        face,
        height: 12.0,
        text_height: None,
        rasterization: FontRasterization::Antialiased,
        outline_width: 0.0,
        color: [1.0; 4],
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        spacing: 0.0,
        word_wrap: false,
        non_space_wrap: false,
        max_lines: 0,
        max_letters: 0,
        horizontal: crate::HorizontalJustification::Left,
        vertical: crate::VerticalJustification::Middle,
        draw_layer: crate::UiDrawLayer::Artwork,
        draw_sub_level: 0,
        password: true,
        multiline: false,
        text_insets: [0.0; 4],
        cursor: 2,
        selection: [2, 2],
        caret_visible: true,
        highlight_color: [96.0 / 255.0, 96.0 / 255.0, 96.0 / 255.0, 1.0],
    };

    assert_eq!(
        edit_box_caret_metrics(&text, &font, &glyphs, 1.0, 100.0),
        (7.0, 7.0)
    );
    Ok(())
}
