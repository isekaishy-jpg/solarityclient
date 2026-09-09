//! Displaced 7A8F20 scene windows, distinct from the near-portal visibility test.

use glam::Vec3;

use super::{WorldModelPortalProjectionFrame, WorldModelPortalProjector};
use crate::collision::world_model_visibility::WorldModelVisibilityError;

/// One exterior portal window sent to 795D00 and the scene depth banks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldModelExteriorPortalWindow {
    /// Min-Y, min-X, max-Y, max-X after native `(coordinate + 1) / 2`.
    /// Values are not clamped to the viewport.
    pub screen_window: [f32; 4],
    /// Greatest nonnegative forward-plane distance of all authored vertices.
    /// This uses the original polygon, including vertices beyond the input cap.
    pub depth: f32,
}

impl WorldModelPortalProjector {
    /// Projects 7A8F20's polygon displaced toward its adjacent group.
    ///
    /// The caller owns per-root portal visitation and calls this once at the
    /// first admitted encounter. A positive reference side negates the normal
    /// offset. Unlike 7A9090, this path has no near-portal full-window shortcut.
    /// The local forward plane is the one established by 7A6E00.
    ///
    /// # Errors
    /// Rejects nonfinite camera, transform, plane or polygon coordinates.
    pub fn project_exterior_polygon(
        &mut self,
        vertices: &[[f32; 3]],
        normal: Vec3,
        reference_side: i16,
        local_forward_plane: [f32; 4],
        frame: WorldModelPortalProjectionFrame,
    ) -> Result<Option<WorldModelExteriorPortalWindow>, WorldModelVisibilityError> {
        if !frame.is_finite()
            || !normal.is_finite()
            || !local_forward_plane.into_iter().all(f32::is_finite)
            || !vertices.iter().flatten().copied().all(f32::is_finite)
        {
            return Err(WorldModelVisibilityError::NonFiniteCoordinates);
        }
        // 7A8F20 stores normal * 009F1968 before 7A85E0 adds the offset.
        let mut offset = normal * 0.01;
        if reference_side > 0 {
            offset = -offset;
        }
        let Some(bounds) = self.project_clipped_polygon(vertices, offset, frame)? else {
            return Ok(None);
        };
        Ok(Some(WorldModelExteriorPortalWindow {
            screen_window: bounds.map(|coordinate| ((f64::from(coordinate) + 1.) * 0.5) as f32),
            depth: portal_depth(vertices, local_forward_plane),
        }))
    }
}

/// 7A70D0 retains the maximum in x87 through the full authored vertex array.
fn portal_depth(vertices: &[[f32; 3]], plane: [f32; 4]) -> f32 {
    let [nx, ny, nz, distance] = plane.map(f64::from);
    let mut maximum = 0_f64;
    let unrolled_end = vertices.len() / 4 * 4;
    for (index, point) in vertices.iter().enumerate() {
        let [x, y, z] = point.map(f64::from);
        // The four-vertex loop uses a different addition order for its second
        // vertex. Its tail uses that same order, with no intermediate spill.
        let depth = if index >= unrolled_end || index % 4 == 1 {
            ((ny * y + nx * x) + nz * z) + distance
        } else {
            ((ny * y + nz * z) + nx * x) + distance
        };
        maximum = maximum.max(depth);
    }
    maximum as f32
}
