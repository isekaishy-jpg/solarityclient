//! One stable RGBA coverage image and its append-only upload journal.

use super::super::{FontError, GLYPH_PADDING, RasterizedGlyph, atlas_overflow};
use crate::font::storage::FontBuffer;
use std::sync::Arc;

/// A page grows only into unused texels. Its lifetime token lets GPU owners
/// retain in-flight draws while retiring pages after their CPU owners depart.
#[derive(Debug, PartialEq)]
pub struct UiGlyphAtlasPage {
    identity: u64,
    extent: (u32, u32),
    rgba8: FontBuffer<u8>,
    revision: u64,
    changes: FontBuffer<(u64, [u32; 4])>,
    cursor: [u32; 3],
    lifetime: Arc<()>,
}

impl UiGlyphAtlasPage {
    /// Retains the initial pack exactly and starts append placement below it.
    pub(in super::super) fn packed(
        identity: u64,
        extent: (u32, u32),
        rgba8: FontBuffer<u8>,
        next_row: u32,
    ) -> Self {
        Self {
            identity,
            extent,
            rgba8,
            revision: 1,
            changes: FontBuffer::default(),
            cursor: [GLYPH_PADDING, next_row, 0],
            lifetime: Arc::new(()),
        }
    }

    pub(super) fn empty(
        identity: u64,
        extent: (u32, u32),
        budget: Option<&solarity_asset::AssetReadBudget>,
    ) -> Result<Self, FontError> {
        let bytes = u64::from(extent.0)
            .checked_mul(u64::from(extent.1))
            .and_then(|pixels| pixels.checked_mul(4))
            .and_then(|bytes| usize::try_from(bytes).ok())
            .ok_or_else(atlas_overflow)?;
        let mut rgba8 = FontBuffer::zeroed(budget, bytes)?;
        rgba8[..4].copy_from_slice(&[255; 4]);
        Ok(Self::packed(identity, extent, rgba8, GLYPH_PADDING))
    }

    /// Returns absence when the current shelf cannot fit this complete glyph.
    /// That is an allocation boundary, not a substitution for missing coverage.
    pub(super) fn insert(
        &mut self,
        glyph: &RasterizedGlyph,
    ) -> Result<Option<(u32, u32)>, FontError> {
        let padded_width = glyph
            .width()
            .checked_add(GLYPH_PADDING * 2)
            .ok_or_else(atlas_overflow)?;
        let [mut x, mut y, mut height] = self.cursor;
        if x.checked_add(padded_width)
            .is_none_or(|end| end > self.extent.0)
        {
            x = GLYPH_PADDING;
            y = y
                .checked_add(height)
                .and_then(|value| value.checked_add(GLYPH_PADDING * 2))
                .ok_or_else(atlas_overflow)?;
            height = 0;
        }
        if x.checked_add(padded_width)
            .is_none_or(|end| end > self.extent.0)
            || y.checked_add(glyph.height())
                .and_then(|end| end.checked_add(GLYPH_PADDING))
                .is_none_or(|end| end > self.extent.1)
        {
            return Ok(None);
        }
        self.changes.reserve_one(self.rgba8.policy())?;
        for row in 0..glyph.height() {
            for column in 0..glyph.width() {
                let destination = (((y + row) * self.extent.0 + x + column) * 4) as usize;
                let source = (row * glyph.width() + column) as usize;
                self.rgba8[destination..destination + 4].copy_from_slice(&[
                    255,
                    255,
                    255,
                    glyph.coverage()[source],
                ]);
            }
        }
        self.cursor = [x + padded_width, y, height.max(glyph.height())];
        self.revision += 1;
        self.changes.push(
            self.rgba8.policy(),
            (self.revision, [x, y, glyph.width(), glyph.height()]),
        )?;
        Ok(Some((x, y)))
    }

    /// Stable sampled-image identity, independent of later coverage additions.
    pub const fn identity(&self) -> u64 {
        self.identity
    }
    /// Page dimensions in texels.
    pub const fn extent(&self) -> (u32, u32) {
        self.extent
    }
    /// Monotonic coverage upload revision.
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    /// Current tightly packed RGBA coverage bytes.
    pub fn rgba8(&self) -> &[u8] {
        &self.rgba8
    }

    /// Each rectangle belongs to one newly cached glyph; no historical bitmap
    /// generations are retained. Coordinates are x/y/width/height in texels.
    pub fn changes_since(&self, revision: u64) -> impl Iterator<Item = [u32; 4]> + '_ {
        let first = self
            .changes
            .partition_point(|(value, _)| *value <= revision);
        self.changes[first..].iter().map(|(_, bounds)| *bounds)
    }

    /// Pins this page for resident draw owners independently of the font plan.
    pub fn lifetime(&self) -> Arc<()> {
        Arc::clone(&self.lifetime)
    }
}
