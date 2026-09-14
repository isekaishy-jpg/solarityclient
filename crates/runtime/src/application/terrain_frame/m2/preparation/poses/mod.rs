//! Pure unit pose work follows callbacks and precedes ordered scene publication.

mod admission;
mod frame;
mod input;

pub(in crate::application::terrain_frame::m2) use admission::PoseAdmission;

use super::super::{M2BonePose, RuntimeTerrainFrameError};
use input::PoseJob;

/// Only current dynamic owners retain palettes; scenery residency is not a job list.
#[derive(Default)]
pub(in crate::application::terrain_frame::m2) struct PoseBatch {
    jobs: Vec<PoseJob>,
    indices: Vec<Option<usize>>,
}

impl PoseBatch {
    /// Consumes a current exact-input palette once, transferring its storage.
    pub(in crate::application::terrain_frame::m2) fn take(
        &mut self,
        index: usize,
        model: &std::sync::Arc<solarity_asset::DecodedM2Model>,
        clock: solarity_rendering::M2AnimationClock,
        view: glam::Mat4,
        overrides: solarity_rendering::M2BonePoseOverrides<'_>,
        output: &mut M2BonePose,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        let Some(job) = self.indices.get(index).copied().flatten() else {
            return Ok(false);
        };
        self.jobs[job].take(model, clock, view, overrides, output)
    }
}
