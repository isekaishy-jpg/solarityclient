//! Ordered model publication and receiver completion at the frame consumption boundary.

use super::super::super::{
    M2Frame, M2TransparentDrawIndex, M2TransparentPass, M2VisibleFrame, RuntimeTerrainFrameError,
    compare_m2_transparent, scene_element_count,
};
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

    /// Assigns transparent scene order once while receiver lighting can still run.
    pub(super) fn order_frame_transparency(
        &mut self,
        first_transparent_pass: M2TransparentPass,
    ) -> Result<u32, RuntimeTerrainFrameError> {
        let _profile = solarity_profiling::profile!("m2.transparent_publication");
        self.transparent_elements.sort_unstable_by(|left, right| {
            (left.pass != first_transparent_pass)
                .cmp(&(right.pass != first_transparent_pass))
                .then_with(|| compare_m2_transparent(&left.key, &right.key))
        });
        let first_transparent_order = scene_element_count(
            self.visible_draws.len(),
            self.particle_draws.len(),
            self.ribbon_draws.len(),
        )?;
        let water_scene_order = first_transparent_order
            .checked_add(
                self.transparent_elements
                    .iter()
                    .take_while(|element| element.pass == first_transparent_pass)
                    .count(),
            )
            .and_then(|index| u32::try_from(index).ok())
            .ok_or(solarity_rendering::VulkanError::M2DrawIndexRange)?;
        for (index, element) in self.transparent_elements.iter().enumerate() {
            let scene_order = first_transparent_order
                .checked_add(index)
                .and_then(|index| u32::try_from(index).ok())
                .ok_or(solarity_rendering::VulkanError::M2DrawIndexRange)?;
            match element.draw {
                M2TransparentDrawIndex::Mesh(draw_index) => {
                    let draw = self
                        .visible_draws
                        .get_mut(draw_index)
                        .ok_or(solarity_rendering::VulkanError::M2DrawIndexRange)?;
                    *draw = draw.with_scene_order(scene_order);
                }
                M2TransparentDrawIndex::Particle(draw_index) => {
                    let draw = self
                        .particle_draws
                        .get_mut(draw_index)
                        .ok_or(solarity_rendering::VulkanError::M2ParticleDrawIndexRange)?;
                    *draw = draw.with_scene_order(scene_order);
                }
                M2TransparentDrawIndex::Ribbon { first, count } => {
                    let draws = self
                        .ribbon_draws
                        .get_mut(first..first + count)
                        .ok_or(solarity_rendering::VulkanError::M2RibbonDrawVertexRange)?;
                    for draw in draws {
                        *draw = draw.with_scene_order(scene_order);
                    }
                }
            }
        }
        self.visible_draws.sort_by_key(|draw| draw.scene_order());
        self.particle_draws.sort_by_key(|draw| draw.scene_order());
        self.ribbon_draws.sort_by_key(|draw| draw.scene_order());
        Ok(water_scene_order)
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
