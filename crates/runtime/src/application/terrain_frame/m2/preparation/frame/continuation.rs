//! Explicit final parking and borrowed packet publication for scoped M2 frames.

use super::super::super::{
    CrtRand, GameObjectFrameInput, M2VisibleFrame, RuntimeTerrainFrameError,
};
use super::PendingM2Frame;
use super::progress::FrameStage;
use crate::application::frame_pipeline::FrameWait;
use crate::application::terrain_coordinator::RuntimeTerrainCoordinator;

impl<'frame> PendingM2Frame<'frame> {
    /// Convenience driver for callers with no independent main work. Both native
    /// and offline contexts resume the same cursor and park only after it yields.
    #[allow(clippy::too_many_arguments)] // Reborrow disjoint scene owners on each resume.
    pub(in crate::application::terrain_frame) fn finish(
        mut self,
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
        while !self.try_advance(
            cpu,
            random,
            game_objects,
            spatial_lighting
                .as_mut()
                .map(|(terrain, environment, ordinary, liquids)| {
                    (&mut **terrain, *environment, *ordinary, *liquids)
                }),
            scenery_shadows,
        )? {
            self.wait(wait)?;
        }
        self.into_visible_frame()
    }

    /// Exposes packets only after terminal phase publication returned every owner.
    pub(in crate::application::terrain_frame) fn into_visible_frame(
        mut self,
    ) -> Result<M2VisibleFrame<'frame>, RuntimeTerrainFrameError> {
        if self.stage != FrameStage::Ready {
            return Err(solarity_cpu::CpuError::BatchActive.into());
        }
        let _trace = self.trace.enter();
        let frame = self
            .frame
            .take()
            .unwrap_or_else(|| unreachable!("pending frame retains its owner"));
        Ok(frame.visible_frame(
            self.water_scene_order,
            self.publication.vertex_capacity,
            self.publication.index_capacity,
        ))
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
