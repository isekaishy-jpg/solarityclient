//! Append-only coverage pages keep existing glyph coordinates and image identities.

mod page;

pub use page::UiGlyphAtlasPage;

use super::{
    ATLAS_ROW_WIDTH, AtlasPlacement, FontError, GLYPH_PADDING, RasterizedGlyph, next_identity,
};

/// Inserts new coverage into the current page or admits one additional page.
/// Existing rectangles never move or change, including while prior draws run.
pub(super) fn insert(
    pages: &mut crate::font::storage::FontBuffer<UiGlyphAtlasPage>,
    glyph: &RasterizedGlyph,
    budget: Option<&solarity_asset::AssetReadBudget>,
) -> Result<AtlasPlacement, FontError> {
    if glyph.width() == 0 || glyph.height() == 0 {
        return Ok(AtlasPlacement {
            x: 0,
            y: 0,
            page: 0,
            extent: pages[0].extent(),
        });
    }
    if let Some(page) = pages.last_mut()
        && let Some((x, y)) = page.insert(glyph)?
    {
        return Ok(AtlasPlacement {
            x,
            y,
            extent: page.extent(),
            page: pages.len() - 1,
        });
    }
    let dimension = |size: u32| {
        size.checked_add(GLYPH_PADDING * 3)
            .and_then(u32::checked_next_power_of_two)
            .map(|size| size.max(ATLAS_ROW_WIDTH))
            .ok_or_else(super::atlas_overflow)
    };
    let extent = (dimension(glyph.width())?, dimension(glyph.height())?);
    let mut page = UiGlyphAtlasPage::empty(next_identity(), extent, budget)?;
    let (x, y) = page.insert(glyph)?.ok_or_else(super::atlas_overflow)?;
    let placement = AtlasPlacement {
        x,
        y,
        extent,
        page: pages.len(),
    };
    pages.push(budget, page)?;
    Ok(placement)
}
