//! Live text shaping and sparse owner updates over immutable glyph coverage.

use std::collections::HashMap;

use super::{
    AtlasPlacement, EditBoxTextLayout, FontError, FontMetrics, GlyphKey, LineFontKey,
    LiveTextLayout, LocalGlyphQuad, RasterizedGlyph, TextOrigin, UiObjectKind, UiObjectRole,
    UiPresentationPacketKey, UiRegionGeometryPlan, UiRuntimeObjectPlan, UiRuntimeText,
    offset_bounds, presented_characters, runtime_font_key, wrap_line,
};
use crate::widget::nearest_owning_scroll_frame;

const DEFAULT_RETAINED_EDIT_BOX_LETTERS: usize = 256;
const MAX_RETAINED_EDIT_BOX_LETTERS: usize = 4_096;

/// One requested owner, including an explicit empty result when its text vanished.
pub(super) struct TextObjectLayout {
    pub(super) index: usize,
    pub(super) origin: Option<TextOrigin>,
    pub(super) edit_box: Option<EditBoxTextLayout>,
}

/// Targeted updates never allocate edit-box slots for unrelated live UI objects.
pub(super) struct TargetedTextLayout {
    pub(super) quads: Vec<LocalGlyphQuad>,
    pub(super) objects: Vec<TextObjectLayout>,
}

/// Places the text block relative to its owner's top, including short-field centering.
fn vertical_block_top(
    owner_height: f64,
    block_height: f64,
    insets: [f64; 4],
    justification: crate::VerticalJustification,
) -> f64 {
    let [_, _, inset_top, inset_bottom] = insets;
    let available_height = (owner_height - inset_top - inset_bottom).max(0.0);
    match justification {
        crate::VerticalJustification::Top => -inset_top,
        crate::VerticalJustification::Middle => {
            -inset_top - (available_height - block_height) * 0.5
        }
        crate::VerticalJustification::Bottom => -owner_height + inset_bottom + block_height,
    }
}

/// Builds the complete indexed metadata bank for an initial publication.
#[allow(clippy::too_many_arguments)]
pub(super) fn layout_live_quads(
    live: &UiRuntimeObjectPlan,
    geometry: &UiRegionGeometryPlan,
    pixels_per_ui_unit: f64,
    glyphs: &HashMap<GlyphKey, RasterizedGlyph>,
    placements: &HashMap<GlyphKey, AtlasPlacement>,
    metrics: &HashMap<LineFontKey, FontMetrics>,
    extent: (u32, u32),
) -> Result<LiveTextLayout, FontError> {
    let mut edit_boxes = vec![None; live.objects().len()];
    let mut origins = vec![None; live.objects().len()];
    let quads = layout_text_objects(
        live,
        geometry,
        pixels_per_ui_unit,
        glyphs,
        placements,
        metrics,
        extent,
        0..live.objects().len(),
        |index, origin, edit_box| {
            origins[index] = origin;
            edit_boxes[index] = edit_box;
        },
    )?;
    Ok(LiveTextLayout {
        quads,
        edit_boxes,
        origins,
    })
}

/// Lays out named owners with scratch storage bounded by the requested text.
#[allow(clippy::too_many_arguments)]
pub(super) fn layout_live_quads_for_objects(
    live: &UiRuntimeObjectPlan,
    geometry: &UiRegionGeometryPlan,
    pixels_per_ui_unit: f64,
    glyphs: &HashMap<GlyphKey, RasterizedGlyph>,
    placements: &HashMap<GlyphKey, AtlasPlacement>,
    metrics: &HashMap<LineFontKey, FontMetrics>,
    extent: (u32, u32),
    object_indices: impl IntoIterator<Item = usize>,
) -> Result<TargetedTextLayout, FontError> {
    let mut objects = Vec::new();
    let quads = layout_text_objects(
        live,
        geometry,
        pixels_per_ui_unit,
        glyphs,
        placements,
        metrics,
        extent,
        object_indices,
        |index, origin, edit_box| {
            objects.push(TextObjectLayout {
                index,
                origin,
                edit_box,
            })
        },
    )?;
    Ok(TargetedTextLayout { quads, objects })
}

