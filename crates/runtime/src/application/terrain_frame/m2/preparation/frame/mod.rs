//! Scoped main-thread continuation between M2 admission and ordered publication.

mod admission;
mod completion;
mod entry;
mod scene;

use super::super::{M2Frame, M2TransparentPass, M2VisibleFrame, RuntimeTerrainFrameError};
use super::diagnostics::Work;
use crate::application::terrain_coordinator::RuntimeTerrainCoordinator;

/// Pins the current M2 owner while independent main-thread work uses disjoint owners.
/// Geometry owns its simulation state until completion. Dropping an unfinished
/// preparation reclaims that state at an explicitly profiled abandonment boundary;
/// the non-Send frame owner keeps this cleanup on the coordinator thread.
#[must_use = "complete the M2 frame after independent scene work"]
pub(in crate::application::terrain_frame) struct PendingM2Frame<'frame> {
    frame: Option<&'frame mut M2Frame>,
    work: Work,
    first_transparent_pass: M2TransparentPass,
    animation_time_ms: f32,
    world_lighting: Option<(
        solarity_rendering::M2SceneUniform,
        solarity_rendering::M2DirectionalLight,
    )>,
}

impl<'frame> PendingM2Frame<'frame> {
    /// Consumes the exact admitted frame after independent preparation, using the
    /// same current spatial environment. All effect storage returns before errors.
    pub(in crate::application::terrain_frame) fn finish(
        mut self,
        cpu: &solarity_cpu::CpuExecutor,
        spatial_lighting: Option<(
            &mut RuntimeTerrainCoordinator,
            solarity_systems::WorldEntityLightEnvironment,
        )>,
    ) -> Result<M2VisibleFrame<'frame>, RuntimeTerrainFrameError> {
        let frame = self
            .frame
            .take()
            .unwrap_or_else(|| unreachable!("pending frame retains its owner"));
        frame.complete_visible_draws(
            cpu,
            &mut self.work,
            self.first_transparent_pass,
            self.animation_time_ms,
            self.world_lighting,
            spatial_lighting,
        )
    }
}

impl Drop for PendingM2Frame<'_> {
    fn drop(&mut self) {
        if let Some(frame) = self.frame.take() {
            frame.abandon_frame_preparation();
        }
    }
}
