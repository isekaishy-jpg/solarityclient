//! Stock grounded intervals and speculative step/fall probes.

mod advance;
mod correction;
mod state;
mod step;

pub use state::*;

use crate::collision::{MovementCollisionVolume, MovementSweep, MovementSweepError};
use crate::movement::MovementGeometry;
use glam::Vec3;

const VECTOR_EPSILON: f32 = f32::from_bits(0x3480_0000);
const DISTANCE_EPSILON: f32 = f32::from_bits(0x3580_0000);
const SLOPE: f32 = f32::from_bits(0x3f24_8dbb);
const MINIMUM_PROGRESS: f32 = f32::from_bits(0x3a83_126f);

struct GroundQuery<'a, G: MovementGeometry + ?Sized> {
    interval: MovementGroundInterval,
    geometry: &'a mut G,
    unavailable: bool,
    skipped_time_ms: u32,
}

impl<G: MovementGeometry + ?Sized> GroundQuery<'_, G> {
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
        &mut self,
        position: Vec3,
        direction: Vec3,
        distance: f32,
    ) -> Result<Option<MovementSweep>, MovementGroundAdvanceError> {
        let volume = self.volume(position)?;
        if !direction.is_finite() || !distance.is_finite() {
            return Err(MovementSweepError::InvalidDisplacement.into());
        }
        if distance.abs() >= DISTANCE_EPSILON
            && !self.geometry.prepare_sweep(&volume, direction, distance)
        {
            self.unavailable = true;
            return Ok(None);
        }
        Ok(Some(volume.sweep_along(
            direction,
            distance,
            self.geometry.triangles(),
        )?))
    }

    fn classify(
        &self,
        state: &mut MovementGroundSnapshot,
        triangle: usize,
    ) -> Result<(bool, bool), MovementGroundAdvanceError> {
        let result = self.geometry.triangles()[triangle].classify_ground_contact(
            &self.volume(state.position)?,
            self.interval.profile.support(),
            state.step_anchor.is_some(),
        )?;
        if result.clear_step {
            state.step_anchor = None;
        }
        Ok((result.follow_surface, result.start_fall))
    }

    fn interrupted(
        &self,
        state: MovementGroundSnapshot,
    ) -> Result<MovementGroundAdvance<G::TriangleIdentity>, MovementGroundAdvanceError> {
        Ok(MovementGroundAdvance {
            consumed_ms: self.interval.duration_ms,
            continuation: MovementGroundContinuation::Grounded(MovementGroundState::new(state)?),
            reset_motion_anchor: false,
            contact_triangle: None,
            geometry_unavailable: true,
            skipped_time_ms: self.skipped_time_ms,
        })
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
