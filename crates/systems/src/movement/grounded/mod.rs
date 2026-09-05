//! Stock grounded intervals and speculative step/fall probes.

mod advance;
mod correction;
mod state;
mod step;

pub use state::*;

use crate::collision::{MovementCollisionTriangle, MovementCollisionVolume, MovementSweep};
use glam::Vec3;

const VECTOR_EPSILON: f32 = f32::from_bits(0x3480_0000);
const DISTANCE_EPSILON: f32 = f32::from_bits(0x3580_0000);
const SLOPE: f32 = f32::from_bits(0x3f24_8dbb);
const MINIMUM_PROGRESS: f32 = f32::from_bits(0x3a83_126f);

struct GroundQuery<'a> {
    interval: MovementGroundInterval,
    triangles: &'a [MovementCollisionTriangle],
}

impl GroundQuery<'_> {
    fn volume(
        &self,
        position: Vec3,
    ) -> Result<MovementCollisionVolume, MovementGroundAdvanceError> {
        Ok(MovementCollisionVolume::new(
            position,
            self.interval.radius,
            self.interval.height,
        )?)
    }

    fn sweep(
        &self,
        position: Vec3,
        direction: Vec3,
        distance: f32,
    ) -> Result<MovementSweep, MovementGroundAdvanceError> {
        Ok(self
            .volume(position)?
            .sweep_along(direction, distance, self.triangles)?)
    }

    fn classify(
        &self,
        state: &mut MovementGroundSnapshot,
        triangle: usize,
    ) -> Result<(bool, bool), MovementGroundAdvanceError> {
        let result = self.triangles[triangle].classify_ground_contact(
            &self.volume(state.position)?,
            self.interval.profile.support(),
            state.step_anchor.is_some(),
        )?;
        if result.clear_step {
            state.step_anchor = None;
        }
        Ok((result.follow_surface, result.start_fall))
    }

    fn remaining_step(&self, state: &MovementGroundSnapshot) -> f64 {
        let height = f64::from(self.interval.profile.step_height());
        state
            .step_anchor
            .map_or(height, |anchor| {
                height - (f64::from(state.position.z) - f64::from(anchor))
            })
            .max(0.0)
    }
}
