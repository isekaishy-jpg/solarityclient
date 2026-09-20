//! Ordered model publication and receiver completion at the frame consumption boundary.

use super::super::super::{M2Frame, M2VisibleFrame, RuntimeTerrainFrameError};
use crate::application::terrain_coordinator::RuntimeTerrainCoordinator;

impl M2Frame {
    /// Runs ordered receivers once, then transfers pure lighting work to CPU.
    pub(super) fn begin_frame_receivers(
        &mut self,
        cpu: &solarity_cpu::CpuExecutor,
        animation_time_ms: f32,
        world_lighting: Option<(
            solarity_rendering::M2SceneUniform,
            solarity_rendering::M2DirectionalLight,
        )>,
        spatial_lighting: Option<(
            &mut RuntimeTerrainCoordinator,
            solarity_systems::WorldEntityLightEnvironment,
        )>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let mut frame_profile = solarity_profiling::profile!("m2.frame_receivers");
        self.pose_batch.report_consumption();
        solarity_profiling::profile_value!("m2.resident_placements", self.placements.len());
        solarity_profiling::profile_value!(
            "m2.dynamic_placements",
            self.placement_visibility.dynamic_indices().len()
        );
        solarity_profiling::profile_value!("m2.particle_vertices", self.particle_vertices.len());
        solarity_profiling::profile_value!("m2.bone_transforms", self.geometry_batch.bone_count());
        if world_lighting.is_some() {
            self.prepare_visible_receivers(animation_time_ms, spatial_lighting)?;
        }
        frame_profile.mark("visible receiver queries");
        if let Some((base, exterior)) = world_lighting {
            let dependency = self.geometry_completion();
            self.scene_lighting
                .begin_finish(Some(cpu), dependency.as_ref(), base, exterior)?;
        }
        Ok(())
    }

    /// Borrows immutable final packets only after every phase has returned its state.
    pub(super) fn visible_frame(
        &self,
        water_scene_order: u32,
        particle_vertex_capacity: usize,
        particle_index_capacity: usize,
    ) -> M2VisibleFrame<'_> {
        M2VisibleFrame {
            trace: solarity_profiling::TraceContext::capture(),
            instance_scenes: &self.scene_lighting.scenes,
            water_scene_order,
            bone_transforms: &self.geometry_batch,
            draws: &self.visible_draws,
            shadow_draws: &self.shadow_draws,
            environment_shadow_draws: &self.environment_shadow_draws,
            particle_vertices: &self.particle_vertices,
            particle_indices: &self.particle_indices,
            particle_draws: &self.particle_draws,
            particle_vertex_capacity,
            particle_index_capacity,
            ribbon_vertices: &self.ribbon_vertices,
            ribbon_draws: &self.ribbon_draws,
            glue_directional_lights: &self.glue_directional_lights,
            glue_point_lights: &self.glue_point_lights,
        }
    }
}
