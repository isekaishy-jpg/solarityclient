//! Mutable world geometry at each native ground, step, and fall probe.

use glam::Vec3;

use crate::collision::{MovementCollisionTriangle, MovementCollisionVolume};

/// Ordered candidates and stable identities within one admitted movement context.
///
/// The owner must establish complete initial interval coverage before advancing.
/// Each non-tiny sweep then calls `prepare_sweep` before consuming candidates.
/// A provider may replace/reorder its arrays on success. Unavailable collection
/// returns false and retains its pending/error cause for the outer owner; it
/// must never present missing geometry as a successful empty query.
pub trait MovementGeometry {
    /// Copied contact identity that survives replacement of the candidate array.
    type TriangleIdentity: Copy;

    /// Refreshes coverage in the body's coordinate space before one sweep.
    /// Returns false when geometry could not be made complete for this probe.
    fn prepare_sweep(
        &mut self,
        volume: &MovementCollisionVolume,
        direction: Vec3,
        distance: f32,
    ) -> bool;

    /// Returns the current complete candidates in native append order.
    fn triangles(&self) -> &[MovementCollisionTriangle];

    /// Copies the owner of an admitted index before another probe can replace it.
    /// Every index in `triangles()` must have a corresponding identity.
    fn triangle_identity(&self, triangle: usize) -> Option<Self::TriangleIdentity>;
}

/// Compatibility provider for callers that already own complete probe coverage.
pub(super) struct FixedMovementGeometry<'a>(pub &'a [MovementCollisionTriangle]);

impl MovementGeometry for FixedMovementGeometry<'_> {
    type TriangleIdentity = usize;

    fn prepare_sweep(&mut self, _: &MovementCollisionVolume, _: Vec3, _: f32) -> bool {
        true
    }

    fn triangles(&self) -> &[MovementCollisionTriangle] {
        self.0
    }

    fn triangle_identity(&self, triangle: usize) -> Option<usize> {
        (triangle < self.0.len()).then_some(triangle)
    }
}
