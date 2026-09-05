//! Fall landing, ceiling, and horizontal response from `0x00760FC0`.

use glam::{Vec2, Vec3};

use super::{
    DEGENERATE_TOLERANCE, DIRECTION_TOLERANCE, MovementCollisionTriangle, MovementCollisionVolume,
    MovementSupportProfile, MovementSweepError,
};

/// Native continuation decision for one fall collision interval.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MovementFallContactKind {
    /// No selected triangle; consume the supplied displacement and interval.
    Clear,
    /// Continue the remaining displacement with the returned XY correction.
    Slide,
    /// The selected triangle supports the displaced foot point.
    Land,
    /// An upward top-face contact before the apex requests a fresh downward fall.
    Ceiling,
}

/// Pure geometry inputs for the movement owner's contact-time calculation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FallContactGeometry {
    /// Displacement after sweep clamping, before contact-time correction.
    pub(crate) displacement: Vec3,
    /// Allowed distance reported by the geometric sweep.
    pub(crate) distance: f32,
    /// Add this to the remaining displacement's XY components for a slide.
    pub(crate) horizontal_correction: Vec2,
    /// Landing, ceiling reset, slide, or clear travel.
    pub(crate) kind: MovementFallContactKind,
    /// Selected triangle in the caller's ordered candidate slice.
    pub(crate) last_triangle: Option<usize>,
}

impl MovementCollisionVolume {
    /// Resolves the geometric portion using an apex interval supplied by the
    /// movement owner. Clock/root calculation remains outside collision.
    pub(crate) fn fall_contact_geometry(
        &self,
        displacement: Vec3,
        triangles: &[MovementCollisionTriangle],
        apex_remaining_seconds: f64,
        horizontal_speed: f32,
        support_profile: MovementSupportProfile,
    ) -> Result<FallContactGeometry, MovementSweepError> {
        let sweep = self.sweep(displacement, triangles)?;
        let maximum = displacement.as_dvec3().length() as f32;
        let mut result = FallContactGeometry {
            displacement,
            distance: if maximum < DIRECTION_TOLERANCE {
                maximum
            } else {
                sweep.distance()
            },
            horizontal_correction: Vec2::ZERO,
            kind: MovementFallContactKind::Clear,
            last_triangle: sweep.last_triangle(),
        };
        // The fall caller keeps its original delta when the sweep selects no
        // triangle, including tiny requests that the narrow phase cuts to zero.
        let Some(index) = sweep.last_triangle() else {
            return Ok(result);
        };
        let direction = (displacement.as_dvec3() / f64::from(maximum)).as_vec3();
        result.displacement = direction * result.distance;
        let triangle = &triangles[index];
        result.kind = if triangle
            .supports_at(self.vertices[0] + result.displacement, support_profile)?
        {
            MovementFallContactKind::Land
        } else if direction.z > 0.0
            && apex_remaining_seconds >= 0.0
            && f64::from(result.distance) <= apex_remaining_seconds * f64::from(horizontal_speed)
            && sweep
                .planes()
                .iter()
                .any(|plane| (plane.normal().z - 1.0).abs() < DEGENERATE_TOLERANCE)
        {
            MovementFallContactKind::Ceiling
        } else {
            result.horizontal_correction = self.fall_correction(
                triangle,
                sweep.planes(),
                direction,
                result.distance,
                maximum,
            );
            MovementFallContactKind::Slide
        };
        Ok(result)
    }
}
