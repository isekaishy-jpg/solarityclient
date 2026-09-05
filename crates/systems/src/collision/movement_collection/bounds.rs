//! Validated bounds and ordered native map-grid selection.

use super::MovementCollectionError;
use glam::{Mat4, Vec3};

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

    /// Transforms an authored box with stock's axis-product accumulation.
    ///
    /// `0x007F9430` and `0x00984860` add each minimum/maximum product to
    /// translation, spilling to float after each axis. Transforming eight
    /// corners with SIMD changes those rounding boundaries.
    ///
    /// # Errors
    /// Returns [`MovementCollectionError::InvalidBounds`] for non-finite output.
    pub fn transformed(self, transform: Mat4) -> Result<Self, MovementCollectionError> {
        if !transform.is_finite() {
            return Err(MovementCollectionError::InvalidBounds);
        }
        let mut minimum = transform.w_axis.truncate();
        let mut maximum = minimum;
        for axis in 0..3 {
            for component in 0..3 {
                let basis = f64::from(transform.col(axis)[component]);
                let first = basis * f64::from(self.minimum[axis]);
                let second = basis * f64::from(self.maximum[axis]);
                minimum[component] = (f64::from(minimum[component]) + first.min(second)) as f32;
                maximum[component] = (f64::from(maximum[component]) + first.max(second)) as f32;
            }
        }
        Self::new(minimum, maximum)
    }

    /// Tests inclusive box overlap without expanding either box (`0x0078F370`).
    #[must_use]
    pub fn intersects(self, other: Self) -> bool {
        self.minimum.cmple(other.maximum).all() && other.minimum.cmple(self.maximum).all()
    }

    /// Tests strict positive extent on every axis (`0x0070BD20`).
    #[must_use]
    pub fn has_positive_extent(self) -> bool {
        self.minimum.cmplt(self.maximum).all()
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
