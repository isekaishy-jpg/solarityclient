//! Owned glyph coverage and layout metrics.

/// One rasterized glyph ready for atlas placement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RasterizedGlyph {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) bearing_x: i32,
    pub(crate) bearing_y: i32,
    pub(crate) advance_x_26_6: i64,
    pub(crate) coverage: GlyphCoverage,
}

impl RasterizedGlyph {
    /// Returns the tightly packed bitmap width.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns the tightly packed bitmap height.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns the horizontal bitmap bearing in pixels.
    #[must_use]
    pub const fn bearing_x(&self) -> i32 {
        self.bearing_x
    }

    /// Returns the vertical bitmap bearing above the baseline in pixels.
    #[must_use]
    pub const fn bearing_y(&self) -> i32 {
        self.bearing_y
    }

    /// Returns horizontal advance in FreeType 26.6 fixed-point pixels.
    #[must_use]
    pub const fn advance_x_26_6(&self) -> i64 {
        self.advance_x_26_6
    }

    /// Returns tightly packed 8-bit coverage, one byte per pixel.
    #[must_use]
    pub fn coverage(&self) -> &[u8] {
        &self.coverage
    }
}

/// One allocation owner follows every glyph/atlas clone, including after cache trim.
#[derive(Clone)]
pub(crate) struct GlyphCoverage(std::sync::Arc<CoverageBytes>);
struct CoverageBytes {
    bytes: Vec<u8>,
    // Free the pixel allocation before releasing admission.
    _memory: Option<solarity_cpu::ByteReservation>,
}
impl GlyphCoverage {
    pub(crate) fn reserve(
        budget: Option<&solarity_asset::AssetReadBudget>,
        bytes: usize,
    ) -> Result<Option<solarity_cpu::ByteReservation>, super::FontError> {
        let bytes = bytes
            .checked_add(size_of::<CoverageBytes>() + 2 * size_of::<usize>())
            .ok_or(solarity_cpu::CpuError::StorageSizeOverflow)
            .map_err(solarity_asset::AssetError::from)?;
        budget
            .map(|budget| {
                budget.storage().reserve(
                    budget.class(),
                    solarity_cpu::CpuStorageKind::Result,
                    bytes,
                )
            })
            .transpose()
            .map_err(solarity_asset::AssetError::from)
            .map_err(Into::into)
    }
    pub(crate) fn admitted(
        bytes: Vec<u8>,
        mut memory: Option<solarity_cpu::ByteReservation>,
    ) -> Result<Self, super::FontError> {
        if let Some(memory) = &mut memory {
            let actual = bytes
                .capacity()
                .checked_add(size_of::<CoverageBytes>() + 2 * size_of::<usize>())
                .ok_or(solarity_cpu::CpuError::StorageSizeOverflow);
            if let Err(error) = actual.and_then(|actual| memory.resize(actual)) {
                drop(bytes);
                return Err(solarity_asset::AssetError::from(error).into());
            }
        }
        Ok(Self(std::sync::Arc::new(CoverageBytes {
            bytes,
            _memory: memory,
        })))
    }
    pub(crate) fn is_pinned(&self) -> bool {
        std::sync::Arc::strong_count(&self.0) > 1
    }
}
impl From<Vec<u8>> for GlyphCoverage {
    fn from(bytes: Vec<u8>) -> Self {
        Self(std::sync::Arc::new(CoverageBytes {
            bytes,
            _memory: None,
        }))
    }
}
impl std::ops::Deref for GlyphCoverage {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.0.bytes
    }
}
impl PartialEq for GlyphCoverage {
    fn eq(&self, other: &Self) -> bool {
        self.0.bytes == other.0.bytes
    }
}
impl Eq for GlyphCoverage {}
impl std::fmt::Debug for GlyphCoverage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.bytes.fmt(formatter)
    }
}
