//! Validated bounds and ordered native map-grid selection.

use super::MovementCollectionError;
use glam::Vec3;

/// Validated world-space box passed to stock's triangle collection.
///
/// Callers must cover every sweep and private step/fall probe in an interval.
/// An empty result only means empty space once the residency owner has proved
/// that every intersecting terrain and object generation is available.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementCollisionBounds {
    pub(in crate::collision) minimum: Vec3,
    pub(in crate::collision) maximum: Vec3,
}

impl MovementCollisionBounds {
    /// Admits a finite, ordered query box, including zero-width axes.
    ///
    /// # Errors
    /// Returns [`MovementCollectionError::InvalidBounds`] for invalid bounds.
    pub fn new(minimum: Vec3, maximum: Vec3) -> Result<Self, MovementCollectionError> {
        if !minimum.is_finite() || !maximum.is_finite() || minimum.cmpgt(maximum).any() {
            return Err(MovementCollectionError::InvalidBounds);
        }
        Ok(Self { minimum, maximum })
    }

    /// Returns the admitted minimum corner.
    #[must_use]
    pub const fn minimum(self) -> Vec3 {
        self.minimum
    }

    /// Returns the admitted maximum corner.
    #[must_use]
    pub const fn maximum(self) -> Vec3 {
        self.maximum
    }

    /// Rejects a face only when all vertices share an outside-box half-space.
    pub(in crate::collision) fn admits(self, vertices: [Vec3; 3], tolerance: f64) -> bool {
        (0..3).all(|axis| {
            let minimum = f64::from(self.minimum[axis]);
            let maximum = f64::from(self.maximum[axis]);
            !vertices.iter().all(|p| {
                let delta = f64::from(p[axis]) - minimum + tolerance;
                delta < 0.0
            }) && !vertices.iter().all(|p| {
                let delta = maximum - f64::from(p[axis]) + tolerance;
                delta < 0.0
            })
        })
    }

    /// The enabled BSP cache rejects faces wholly on a box boundary as well.
    pub(super) fn admits_cached_leaf(self, vertices: [Vec3; 3]) -> bool {
        (0..3).all(|axis| {
            !vertices
                .iter()
                .all(|point| point[axis] <= self.minimum[axis])
                && !vertices
                    .iter()
                    .all(|point| point[axis] >= self.maximum[axis])
        })
    }
}
