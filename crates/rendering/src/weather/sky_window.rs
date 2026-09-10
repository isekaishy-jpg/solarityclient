//! Native sky visibility intersection and D3D9 pixel coverage.

use crate::{WorldCameraError, WorldScreenWindow};

/// Positive sky scissor rectangle in normalized min-Y, min-X, max-Y, max-X.
/// Bounds may extend beyond the viewport until 7F09B0's intersection step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldSkyWindow([f32; 4]);

impl WorldSkyWindow {
    /// Complete sky visibility for a camera registered outdoors.
    pub const FULL: Self = Self([0., 0., 1., 1.]);

    /// Retains one merged portal rectangle before viewport intersection.
    ///
    /// # Errors
    /// Rejects nonfinite, empty, or reversed bounds.
    pub fn new(bounds: [f32; 4]) -> Result<Self, WorldCameraError> {
        if !bounds.into_iter().all(f32::is_finite)
            || bounds[0] >= bounds[2]
            || bounds[1] >= bounds[3]
        {
            return Err(WorldCameraError::ScreenWindow);
        }
        Ok(Self(bounds))
    }

    /// Returns normalized bounds in the native rectangle field order.
    #[must_use]
    pub const fn bounds(self) -> [f32; 4] {
        self.0
    }

    /// 48ED60 intersects the sky bank with the current normalized viewport.
    /// 7F09B0 skips both rendering and model advancement for zero-area results.
    #[must_use]
    pub fn clipped(self, viewport: WorldScreenWindow) -> Option<Self> {
        let viewport = [
            viewport.minimum_y(),
            viewport.minimum_x(),
            viewport.maximum_y(),
            viewport.maximum_x(),
        ]
        .map(|value| ((f64::from(value) + 1.) * 0.5) as f32);
        let bounds = std::array::from_fn(|axis| {
            let sky = self.0[axis];
            let view = viewport[axis];
            if (axis < 2 && sky > view) || (axis >= 2 && sky < view) {
                sky
            } else {
                view
            }
        });
        (bounds[0] < bounds[2] && bounds[1] < bounds[3]).then_some(Self(bounds))
    }

    /// 6A38D0 converts the ordinary backbuffer scissor to top-left pixel bounds.
    /// Lower edges add 0.5; upper edges add 1 before truncation. The viewport
    /// changes projection separately and must not be cropped to this rectangle.
    #[must_use]
    pub fn pixel_bounds(self, extent: [u32; 2]) -> [u32; 4] {
        let [min_y, min_x, max_y, max_x] = self.0.map(f64::from);
        let [width, height] = extent.map(|value| f64::from(value as f32));
        [
            ((min_x * width + 0.5) as u32).min(extent[0]),
            (((1. - max_y) * height + 0.5) as u32).min(extent[1]),
            ((max_x * width + 1.) as u32).min(extent[0]),
            (((1. - min_y) * height + 1.) as u32).min(extent[1]),
        ]
    }
}