/// Shared layout publishes metadata directly into the caller's dense or sparse
/// result. Full publications need no second arena-sized intermediate allocation.
#[allow(clippy::too_many_arguments)]
fn layout_text_objects(
    live: &UiRuntimeObjectPlan,
    geometry: &UiRegionGeometryPlan,
    pixels_per_ui_unit: f64,
    glyphs: &HashMap<GlyphKey, RasterizedGlyph>,
    placements: &HashMap<GlyphKey, AtlasPlacement>,
    metrics: &HashMap<LineFontKey, FontMetrics>,
    extent: (u32, u32),
    object_indices: impl IntoIterator<Item = usize>,
    mut publish: impl FnMut(usize, Option<TextOrigin>, Option<EditBoxTextLayout>),
) -> Result<Vec<LocalGlyphQuad>, FontError> {
    let mut quads = Vec::new();
    for object_index in object_indices {
        let Some(object) = live.objects().get(object_index) else {
            return Err(FontError::Presentation {
                message: format!("live text object {object_index} is unavailable"),
            });
        };
        let Some(text) = &object.text else {
            publish(object_index, None, None);
            continue;
        };
        let Some(region) = geometry.region(object_index) else {
            publish(object_index, None, None);
            continue;
        };
        let packet_key = UiPresentationPacketKey::for_text(live, object_index);
        let retained_tooltip =
            packet_key.is_some_and(|key| key.strata() == crate::UiFrameStrata::Tooltip);
        let retained_empty_font_string = text.content.is_empty()
            && object.kind == UiObjectKind::FontString
            && packet_key.is_some();
        if text.content.is_empty()
            && object.kind != UiObjectKind::EditBox
            && !retained_tooltip
            && !retained_empty_font_string
        {
            publish(object_index, None, None);
            continue;
        }
        let font = runtime_font_key(text, pixels_per_ui_unit)?;
        let metrics = metrics.get(&font).ok_or_else(|| FontError::Presentation {
            message: format!(
                "live text object {object_index} has no retained metrics for {} at {}px",
                font.face, font.pixel_height
            ),
        })?;
        let rendered_pixel_height =
            crate::font::text_pixel_height(font.pixel_height, text.text_height, pixels_per_ui_unit);
        let glyph_pixels_per_ui_unit =
            pixels_per_ui_unit * f64::from(font.pixel_height) / rendered_pixel_height;
        let displayed = presented_characters(text);
        let lines = if object.kind == UiObjectKind::EditBox && !text.multiline {
            vec![
                displayed
                    .into_iter()
                    .filter(|presented| !matches!(presented.character, '\r' | '\n'))
                    .collect::<Vec<_>>(),
            ]
        } else {
            let mut lines = vec![Vec::new()];
            let mut previous_was_carriage_return = false;
            for presented in displayed {
                if presented.character == '\r' {
                    lines.push(Vec::new());
                    previous_was_carriage_return = true;
                } else if presented.character == '\n' {
                    if !previous_was_carriage_return {
                        lines.push(Vec::new());
                    }
                    previous_was_carriage_return = false;
                } else if let Some(line) = lines.last_mut() {
                    line.push(presented);
                    previous_was_carriage_return = false;
                }
            }
            lines
        };
        let mut lines = if lines.is_empty() {
            vec![Vec::new()]
        } else {
            lines
        };
        let owner = region.logical_bounds();
        let [inset_left, inset_right, _, _] = text.text_insets;
        let available_width = (owner.width() - inset_left - inset_right).max(0.0);
        // CSimpleButton owns a single-line label even though the nested
        // FontString begins with CSimpleFontString's ordinary wrap default.
        // Stock clips a long label to the button instead of creating a second
        // line; CharSelectCreateCharacterButton exposes the distinction.
        if matches!(
            object.kind,
            UiObjectKind::FontString | UiObjectKind::ScrollingMessageFrame
        ) && object.role != UiObjectRole::ButtonText
            && text.word_wrap
        {
            lines = lines
                .into_iter()
                .flat_map(|line| {
                    wrap_line(
                        &line,
                        available_width,
                        text.non_space_wrap,
                        |presented| presented.character,
                        |presented| {
                            glyphs
                                .get(&GlyphKey::new(&font, presented.character))
                                .map_or(0.0, |glyph| {
                                    glyph.advance_x_26_6() as f64 / 64.0 / glyph_pixels_per_ui_unit
                                })
                        },
                    )
                })
                .collect();
        }
        if text.max_lines > 0 {
            lines.truncate(text.max_lines as usize);
        }
        let line_height = rendered_pixel_height / pixels_per_ui_unit;
        let block_height =
            line_height * lines.len() as f64 + text.spacing * lines.len().saturating_sub(1) as f64;
        let block_top = vertical_block_top(
            owner.height(),
            block_height,
            text.text_insets,
            text.vertical,
        );
        let anchor_x = match text.horizontal {
            crate::HorizontalJustification::Left => inset_left,
            crate::HorizontalJustification::Center => inset_left + available_width * 0.5,
            crate::HorizontalJustification::Right => owner.width() - inset_right,
        };
        let origin = Some(TextOrigin::new([anchor_x, block_top], pixels_per_ui_unit));
        let ascender = metrics.ascender_26_6 as f64 / 64.0 / glyph_pixels_per_ui_unit;
        let color = text.color.map(|component| component as f32);
        // CSimpleScrollFrame clips every region beneath its assigned child,
        // not only SimpleHTML. CharacterCreate's race and class descriptions
        // are ordinary FontStrings and depend on this viewport inheritance.
        let clip_object = if matches!(
            object.kind,
            UiObjectKind::EditBox | UiObjectKind::ScrollingMessageFrame
        ) {
            Some(object_index)
        } else {
            nearest_owning_scroll_frame(live, object_index)
        };
        let mut primary_quads = Vec::new();
        let mut selection_quads = Vec::new();
        let mut caret_quads = Vec::new();
        let mut edit_box_layout = (object.kind == UiObjectKind::EditBox)
            .then(|| EditBoxTextLayout::new(text.content.len(), text.cursor, text.selection));
        if let Some(layout) = edit_box_layout.as_mut() {
            layout.reserve_lines(lines.len());
        }
        for (line_index, line) in lines.iter().enumerate() {
            let line_width = line.iter().try_fold(0.0, |width, presented| {
                let character = presented.character;
                let key = GlyphKey::new(&font, character);
                glyphs.get(&key).map_or_else(
                    || {
                        Err(FontError::Presentation {
                            message: format!(
                                "live text object {object_index} uses unavailable glyph {character:?}"
                            ),
                        })
                    },
                    |glyph| {
                        Ok(width
                            + glyph.advance_x_26_6() as f64 / 64.0 / glyph_pixels_per_ui_unit)
                    },
                )
            })?;
            let mut pen_x = match text.horizontal {
                crate::HorizontalJustification::Left => inset_left,
                crate::HorizontalJustification::Center => {
                    inset_left + (available_width - line_width) * 0.5
                }
                crate::HorizontalJustification::Right => owner.width() - inset_right - line_width,
            };
            let line_top = block_top - line_index as f64 * (line_height + text.spacing);
            let baseline = line_top - ascender;
            let caret = (object.kind == UiObjectKind::EditBox
                && object.edit_focused.unwrap_or(false)
                && !text.multiline
                && line_index == 0)
                .then(|| {
                    edit_box_caret_metrics(
                        text,
                        &font,
                        glyphs,
                        glyph_pixels_per_ui_unit,
                        available_width,
                    )
                });
            if let Some((caret_offset, caret_width)) = caret {
                // CSimpleEditBox scrolls its one-line text just enough to keep
                // the insertion cell inside the authored text insets.
                let caret_left = pen_x + caret_offset;
                let content_right = owner.width() - inset_right;
                if caret_left + caret_width > content_right {
                    pen_x += content_right - caret_width - caret_left;
                } else if caret_left < inset_left {
                    pen_x += inset_left - caret_left;
                }
            }
            let line_start_x = pen_x;
            if let Some(layout) = edit_box_layout.as_mut() {
                layout.push_line(baseline);
            }
            let selection_begin = text.selection[0].min(text.selection[1]);
            let selection_end = text.selection[0].max(text.selection[1]);
            for presented in line {
                let character = presented.character;
                let key = GlyphKey::new(&font, character);
                let glyph = glyphs.get(&key).ok_or_else(|| FontError::Presentation {
                    message: format!(
                        "live text object {object_index} uses unavailable glyph {character:?}"
                    ),
                })?;
                let advance = glyph.advance_x_26_6() as f64 / 64.0 / glyph_pixels_per_ui_unit;
                if let Some(layout) = edit_box_layout.as_mut() {
                    layout.push_cluster(
                        presented.source_begin,
                        presented.source_end,
                        line_index,
                        pen_x,
                        pen_x + advance,
                    );
                }
                if selection_begin != selection_end
                    && presented.source_end > selection_begin
                    && presented.source_begin < selection_end
                    && advance > 0.0
                {
                    selection_quads.push(LocalGlyphQuad {
                        page: 0,
                        packet_key,
                        object_index,
                        clip_object,
                        bounds: [
                            pen_x as f32,
                            (line_top - line_height) as f32,
                            (pen_x + advance) as f32,
                            line_top as f32,
                        ],
                        texture_coordinates: solid_coordinates(extent),
                        color: text.highlight_color.map(|component| component as f32),
                        caret: false,
                        reserved: false,
                    });
                }
                if glyph.width() > 0 && glyph.height() > 0 {
                    let placement = placements.get(&key).ok_or_else(|| {
                        FontError::Presentation {
                            message: format!(
                                "live text object {object_index} has no atlas placement for {character:?}"
                            ),
                        }
                    })?;
                    let left = pen_x + f64::from(glyph.bearing_x()) / glyph_pixels_per_ui_unit;
                    let top = baseline + f64::from(glyph.bearing_y()) / glyph_pixels_per_ui_unit;
                    let right = left + f64::from(glyph.width()) / glyph_pixels_per_ui_unit;
                    let bottom = top - f64::from(glyph.height()) / glyph_pixels_per_ui_unit;
                    let u0 = placement.x as f32 / placement.extent.0 as f32;
                    let v0 = placement.y as f32 / placement.extent.1 as f32;
                    let u1 = (placement.x + glyph.width()) as f32 / placement.extent.0 as f32;
                    let v1 = (placement.y + glyph.height()) as f32 / placement.extent.1 as f32;
                    primary_quads.push(LocalGlyphQuad {
                        page: placement.page,
                        packet_key,
                        object_index,
                        clip_object,
                        bounds: [left as f32, bottom as f32, right as f32, top as f32],
                        texture_coordinates: [[u0, v0], [u0, v1], [u1, v0], [u1, v1]],
                        color: presented.color.unwrap_or(color),
                        caret: false,
                        reserved: false,
                    });
                }
                pen_x += advance;
            }
            if let Some((caret_offset, caret_width)) = caret {
                // Build 12340 presents the insertion point as a full
                // character-cell block. Pixel (0, 0) is the atlas-owned solid
                // coverage sample reserved by `compose_atlas`. Retain that
                // final quad while the blink is dark so a half-cycle changes
                // four color vertices instead of the complete material and
                // draw topology of the Glue frame.
                caret_quads.push(LocalGlyphQuad {
                    page: 0,
                    packet_key,
                    object_index,
                    clip_object,
                    bounds: [
                        (line_start_x + caret_offset) as f32,
                        (line_top - line_height) as f32,
                        (line_start_x + caret_offset + caret_width) as f32,
                        line_top as f32,
                    ],
                    texture_coordinates: solid_coordinates(extent),
                    color,
                    caret: true,
                    reserved: false,
                });
            }
        }
        // GxuFontString draws material passes in outline, shadow, then face
        // order. Keeping these as separate quads restores the black edging
        // and offset shadow that make Glue labels legible over animated M2s.
        let regular_start = quads.len();
        quads.extend(selection_quads);
        let outline = text.outline_width as f32;
        if outline > 0.0 {
            for primary in &primary_quads {
                for offset in [
                    [-outline, -outline],
                    [0.0, -outline],
                    [outline, -outline],
                    [-outline, 0.0],
                    [outline, 0.0],
                    [-outline, outline],
                    [0.0, outline],
                    [outline, outline],
                ] {
                    quads.push(offset_live_quad(primary, offset, [0.0, 0.0, 0.0, 1.0]));
                }
            }
        }
        let shadow_offset = [text.shadow_offset[0] as f32, text.shadow_offset[1] as f32];
        if shadow_offset != [0.0, 0.0] && text.shadow_color[3] > 0.0 {
            let shadow_color = text.shadow_color.map(|component| component as f32);
            quads.extend(
                primary_quads
                    .iter()
                    .map(|primary| offset_live_quad(primary, shadow_offset, shadow_color)),
            );
        }
        quads.extend(primary_quads);
        if object.kind == UiObjectKind::EditBox || retained_tooltip || retained_empty_font_string {
            // Empty labels need a source run that can grow in place, not a
            // fixed text-length budget. Tooltip templates instantiate many
            // unused lines; padding each to 128 letters made every unrelated
            // screen publication resolve and serialize thousands of blanks.
            // replace_live_object_quads retains each owner's observed peak,
            // and the mesh's source-run replacement preserves other owners.
            let letters = if object.kind != UiObjectKind::EditBox {
                1
            } else if text.max_letters == 0 {
                DEFAULT_RETAINED_EDIT_BOX_LETTERS
            } else {
                text.max_letters as usize
            }
            .min(MAX_RETAINED_EDIT_BOX_LETTERS);
            let regular_passes = 1
                + usize::from(object.kind == UiObjectKind::EditBox)
                + usize::from(outline > 0.0) * 8
                + usize::from(shadow_offset != [0.0, 0.0] && text.shadow_color[3] > 0.0);
            let regular_capacity = letters.saturating_mul(regular_passes);
            let used = quads.len() - regular_start;
            quads.extend((used..regular_capacity).map(|_| LocalGlyphQuad {
                page: 0,
                packet_key,
                object_index,
                clip_object,
                bounds: [0.0, -1.0, 1.0, 0.0],
                texture_coordinates: solid_coordinates(extent),
                color: [0.0; 4],
                caret: false,
                reserved: true,
            }));
            if object.kind == UiObjectKind::EditBox && caret_quads.is_empty() {
                caret_quads.push(LocalGlyphQuad {
                    page: 0,
                    packet_key,
                    object_index,
                    clip_object,
                    bounds: [0.0, -1.0, 1.0, 0.0],
                    texture_coordinates: solid_coordinates(extent),
                    color: [0.0; 4],
                    caret: true,
                    reserved: true,
                });
            }
        }
        quads.extend(caret_quads);
        publish(object_index, origin, edit_box_layout);
    }
    Ok(quads)
}

