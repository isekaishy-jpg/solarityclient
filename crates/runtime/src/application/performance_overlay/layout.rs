//! Resolution-independent placement for the top-left FPS display.

/// Stock UI canvas height used by Glue and FrameXML projection.
pub(crate) const UI_HEIGHT: f32 = 768.0;
/// Inset from the logical top-left corner, in stock UI units.
pub(crate) const FPS_TEXT_TOP_LEFT: [f32; 2] = [12.0, 10.0];
/// Vertical region used to center the sixteen-unit font line.
pub(crate) const FPS_TEXT_REGION_HEIGHT: f32 = 24.0;

/// Derives the fixed-height logical canvas without moving the corner inset.
pub(crate) fn overlay_extent(pixel_extent: (u32, u32)) -> [f32; 2] {
    [
        pixel_extent.0 as f32 / pixel_extent.1 as f32 * UI_HEIGHT,
        UI_HEIGHT,
    ]
}
