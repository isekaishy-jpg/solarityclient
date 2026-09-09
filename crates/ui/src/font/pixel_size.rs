//! Build-12340 scalable font raster and display-height arithmetic.

/// Rounds the requested UI height before enforcing the native 2–32px raster range.
pub(crate) fn raster_pixel_height(height: f64, pixels_per_ui_unit: f64) -> u32 {
    (height * pixels_per_ui_unit).round().clamp(2.0, 32.0) as u32
}

/// Keeps pixel fonts at raster size and applies explicit text-height scaling separately.
pub(crate) fn text_pixel_height(
    raster_height: u32,
    text_height: Option<f64>,
    pixels_per_ui_unit: f64,
) -> f64 {
    text_height.map_or(f64::from(raster_height), |height| {
        (height * pixels_per_ui_unit).round().max(2.0)
    })
}

/// Uses face design metrics for the atlas baseline; a zero span cannot define one.
pub(super) fn ascender_pixels(ascender: i16, descender: i16, pixel_height: u32) -> Option<i64> {
    let ascender = i32::from(ascender);
    let span = ascender + i32::from(descender).abs();
    (span != 0).then(|| (ascender as f32 * pixel_height as f32 / span as f32).round() as i64)
}

#[cfg(test)]
#[path = "../../tests/font/pixel_size.rs"]
mod tests;
