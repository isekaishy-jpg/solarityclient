//! Stock loading-card texture coordinate policy.

/// Returns centered texture coordinates which fill the viewport without
/// changing the loading artwork's authored aspect ratio.
///
/// Build 12340's `LoadingScreen.cpp` draw path at `0x0040A270` compares the
/// display and artwork aspect ratios. It crops the longer texture axis around
/// its midpoint instead of stretching the image to the display.
pub(crate) fn centered_aspect_fill_uv(
    viewport_extent: [f32; 2],
    authored_aspect: f32,
) -> [[f32; 2]; 4] {
    debug_assert!(viewport_extent[0].is_finite() && viewport_extent[0] > 0.0);
    debug_assert!(viewport_extent[1].is_finite() && viewport_extent[1] > 0.0);
    debug_assert!(authored_aspect.is_finite() && authored_aspect > 0.0);

    let viewport_aspect = viewport_extent[0] / viewport_extent[1];
    let aspect_ratio = viewport_aspect / authored_aspect;
    let (left, right, top, bottom) = if aspect_ratio < 1.0 {
        let visible_width = aspect_ratio;
        let left = (1.0 - visible_width) * 0.5;
        (left, left + visible_width, 0.0, 1.0)
    } else if aspect_ratio > 1.0 {
        let visible_height = aspect_ratio.recip();
        let top = (1.0 - visible_height) * 0.5;
        (0.0, 1.0, top, top + visible_height)
    } else {
        (0.0, 1.0, 0.0, 1.0)
    };
    [[left, top], [left, bottom], [right, top], [right, bottom]]
}
