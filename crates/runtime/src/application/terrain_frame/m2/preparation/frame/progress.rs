//! Nonblocking ordered phase transitions leave unrelated main owners available.

use super::super::super::{CrtRand, GameObjectFrameInput, RuntimeTerrainFrameError};
use super::PendingM2Frame;
use crate::application::frame_pipeline::FrameWait;
use crate::application::terrain_coordinator::RuntimeTerrainCoordinator;

/// Each transition occurs once; yielding never repeats a callback or publication.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FrameStage {
    Admission,
    Geometry,
    GeometryRetirement,
    PoseRetirement,
    Receivers,
    Lighting,
    Ready,
    Failed,
}

impl PendingM2Frame<'_> {
    /// Published sources become readable after ordered receiver callbacks. They
    /// stay immutable while the worker derives per-model uniform outputs.
    pub(in crate::application::terrain_frame) fn scene_lights(
        &self,
    ) -> Option<super::super::super::SceneLightInputs<'_>> {
        if !matches!(self.stage, FrameStage::Lighting | FrameStage::Ready) {
            return None;
        }
        let frame = self
            .frame
            .as_ref()
            .unwrap_or_else(|| unreachable!("pending frame retains its owner"));
        Some(frame.scene_lighting.inputs())
    }

    /// Admits ready roots between independent main steps. Receiver callbacks stay
    /// behind the original world packet-preparation boundary.
    pub(in crate::application::terrain_frame) fn try_admit(
        &mut self,
        random: &mut CrtRand,
        game_objects: Option<GameObjectFrameInput<'_>>,
        spatial_lighting: Option<(
            &mut RuntimeTerrainCoordinator,
            solarity_systems::WorldEntityLightEnvironment,
            glam::Vec3,
            &solarity_asset::LiquidTypeCatalog,
        )>,
        scenery_shadows: Option<
            crate::application::terrain_frame::shadow::SceneryShadowQueries<'_>,
        >,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if self.stage == FrameStage::Failed {
            return Err(solarity_cpu::CpuError::BatchInactive.into());
        }
        if self.stage != FrameStage::Admission {
            return Ok(());
        }
        let _trace = self.trace.enter();
        let frame = self
            .frame
            .as_mut()
            .unwrap_or_else(|| unreachable!("pending frame retains its owner"));
        if !self.admission.complete
            && let Err(error) = frame.admit_visible_draws(
                self.view,
                &mut self.admission,
                random,
                game_objects,
                spatial_lighting,
                scenery_shadows,
            )
        {
            self.stage = FrameStage::Failed;
            return Err(error);
        }
        if self.admission.complete {
            self.stage = FrameStage::Geometry;
        }
        Ok(())
    }

    /// Runs the ready ordered prefix, then returns every unrelated owner to main.
    /// A failure is terminal for this owner; Drop still restores all worker state.
    pub(in crate::application::terrain_frame) fn try_advance(
        &mut self,
        cpu: &solarity_cpu::CpuExecutor,
        random: &mut CrtRand,
        game_objects: Option<GameObjectFrameInput<'_>>,
        spatial_lighting: Option<(
            &mut RuntimeTerrainCoordinator,
            solarity_systems::WorldEntityLightEnvironment,
            glam::Vec3,
            &solarity_asset::LiquidTypeCatalog,
        )>,
        scenery_shadows: Option<
            crate::application::terrain_frame::shadow::SceneryShadowQueries<'_>,
        >,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        let result = self.advance(cpu, random, game_objects, spatial_lighting, scenery_shadows);
        if result.is_err() {
            self.stage = FrameStage::Failed;
        }
        result
    }

    /// No pending path enters an executor condition wait or leases a result.
    fn advance(
        &mut self,
        cpu: &solarity_cpu::CpuExecutor,
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
    ) -> Result<bool, RuntimeTerrainFrameError> {
        self.try_admit(
            random,
            game_objects,
            spatial_lighting
                .as_mut()
                .map(|(terrain, environment, ordinary, liquids)| {
                    (&mut **terrain, *environment, *ordinary, *liquids)
                }),
            scenery_shadows,
        )?;
        if self.stage == FrameStage::Admission {
            return Ok(false);
        }
        let _trace = self.trace.enter();
        let _profile = solarity_profiling::profile!("m2.frame_publication");
        let _cycles = solarity_profiling::profile_cycles!("m2.prepare_cpu");
        let frame = self
            .frame
            .as_mut()
            .unwrap_or_else(|| unreachable!("pending frame retains its owner"));
        loop {
            match self.stage {
                FrameStage::Admission => unreachable!("admission was serviced before publication"),
                FrameStage::Geometry => {
                    if !frame
                        .try_publish_geometry(&mut self.admission.work, &mut self.publication)?
                    {
                        return Ok(false);
                    }
                    self.stage = FrameStage::GeometryRetirement;
                }
                FrameStage::GeometryRetirement => {
                    if !frame.geometry_is_finished() {
                        return Ok(false);
                    }
                    let result = frame.finish_geometry(&mut FrameWait::Offline);
                    frame.restore_geometry_states();
                    result?;
                    self.stage = FrameStage::PoseRetirement;
                }
                FrameStage::PoseRetirement => {
                    if !frame.pose_batch.is_finished() {
                        return Ok(false);
                    }
                    frame.pose_batch.finish(&mut FrameWait::Offline)?;
                    self.stage = FrameStage::Receivers;
                }
                FrameStage::Receivers => {
                    frame.begin_frame_receivers(
                        cpu,
                        self.view.animation_time_ms,
                        self.view.world_lighting,
                        spatial_lighting
                            .as_mut()
                            .map(|(terrain, environment, ..)| (&mut **terrain, *environment)),
                    )?;
                    // Lighting now owns its input; failure below must still drain it.
                    self.stage = FrameStage::Lighting;
                    self.water_scene_order =
                        frame.order_frame_transparency(self.view.first_transparent_pass)?;
                }
                FrameStage::Lighting => {
                    if !frame.scene_lighting.is_ready() {
                        return Ok(false);
                    }
                    if frame.scene_lighting.has_pending() {
                        frame
                            .scene_lighting
                            .finish_pending(&mut FrameWait::Offline)?;
                    }
                    self.stage = FrameStage::Ready;
                }
                FrameStage::Ready => return Ok(true),
                FrameStage::Failed => return Err(solarity_cpu::CpuError::BatchInactive.into()),
            }
        }
    }

    /// Parks only after the outer driver has exhausted its permitted main work.
    /// Callers retain the exact owner and resume it after readiness, never restart it.
    pub(in crate::application::terrain_frame) fn wait(
        &self,
        wait: &mut FrameWait<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let _trace = self.trace.enter();
        let frame = self
            .frame
            .as_ref()
            .unwrap_or_else(|| unreachable!("pending frame retains its owner"));
        match self.stage {
            FrameStage::Admission => {
                if let Some(index) = frame.frame_work.next_index() {
                    frame.pose_batch.wait_for_root(index, wait)?;
                }
            }
            FrameStage::Geometry | FrameStage::GeometryRetirement => {
                frame.wait_geometry(&self.publication, wait)?
            }
            FrameStage::PoseRetirement => frame.pose_batch.wait_finished(wait)?,
            FrameStage::Lighting => frame.scene_lighting.wait_pending(wait)?,
            FrameStage::Receivers | FrameStage::Ready => {}
            FrameStage::Failed => return Err(solarity_cpu::CpuError::BatchInactive.into()),
        }
        Ok(())
    }
}
