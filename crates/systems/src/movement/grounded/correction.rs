//! Vertical surface following and horizontal wall response.

use super::{GroundQuery, MINIMUM_PROGRESS, MovementGroundSnapshot, SLOPE, VECTOR_EPSILON};
use crate::movement::MovementGeometry;
use glam::Vec3;

impl<G: MovementGeometry + ?Sized> GroundQuery<'_, G> {
    pub(super) fn follow_surface(
        &self,
        state: &MovementGroundSnapshot,
        direction: Vec3,
        distance: &mut f32,
        mut normal: Vec3,
        foot: Option<Vec3>,
    ) -> Vec3 {
        if let Some(foot) = foot
            && normal.z <= SLOPE
            && foot.as_dvec3().length_squared().abs() >= f64::from(VECTOR_EPSILON)
        {
            normal = -foot;
        }
        let projected = -normal.as_dvec3().dot(direction.as_dvec3()) * f64::from(*distance);
        let desired = if normal.z.abs() < VECTOR_EPSILON {
            if projected < 0.0 { -f32::MAX } else { f32::MAX }
        } else {
            (projected / f64::from(normal.z)) as f32
        };
        let remaining = self.remaining_step(state);
        let full = f64::from(self.interval.profile.step_height());
        let z = if state.step_anchor.is_some() && desired < 0.0 {
            *distance = 0.0;
            remaining
        } else if f64::from(desired) > remaining {
            *distance = (f64::from(*distance) / f64::from(desired) * remaining) as f32;
            remaining
        } else if f64::from(desired) < -full {
            *distance = -(f64::from(*distance) / f64::from(desired) * full) as f32;
            -full
        } else {
            f64::from(desired)
        };
        Vec3::new((0.0 * z) as f32, (0.0 * z) as f32, z as f32)
    }
}

pub(super) fn wall_correction(
    direction: Vec3,
    distance: f32,
    horizontal_remaining: f32,
    normal: Vec3,
) -> Vec3 {
    if normal.z < 0.0 && -normal.z > SLOPE {
        return Vec3::ZERO;
    }
    let mut horizontal = normal.truncate().as_dvec2();
    let squared = horizontal.length_squared();
    if squared > f64::from(VECTOR_EPSILON) {
        horizontal /= squared.sqrt();
    }
    let mut projected = -normal.as_dvec3().dot(direction.as_dvec3()) * f64::from(distance);
    let denominator = normal.truncate().as_dvec2().dot(horizontal);
    if denominator.abs() >= f64::from(VECTOR_EPSILON) {
        projected /= denominator;
    }
    let mut correction = (horizontal * (projected + f64::from(MINIMUM_PROGRESS))).as_vec2();
    let travel = direction.truncate().as_dvec2() * f64::from(distance) + correction.as_dvec2();
    let squared = travel.length_squared();
    if f64::from(horizontal_remaining).powi(2) < squared {
        let scale = f64::from(horizontal_remaining) / squared.sqrt();
        correction =
            (travel * scale - direction.truncate().as_dvec2() * f64::from(distance)).as_vec2();
    }
    correction.extend(0.0)
}
