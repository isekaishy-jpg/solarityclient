//! Surface support at the foot point, as used by stock landing and step tests.

use glam::Vec3;

use super::face::extruded_edge_plane;
use super::{MovementCollisionPlane, MovementCollisionTriangle, MovementSweepError};

const PLAYER_SUPPORT_SLOPE: f32 = f32::from_bits(0x3f24_8dbb);
const OTHER_SUPPORT_SLOPE: f32 = f32::from_bits(0x3e31_d0d4);
const FOOTPRINT_TOLERANCE: f32 = f32::from_bits(0x3daa_aaab);

/// Support profile already resolved by the native unit-control policy.
///
/// `0x00716710` depends on control flags, unit type, and controlling-unit
/// identity. A caller must resolve that policy before selecting this profile;
/// being the selected character alone is not a substitute for the predicate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MovementSupportProfile {
    /// Uses the stricter player-control slope threshold at `0x00A37F0C`.
    PlayerControlled,
    /// Uses the other-unit slope threshold at `0x00A37F10`.
    Other,
}

impl MovementCollisionTriangle {
    /// Tests whether this surface supports the supplied foot point for landing.
    ///
    /// This combines `0x0075D340`'s strict normal-Z gate with `0x0075D0A0`'s
    /// vertically extruded triangle footprint. The caller supplies the point
    /// after sweep displacement. Height-to-surface distance is not retested;
    /// contact distance belongs to the preceding sweep. This method does not
    /// apply a landing event.
    ///
    /// # Errors
    /// Returns [`MovementSweepError::NonFiniteSupportPoint`] for NaN or infinity.
    pub fn supports_at(
        &self,
        foot_point: Vec3,
        profile: MovementSupportProfile,
    ) -> Result<bool, MovementSweepError> {
        if !foot_point.is_finite() {
            return Err(MovementSweepError::NonFiniteSupportPoint);
        }
        let threshold = match profile {
            MovementSupportProfile::PlayerControlled => PLAYER_SUPPORT_SLOPE,
            MovementSupportProfile::Other => OTHER_SUPPORT_SLOPE,
        };
        if self.normal.z <= threshold {
            return Ok(false);
        }

        // The native wrapper initializes all planes to +Z through zero and
        // ignores the extrusion helper's return. Preserve partially constructed
        // planes if a projected edge degenerates, rather than inventing a ray.
        let mut sides = [MovementCollisionPlane::through(Vec3::Z, Vec3::ZERO); 3];
        for (edge, side) in sides.iter_mut().enumerate() {
            let Some(plane) = extruded_edge_plane(
                self.vertices[edge],
                self.vertices[(edge + 1) % 3],
                self.vertices[(edge + 2) % 3],
                Vec3::Z,
            ) else {
                break;
            };
            *side = plane;
        }
        Ok(sides
            .iter()
            .all(|plane| plane.distance(foot_point) <= f64::from(FOOTPRINT_TOLERANCE)))
    }
}
