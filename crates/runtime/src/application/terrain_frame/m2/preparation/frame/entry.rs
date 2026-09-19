//! Public entry points share one admission path and unconditional failure cleanup.

use super::super::super::{
    CrtRand, GameObjectFrameInput, M2CameraEffectScale, M2Frame, M2TransparentPass, M2VisibleFrame,
    RuntimeTerrainFrameError, VulkanRenderer, WorldCameraFrame, WorldFrustum, unit_effects,
};
use super::PendingM2Frame;

impl M2Frame {
    /// Restores a failed or abandoned frame before its owner can mutate placements.
    /// Domain errors retain precedence; this boundary reports only executor failure.
    pub(super) fn abandon_frame_preparation(&mut self) {
        let _profile = solarity_profiling::profile!("m2.frame_abandon_wait");
        let geometry = self.finish_geometry();
        self.restore_geometry_states();
        let poses = self.pose_batch.finish();
        for error in [geometry, poses].into_iter().filter_map(Result::err) {
            tracing::warn!(error = %error, "M2 frame abandonment encountered an executor failure");
        }
    }

    /// Drives the same staged preparation when a caller has no independent world work.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::application::terrain_frame) fn prepare_visible_draws_with_unit_effects(
        &mut self,
        renderer: &VulkanRenderer,
        cpu: &solarity_cpu::CpuExecutor,
        frustum: WorldFrustum,
        camera: WorldCameraFrame,
        first_transparent_pass: M2TransparentPass,
        fog_color: glam::Vec3,
        animation_time_ms: f32,
        effect_scale: M2CameraEffectScale,
        random: &mut CrtRand,
        game_objects: Option<GameObjectFrameInput<'_>>,
        unit_effect_callback: Option<&mut unit_effects::UnitEffectEventCallback<'_>>,
        world_lighting: Option<(
            solarity_rendering::M2SceneUniform,
            solarity_rendering::M2DirectionalLight,
        )>,
        mut spatial_lighting: Option<(
            &mut crate::application::terrain_coordinator::RuntimeTerrainCoordinator,
            solarity_systems::WorldEntityLightEnvironment,
            glam::Vec3,
            &solarity_asset::LiquidTypeCatalog,
        )>,
        shadow_projection: Option<solarity_rendering::WorldShadowProjection>,
        scenery_shadows: Option<
            crate::application::terrain_frame::shadow::SceneryShadowQueries<'_>,
        >,
    ) -> Result<M2VisibleFrame<'_>, RuntimeTerrainFrameError> {
        let pending = self.begin_visible_draws_with_unit_effects(
            renderer,
            cpu,
            frustum,
            camera,
            first_transparent_pass,
            fog_color,
            animation_time_ms,
            effect_scale,
            random,
            game_objects,
            unit_effect_callback,
            world_lighting,
            spatial_lighting
                .as_mut()
                .map(|(terrain, environment, ordinary, liquid_types)| {
                    (&mut **terrain, *environment, *ordinary, *liquid_types)
                }),
            shadow_projection,
            scenery_shadows,
        )?;
        pending.finish(
            cpu,
            spatial_lighting.map(|(terrain, environment, ..)| (terrain, environment)),
        )
    }

    /// Completes ordered callbacks and scene admission, then returns while geometry
    /// can still run. The caller must not advance gameplay or change the admitted
    /// scene before finishing; disjoint packet preparation may use its stable view.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::application::terrain_frame) fn begin_visible_draws_with_unit_effects(
        &mut self,
        renderer: &VulkanRenderer,
        cpu: &solarity_cpu::CpuExecutor,
        frustum: WorldFrustum,
        camera: WorldCameraFrame,
        first_transparent_pass: M2TransparentPass,
        fog_color: glam::Vec3,
        animation_time_ms: f32,
        effect_scale: M2CameraEffectScale,
        random: &mut CrtRand,
        game_objects: Option<GameObjectFrameInput<'_>>,
        unit_effect_callback: Option<&mut unit_effects::UnitEffectEventCallback<'_>>,
        world_lighting: Option<(
            solarity_rendering::M2SceneUniform,
            solarity_rendering::M2DirectionalLight,
        )>,
        spatial_lighting: Option<(
            &mut crate::application::terrain_coordinator::RuntimeTerrainCoordinator,
            solarity_systems::WorldEntityLightEnvironment,
            glam::Vec3,
            &solarity_asset::LiquidTypeCatalog,
        )>,
        shadow_projection: Option<solarity_rendering::WorldShadowProjection>,
        scenery_shadows: Option<
            crate::application::terrain_frame::shadow::SceneryShadowQueries<'_>,
        >,
    ) -> Result<PendingM2Frame<'_>, RuntimeTerrainFrameError> {
        match self.admit_visible_draws(
            renderer,
            cpu,
            frustum,
            camera,
            first_transparent_pass,
            fog_color,
            animation_time_ms,
            effect_scale,
            random,
            game_objects,
            unit_effect_callback,
            world_lighting,
            spatial_lighting,
            shadow_projection,
            scenery_shadows,
        ) {
            Ok(work) => Ok(PendingM2Frame {
                frame: Some(self),
                work,
                first_transparent_pass,
                animation_time_ms,
                world_lighting,
            }),
            Err(error) => {
                self.abandon_frame_preparation();
                Err(error)
            }
        }
    }
}