fn solid_coordinates(extent: (u32, u32)) -> [[f32; 2]; 4] {
    [[0.5 / extent.0 as f32, 0.5 / extent.1 as f32]; 4]
}

/// Reuses one glyph's atlas coverage for an offset live-text material pass.
fn offset_live_quad(source: &LocalGlyphQuad, offset: [f32; 2], color: [f32; 4]) -> LocalGlyphQuad {
    LocalGlyphQuad {
        page: source.page,
        packet_key: source.packet_key,
        object_index: source.object_index,
        clip_object: source.clip_object,
        bounds: offset_bounds(source.bounds, offset),
        texture_coordinates: source.texture_coordinates,
        color,
        caret: source.caret,
        reserved: source.reserved,
    }
}

/// Resolves the byte-indexed EditBox cursor to its visible insertion cell.
fn edit_box_caret_metrics(
    text: &UiRuntimeText,
    font: &LineFontKey,
    glyphs: &HashMap<GlyphKey, RasterizedGlyph>,
    pixels_per_ui_unit: f64,
    available_width: f64,
) -> (f64, f64) {
    let mut cursor = text.cursor.min(text.content.len());
    while !text.content.is_char_boundary(cursor) {
        cursor -= 1;
    }
    let presented = |character: char| {
        if text.password && !matches!(character, ' ' | '\t' | '\r' | '\n') {
            '*'
        } else {
            character
        }
    };
    let advance = |character| {
        glyphs
            .get(&GlyphKey::new(font, presented(character)))
            .map_or(0.0, |glyph| {
                glyph.advance_x_26_6() as f64 / 64.0 / pixels_per_ui_unit
            })
    };
    let offset = text.content[..cursor]
        .chars()
        .filter(|character| text.multiline || !matches!(character, '\r' | '\n'))
        .map(advance)
        .sum();
    let width = text.content[cursor..]
        .chars()
        .find(|character| text.multiline || !matches!(character, '\r' | '\n'))
        .map_or_else(|| advance(' '), advance)
        .max(pixels_per_ui_unit.recip())
        .min(available_width.max(0.0));
    (offset, width)
}

#[cfg(test)]
#[path = "../../../tests/font/live_layout.rs"]
mod tests;
