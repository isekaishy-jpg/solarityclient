//! Private font-layout invariants exercised outside production sources.

use std::collections::HashMap;

use solarity_asset::AssetPath;

use super::{
    FontError, FontRasterization, GlyphKey, LineFontKey, LocalGlyphQuad, RasterizedGlyph,
    UiRuntimeText, compose_atlas, edit_box_caret_metrics, group_live_quads,
    replace_live_object_quads,
};

fn glyph(advance: i64) -> RasterizedGlyph {
    RasterizedGlyph {
        width: 1,
        height: 1,
        bearing_x: 0,
        bearing_y: 1,
        advance_x_26_6: advance * 64,
        coverage: vec![255],
    }
}

fn local_quad(object_index: usize, caret: bool) -> LocalGlyphQuad {
    LocalGlyphQuad {
        packet_key: None,
        object_index,
        clip_object: None,
        bounds: [0.0; 4],
        texture_coordinates: [[0.0; 2]; 4],
        color: [1.0; 4],
        caret,
        reserved: false,
    }
}

/// Growth and removal only change requested owners; untouched payloads stay allocated.
#[test]
fn object_glyph_refresh_preserves_untouched_runs_and_draw_order() {
    let mut existing = group_live_quads(vec![
        local_quad(0, false),
        local_quad(0, false),
        local_quad(2, false),
        local_quad(4, false),
    ]);
    let untouched = existing[0].as_ptr();
    let replacements = vec![
        local_quad(1, true),
        local_quad(1, true),
        local_quad(4, true),
    ];
    replace_live_object_quads(&mut existing, &replacements, &[1, 2, 4]);
    assert_eq!(existing[0].as_ptr(), untouched);
    assert_eq!(
        existing
            .iter()
            .flatten()
            .filter(|quad| !quad.reserved)
            .map(|quad| (quad.object_index, quad.caret))
            .collect::<Vec<_>>(),
        [(0, false), (0, false), (1, true), (1, true), (4, true)]
    );
    assert!(existing[2][0].reserved);
    assert_eq!(existing[2][0].color, [0.0; 4]);
}

/// A simultaneously growing label cannot discard another label's retained slots.
#[test]
fn growing_glyph_owner_preserves_other_owners_retained_capacity() {
    let mut existing = group_live_quads(vec![
        local_quad(1, false),
        local_quad(2, false),
        local_quad(2, false),
        local_quad(2, false),
        local_quad(4, false),
    ]);
    let retained = existing[2].as_ptr();
    replace_live_object_quads(
        &mut existing,
        &[
            local_quad(1, true),
            local_quad(1, true),
            local_quad(1, true),
            local_quad(2, true),
        ],
        &[1, 2],
    );
    assert_eq!(existing[1].len(), 3);
    assert_eq!(existing[2].len(), 3);
    assert_eq!(existing[2].as_ptr(), retained);
    assert!(existing[2][0].caret);
    assert!(existing[2][1].reserved);
    assert_eq!(existing[2][1].color, [0.0; 4]);
    assert!(!existing[2][1].caret);
    replace_live_object_quads(
        &mut existing,
        &[
            local_quad(2, false),
            local_quad(2, false),
            local_quad(2, false),
        ],
        &[2],
    );
    assert_eq!(existing[2].as_ptr(), retained);
    assert!(existing[2].iter().all(|quad| !quad.reserved));
    assert_eq!(existing[4], [local_quad(4, false)]);
}

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

#[test]
fn atlas_reserves_opaque_padding_texel_for_text_primitives() -> Result<(), FontError> {
    let pixels = compose_atlas((1, 1), &[], &HashMap::new(), &HashMap::new())?;
    assert_eq!(pixels, [255; 4]);
    Ok(())
}

/// A natural-width title must survive translation into resolved screen bounds.
#[test]
fn word_wrap_preserves_exact_fit_after_screen_coordinate_subtraction() {
    // Installed FRIZQT at 26px, rendered at 1440/768 pixels per UI unit.
    let text = "Party Options"
        .chars()
        .zip([
            16., 15., 10., 10., 15., 7., 23., 17., 10., 7., 17., 16., 13.,
        ])
        .map(|(c, pixels)| (c, pixels / 1.875))
        .collect::<Vec<_>>();
    let natural: f64 = text.iter().map(|(_, advance)| advance).sum();
    let left = 1115.7200004577637;
    let resolved_width = (left + natural) - left;
    assert!(resolved_width < natural);
    let wrap = |items: &[(char, f64)], width, non_space| {
        super::wrap_line(items, width, non_space, |item| item.0, |item| item.1)
    };
    assert_eq!(
        wrap(&text, resolved_width, false),
        std::slice::from_ref(&text)
    );
    assert_eq!(wrap(&text, resolved_width - 1. / 64., false).len(), 2);
    let word = text
        .iter()
        .copied()
        .filter(|(c, _)| *c != ' ')
        .collect::<Vec<_>>();
    let natural: f64 = word.iter().map(|(_, advance)| advance).sum();
    let resolved_width = (left + natural) - left;
    assert_eq!(
        wrap(&word, resolved_width, true),
        std::slice::from_ref(&word)
    );
    assert_eq!(wrap(&word, resolved_width - 1. / 64., true).len(), 2);
}
