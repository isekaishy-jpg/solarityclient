//! Native ground contact classification and combined sloped-foot normals.

use glam::Vec3;

use super::{
    CONTACT_TOLERANCE, DEGENERATE_TOLERANCE, MovementCollisionPlane, MovementCollisionTriangle,
    MovementCollisionVolume, MovementSupportProfile, MovementSweep, MovementSweepError,
};

const PLAYER_SLOPE: f32 = f32::from_bits(0x3f24_8dbb);
const FOOT_NORMAL_Z: f32 = f32::from_bits(0xbef3_86a4);
const COMBINED_FOOT_SCALE: f32 = f32::from_bits(0x3f23_7868);

/// Decisions from `0x0075D1C0`, applied by the ground movement owner.
pub(crate) struct GroundContactClassification {
    /// Use the surface-following correction instead of a new step test.
    pub(crate) follow_surface: bool,
    /// Clear the current native step state while retaining its unused anchor.
    pub(crate) clear_step: bool,
    /// A top contact cleared an active step and requests a fall transition.
    pub(crate) start_fall: bool,
}

impl MovementCollisionTriangle {
    /// Stock distinguishes slope admission from support at the foot: an
    /// admitted slope is followed even outside its footprint, while an actual
    /// footprint hit also clears the step state.
    pub(crate) fn classify_ground_contact(
        &self,
        volume: &MovementCollisionVolume,
        profile: MovementSupportProfile,
        step_active: bool,
    ) -> Result<GroundContactClassification, MovementSweepError> {
        if self.normal.z > profile.minimum_normal_z() {
            return Ok(GroundContactClassification {
                follow_surface: true,
                clear_step: self.supports_at(volume.vertices[0], profile)?,
                start_fall: false,
            });
        }
        let plane = MovementCollisionPlane::through(self.normal, self.vertices[0]);
        let touches_top = volume.vertices[5..]
            .iter()
            .any(|&point| plane.distance(point).abs() < f64::from(CONTACT_TOLERANCE));
        if self.normal.z >= 0.0 || !touches_top {
            return Ok(GroundContactClassification {
                follow_surface: step_active,
                clear_step: false,
                start_fall: false,
            });
        }
        Ok(GroundContactClassification {
            follow_surface: -self.normal.z > PLAYER_SLOPE,
            clear_step: step_active,
            start_fall: step_active,
        })
    }
}

impl MovementSweep {
    /// `0x0075DE80` only combines one through four sloped-foot contacts.
    /// A four-contact result is a valid zero vector, distinct from no result.
    pub(crate) fn combined_foot_normal(&self) -> Option<Vec3> {
        let planes = self.planes();
        if planes.is_empty()
            || planes.len() > 4
            || planes
                .iter()
                .any(|plane| (plane.normal().z - FOOT_NORMAL_Z).abs() >= DEGENERATE_TOLERANCE)
        {
            return None;
        }
        match planes.len() {
            1 => Some(planes[0].normal()),
            2 => Some(
                ((planes[0].normal().as_dvec3() + planes[1].normal().as_dvec3())
                    * f64::from(COMBINED_FOOT_SCALE))
                .as_vec3(),
            ),
            3 => {
                // Among three of the four fixed foot faces, retain the single
                // face on the axis represented once, in native contact order.
                let x_faces = planes
                    .iter()
                    .filter(|plane| plane.normal().y.abs() < DEGENERATE_TOLERANCE)
                    .count();
                planes
                    .iter()
                    .find(|plane| (plane.normal().y.abs() < DEGENERATE_TOLERANCE) == (x_faces == 1))
                    .map(|plane| plane.normal())
            }
            4 => Some(Vec3::ZERO),
            _ => None,
        }
    }
}
