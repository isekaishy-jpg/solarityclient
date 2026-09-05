//! Stock loading-card viewport fitting.

/// Native aspect of ordinary build-12340 loading artwork.
pub(crate) const STOCK_LOADING_ART_ASPECT: f32 = 4.0 / 3.0;

/// Native aspect of build-12340's `Wide` loading artwork.
///
/// `LoadingScreen.cpp` divides the normalized display aspect by
/// `0x00AB63B8 / 0x00AB63B4`. Those executable constants are `1.6` and
/// `1.3333334`, respectively, making the wide artwork 16:10 rather than 16:9.
pub(crate) const STOCK_WIDE_LOADING_ART_ASPECT: f32 = 8.0 / 5.0;

/// Returns the centered card bounds in bottom-left logical display coordinates.
///
/// Build 12340's `LoadingScreen.cpp` draw path at `0x0040A270` compares the
/// display and artwork aspect ratios, then calls `0x00681F60` to fit the
/// graphics viewport. The image and progress bar share that viewport. The
/// image retains its full UV range (`0x00AB6400`); unused display space is black.
pub(crate) fn centered_aspect_fit_bounds(
    viewport_extent: [f32; 2],
    authored_aspect: f32,
) -> [f32; 4] {
    debug_assert!(viewport_extent[0].is_finite() && viewport_extent[0] > 0.0);
    debug_assert!(viewport_extent[1].is_finite() && viewport_extent[1] > 0.0);
    debug_assert!(authored_aspect.is_finite() && authored_aspect > 0.0);

    let viewport_aspect = viewport_extent[0] / viewport_extent[1];
    let aspect_ratio = viewport_aspect / authored_aspect;
    let (left, right, bottom, top) = if aspect_ratio < 1.0 {
        let bottom = (1.0 - aspect_ratio) * 0.5;
        (0.0, 1.0, bottom, bottom + aspect_ratio)
    } else if aspect_ratio > 1.0 {
        let width = aspect_ratio.recip();
        let left = (1.0 - width) * 0.5;
        (left, left + width, 0.0, 1.0)
    } else {
        (0.0, 1.0, 0.0, 1.0)
    };
    [
        left * viewport_extent[0],
        bottom * viewport_extent[1],
        right * viewport_extent[0],
        top * viewport_extent[1],
    ]
}
