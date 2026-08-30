//! Owned glyph coverage and layout metrics.

/// One rasterized glyph ready for atlas placement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RasterizedGlyph {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) bearing_x: i32,
    pub(crate) bearing_y: i32,
    pub(crate) advance_x_26_6: i64,
    pub(crate) coverage: Vec<u8>,
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
