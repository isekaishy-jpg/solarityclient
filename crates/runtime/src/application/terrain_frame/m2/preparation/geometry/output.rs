//! Ordered relocation owns output streams without borrowing the worker batch.

use super::super::super::{M2TransparentDrawIndex, RuntimeTerrainFrameError, scene_element_count};
use super::super::diagnostics::Work;
use super::GeometryJob;
use solarity_rendering::VulkanError;

/// Exclusive fixed-capacity outputs are admitted before finalization runs.
pub(super) struct GeometryOutput<'a> {
    pub(super) published_bones: &'a mut usize,
    pub(super) visible_draws: &'a mut Vec<solarity_rendering::M2PreparedDraw>,
    pub(super) shadow_draws: &'a mut Vec<solarity_rendering::M2PreparedDraw>,
    pub(super) environment_shadow_draws: &'a mut Vec<solarity_rendering::WorldEnvironmentM2Caster>,
    pub(super) particle_draws: &'a mut Vec<solarity_rendering::M2ParticlePreparedDraw>,
    pub(super) ribbon_draws: &'a mut Vec<solarity_rendering::M2RibbonPreparedDraw>,
    pub(super) transparent_elements: &'a mut Vec<super::super::super::M2TransparentElement>,
    pub(super) particle_vertices: &'a mut Vec<solarity_rendering::M2ParticleRenderVertex>,
    pub(super) particle_indices: &'a mut Vec<u32>,
    pub(super) ribbon_vertices: &'a mut Vec<solarity_rendering::M2RibbonRenderVertex>,
    pub(super) recoverable_errors: &'a mut Vec<String>,
    pub(super) vertex_capacity: usize,
    pub(super) index_capacity: usize,
}

