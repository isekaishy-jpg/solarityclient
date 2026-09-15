//! Borrowed batch execution, unconditional state return, and ordered output relocation.

use super::super::super::{
    M2Frame, M2GpuPlacement, M2TransparentDrawIndex, RuntimeTerrainFrameError, scene_element_count,
};
use super::super::diagnostics::Work;
use super::{GeometryBatch, GeometryInput, GeometryJob};
use solarity_rendering::{M2BonePose, M2BonePoseOverrides, M2MaterialPose};
use solarity_rendering::{M2CameraEffectScale, VulkanError, VulkanRenderer, WorldCameraFrame};

impl M2Frame {
    /// Starts the visible work list; prior state was returned before its frame ended.
    pub(in super::super) fn begin_geometry(&mut self) {
        debug_assert!(self.geometry_batch.jobs.iter().all(|job| !job.owns_effects));
        self.geometry_batch.active = 0;
    }

    /// Restores every model on success, validation errors and joined-worker panics.
    pub(in super::super) fn restore_geometry_states(&mut self) {
        for job in &mut self.geometry_batch.jobs[..self.geometry_batch.active] {
            if !job.owns_effects {
                continue;
            }
            let Some(input) = job.input else {
                unreachable!("owned effects have an admitted input");
            };
            let placement = &mut self.placements[input.placement_index];
            std::mem::swap(&mut job.particles, &mut placement.particles);
            std::mem::swap(&mut job.ribbons, &mut placement.ribbons);
            job.owns_effects = false;
        }
    }

    /// Joins all pure visible jobs before publishing any current geometry.
    pub(in super::super) fn execute_geometry(
        &mut self,
        renderer: &VulkanRenderer,
        cpu: Option<&solarity_cpu::CpuExecutor>,
        camera: WorldCameraFrame,
        effect_scale: M2CameraEffectScale,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let _profile = solarity_profiling::profile!("m2.geometry_batch");
        let catalog = renderer.m2_effect_draw_catalog();
        let sources = &self.sources;
        let twinkle = &self.particle_twinkle;
        let batch = &mut self.geometry_batch;
        batch.jobs.truncate(batch.active);
        let sample = |job: &mut GeometryJob| {
            let Some(input) = job.input else {
                unreachable!("only admitted geometry jobs execute");
            };
            let Some(source) = &sources[input.source_index] else {
                unreachable!("admitted source survives joined preparation");
            };
            job.result = Some(job.prepare(source, catalog, camera, effect_scale, twinkle));
        };
        if batch.active >= 8
            && let Some(cpu) = cpu
            && cpu.frame_worker_count() != 0
        {
            cpu.for_each_frame(&mut batch.jobs, sample)?;
        } else {
            for job in &mut batch.jobs {
                sample(job);
            }
        }
        Ok(())
    }

    /// Relocates model-local streams in the same order as the original traversal.
    pub(in super::super) fn publish_geometry(
        &mut self,
        work: &mut Work,
    ) -> Result<(usize, usize), RuntimeTerrainFrameError> {
        let _profile = solarity_profiling::profile!("m2.geometry_publication");
        let mut vertex_capacity = 0usize;
        let mut index_capacity = 0usize;
        for job in &mut self.geometry_batch.jobs[..self.geometry_batch.active] {
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
            if job.palette.pending {
                let start = input.bone_offset as usize;
                let end = start
                    .checked_add(job.pose.transforms().len())
                    .ok_or(VulkanError::M2BoneTransformRange)?;
                let output = self
                    .bone_transforms
                    .get_mut(start..end)
                    .ok_or(VulkanError::M2BoneTransformRange)?;
                output.copy_from_slice(job.pose.transforms());
            }
            work.geometry_outputs(
                !job.visible_draws.is_empty(),
                !job.particle_draws.is_empty(),
                !job.ribbon_draws.is_empty(),
                input.has_shadow_bones,
            );
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
            for draw in job.visible_draws.drain(..) {
                self.visible_draws.push(if draw.scene_order() == u32::MAX {
                    draw
                } else {
                    draw.with_scene_order(
                        draw.scene_order()
                            .checked_add(scene)
                            .ok_or(VulkanError::M2DrawIndexRange)?,
                    )
                });
            }
            for draw in job.particle_draws.drain(..) {
                self.particle_draws
                    .push(draw.relocate(vertices, indices, scene, effects)?);
            }
            for draw in job.ribbon_draws.drain(..) {
                self.ribbon_draws
                    .push(draw.relocate(ribbon_vertices, scene, effects)?);
            }
            for mut element in job.transparent_elements.drain(..) {
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
                    M2TransparentDrawIndex::Ribbon { first, count } => {
                        M2TransparentDrawIndex::Ribbon {
                            first: ribbons
                                .checked_add(first)
                                .ok_or(VulkanError::M2RibbonDrawVertexRange)?,
                            count,
                        }
                    }
                };
                self.transparent_elements.push(element);
            }
            vertex_capacity = vertex_capacity
                .checked_add(job.particle_vertex_capacity)
                .ok_or(solarity_rendering::M2ParticleMeshPlanError::VertexCount)?;
            index_capacity = index_capacity
                .checked_add(job.particle_index_capacity)
                .ok_or(solarity_rendering::M2ParticleMeshPlanError::IndexCount)?;
            self.particle_vertices
                .reserve(vertex_capacity.saturating_sub(self.particle_vertices.len()));
            self.particle_indices
                .reserve(index_capacity.saturating_sub(self.particle_indices.len()));
            self.particle_vertices
                .extend_from_slice(&job.particle_vertices);
            self.particle_indices
                .extend_from_slice(&job.particle_indices);
            self.ribbon_vertices.extend_from_slice(&job.ribbon_vertices);
            self.recoverable_errors.append(&mut job.recoverable_errors);
        }
        Ok((vertex_capacity, index_capacity))
    }
}

impl GeometryBatch {
    /// Transfers palettes and simulation storage without cloning live particle state.
    pub(in super::super) fn queue(
        &mut self,
        input: GeometryInput,
        placement: &mut M2GpuPlacement,
        pose: &mut M2BonePose,
        material_poses: &mut Vec<Option<M2MaterialPose>>,
        palette: Option<M2BonePoseOverrides<'_>>,
    ) {
        let batch = self;
        if batch.active == batch.jobs.len() {
            batch.jobs.push(GeometryJob::default());
        }
        let job = &mut batch.jobs[batch.active];
        job.reset();
        job.input = Some(input);
        job.owns_effects = true;
        job.palette.prepare(palette);
        std::mem::swap(&mut job.particles, &mut placement.particles);
        std::mem::swap(&mut job.ribbons, &mut placement.ribbons);
        std::mem::swap(&mut job.pose, pose);
        std::mem::swap(&mut job.material_poses, material_poses);
        batch.active += 1;
    }
}
