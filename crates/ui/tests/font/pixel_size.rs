use super::{ascender_pixels, raster_pixel_height, text_pixel_height};

// Original 0x006C22F0, scalable face ascender=1500/descender=-500.
// 0x00992780 (FreeType size activation) is the only replaced boundary.
#[test]
fn scalable_font_size_and_baseline_match_build_12340() {
    for (height, raster, ascender) in [
        (0.25, 2, 2),
        (1.0, 2, 2),
        (2.0, 2, 2),
        (12.49, 12, 9),
        (12.5, 13, 10),
        (18.0, 18, 14),
        (26.0, 26, 20),
        (32.0, 32, 24),
        (33.0, 32, 24),
        (62.0, 32, 24),
        (128.0, 32, 24),
    ] {
        assert_eq!(raster_pixel_height(height, 1.0), raster, "{height}");
        assert_eq!(ascender_pixels(1500, -500, raster), Some(ascender));
        assert_eq!(text_pixel_height(raster, None, 1.0), f64::from(raster));
    }
}

// 0x00483890 clears the pixel-font flag without rebuilding its font resource;
// 0x006C74D0/0x006C6CD0 then scale that resource to the explicit text height.
#[test]
fn explicit_text_height_scales_the_capped_font_resource() {
    let raster = raster_pixel_height(62.0, 1.5);
    assert_eq!(raster, 32);
    assert_eq!(text_pixel_height(raster, None, 1.5), 32.0);
    assert_eq!(text_pixel_height(raster, Some(64.0), 1.5), 96.0);
    assert_eq!(text_pixel_height(raster, Some(0.5), 1.5), 2.0);
}