impl GeometryOutput<'_> {
    /// Relocates one completed model in native traversal order; no scheduler lock
    /// is held while growing or copying the final frame streams.
    pub(super) fn publish(
        &mut self,
        job: &mut GeometryJob,
        work: &mut Work,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let Some(result) = job.result.take() else {
            unreachable!("joined geometry must have a result");
        };
        result?;
        let Some(input) = job.input else {
            unreachable!("joined geometry has an input");
        };
        input.trace.link("m2.geometry.publish");
        if input.trace.is_sampled() {
            let owner = input.placement_index as u64 + 1;
            input.trace.value(
                "m2.output.mesh_draws",
                owner,
                0,
                job.visible_draws.len() as u64,
            );
            input.trace.value(
                "m2.output.particle_vertices",
                owner,
                0,
                job.particle_vertices.len() as u64,
            );
            input.trace.value(
                "m2.output.ribbon_vertices",
                owner,
                0,
                job.ribbon_vertices.len() as u64,
            );
            input.trace.value(
                "m2.output.palette_bones",
                owner,
                u64::from(job.palette.pending),
                job.pose.transforms().len() as u64,
            );
        }
        let bone_offset =
            u32::try_from(*self.published_bones).map_err(|_| VulkanError::M2BoneTransformRange)?;
        let has_shadow_bones = !job.shadow_draws.is_empty();
        if input.visible.is_some() || has_shadow_bones {
            *self.published_bones = self
                .published_bones
                .checked_add(job.pose.transforms().len())
                .ok_or(VulkanError::M2BoneTransformRange)?;
            job.publishes_palette = true;
        }
        for draw in job.shadow_draws.drain() {
            let draw = draw.relocate_bones(bone_offset)?;
            if input.primary_shadow {
                solarity_cpu::FixedWriter::new(self.shadow_draws).push(draw)?;
            }
            if input.environment_maps != 0 {
                solarity_cpu::FixedWriter::new(self.environment_shadow_draws).push(
                    solarity_rendering::WorldEnvironmentM2Caster {
                        draw,
                        maps: input.environment_maps,
                    },
                )?;
            }
        }
        work.geometry_outputs(
            !job.visible_draws.is_empty(),
            !job.particle_draws.is_empty(),
            !job.ribbon_draws.is_empty(),
            has_shadow_bones,
        );
        if input.visible.is_none() {
            return Ok(());
        }
        let meshes = self.visible_draws.len();
        let particles = self.particle_draws.len();
        let ribbons = self.ribbon_draws.len();
        let scene = u32::try_from(scene_element_count(meshes, particles, ribbons)?)
            .map_err(|_| VulkanError::M2DrawIndexRange)?;
        let effects = u32::try_from(self.transparent_elements.len())
            .map_err(|_| VulkanError::M2DrawIndexRange)?;
        let vertices = u32::try_from(self.particle_vertices.len())
            .map_err(|_| VulkanError::M2ParticleDrawVertexRange)?;
        let indices = u32::try_from(self.particle_indices.len())
            .map_err(|_| VulkanError::M2ParticleDrawIndexRange)?;
        let ribbon_vertices = u32::try_from(self.ribbon_vertices.len())
            .map_err(|_| VulkanError::M2RibbonDrawVertexRange)?;
        for draw in job.visible_draws.drain() {
            let draw = draw.relocate_bones(bone_offset)?;
            solarity_cpu::FixedWriter::new(self.visible_draws).push(
                if draw.scene_order() == u32::MAX {
                    draw
                } else {
                    draw.with_scene_order(
                        draw.scene_order()
                            .checked_add(scene)
                            .ok_or(VulkanError::M2DrawIndexRange)?,
                    )
                },
            )?;
        }
        for draw in job.particle_draws.drain() {
            solarity_cpu::FixedWriter::new(self.particle_draws)
                .push(draw.relocate(vertices, indices, scene, effects)?)?;
        }
        for draw in job.ribbon_draws.drain() {
            solarity_cpu::FixedWriter::new(self.ribbon_draws).push(draw.relocate(
                ribbon_vertices,
                scene,
                effects,
            )?)?;
        }
        for mut element in job.transparent_elements.drain() {
            element.key = element
                .key
                .relocate_producer(effects)
                .ok_or(VulkanError::M2DrawIndexRange)?;
            element.draw = match element.draw {
                M2TransparentDrawIndex::Mesh(index) => M2TransparentDrawIndex::Mesh(
                    meshes
                        .checked_add(index)
                        .ok_or(VulkanError::M2DrawIndexRange)?,
                ),
                M2TransparentDrawIndex::Particle(index) => M2TransparentDrawIndex::Particle(
                    particles
                        .checked_add(index)
                        .ok_or(VulkanError::M2ParticleDrawIndexRange)?,
                ),
                M2TransparentDrawIndex::Ribbon { first, count } => M2TransparentDrawIndex::Ribbon {
                    first: ribbons
                        .checked_add(first)
                        .ok_or(VulkanError::M2RibbonDrawVertexRange)?,
                    count,
                },
            };
            solarity_cpu::FixedWriter::new(self.transparent_elements).push(element)?;
        }
        self.vertex_capacity = self
            .vertex_capacity
            .checked_add(job.particle_vertex_capacity)
            .ok_or(solarity_rendering::M2ParticleMeshPlanError::VertexCount)?;
        self.index_capacity = self
            .index_capacity
            .checked_add(job.particle_index_capacity)
            .ok_or(solarity_rendering::M2ParticleMeshPlanError::IndexCount)?;
        solarity_cpu::FixedWriter::new(self.particle_vertices)
            .extend_from_slice(&job.particle_vertices)?;
        solarity_cpu::FixedWriter::new(self.particle_indices)
            .extend_from_slice(&job.particle_indices)?;
        solarity_cpu::FixedWriter::new(self.ribbon_vertices)
            .extend_from_slice(&job.ribbon_vertices)?;
        for error in job.recoverable_errors.drain() {
            solarity_cpu::FixedWriter::new(self.recoverable_errors).push(error)?;
        }
        Ok(())
    }
}
