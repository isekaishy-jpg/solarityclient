//! Resumes admission after main work and returns all owned state on abandonment.

use super::super::super::{
    CrtRand, GameObjectFrameInput, M2VisibleFrame, RuntimeTerrainFrameError, VulkanRenderer,
};
use super::PendingM2Frame;
use super::input::AdmissionMode;
use crate::application::frame_pipeline::FrameWait;
use crate::application::terrain_coordinator::RuntimeTerrainCoordinator;

impl<'frame> PendingM2Frame<'frame> {
    /// Resumes ordered traversal and consumes the exact frame after independent
    /// preparation. Callers reborrow the same resource/scene owners; no large owner
    /// is cloned or held borrowed while independent packet uploads run.
    #[allow(clippy::too_many_arguments)] // Reborrow disjoint main owners after independent work.
    pub(in crate::application::terrain_frame) fn finish(
        mut self,
        renderer: &VulkanRenderer,
        cpu: &solarity_cpu::CpuExecutor,
        wait: &mut FrameWait<'_>,
        random: &mut CrtRand,
        game_objects: Option<GameObjectFrameInput<'_>>,
        mut spatial_lighting: Option<(
            &mut RuntimeTerrainCoordinator,
            solarity_systems::WorldEntityLightEnvironment,
            glam::Vec3,
            &solarity_asset::LiquidTypeCatalog,
        )>,
        scenery_shadows: Option<
            crate::application::terrain_frame::shadow::SceneryShadowQueries<'_>,
        >,
    ) -> Result<M2VisibleFrame<'frame>, RuntimeTerrainFrameError> {
        let _trace = self.trace.enter();
        while !self.admission.complete {
            let frame = self
                .frame
                .as_mut()
                .unwrap_or_else(|| unreachable!("pending frame retains its owner"));
            if let Some(index) = frame.frame_work.next_index() {
                frame.pose_batch.wait_for_root(index, wait)?;
            }
            frame.admit_visible_draws(
                renderer,
                self.view,
                &mut self.admission,
                if wait.is_native() {
                    AdmissionMode::Ready
                } else {
                    AdmissionMode::Complete
                },
                random,
                game_objects,
                spatial_lighting
                    .as_mut()
                    .map(|(terrain, environment, ordinary, liquids)| {
                        (&mut **terrain, *environment, *ordinary, *liquids)
                    }),
                scenery_shadows,
            )?;
        }
        let frame = self
            .frame
            .take()
            .unwrap_or_else(|| unreachable!("pending frame retains its owner"));
        frame.complete_visible_draws(
            cpu,
            wait,
            &mut self.admission.work,
            self.view.first_transparent_pass,
            self.view.animation_time_ms,
            self.view.world_lighting,
            spatial_lighting.map(|(terrain, environment, ..)| (terrain, environment)),
        )
    }
}

impl Drop for PendingM2Frame<'_> {
    fn drop(&mut self) {
        if let Some(frame) = self.frame.take() {
            let _trace = self.trace.enter();
            frame.abandon_frame_preparation();
        }
    }
}
