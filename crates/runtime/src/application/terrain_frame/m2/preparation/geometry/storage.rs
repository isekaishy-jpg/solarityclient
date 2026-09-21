//! Worker admission reserves palette, draw streams and effect state before simulation.

use super::super::super::{M2GpuSource, RuntimeTerrainFrameError};
use super::{GeometryInput, GeometryJob};
use solarity_cpu::{
    CpuBuffer, CpuError, CpuStorageBudget, CpuStorageClass as Class, CpuStorageKind as Kind,
    CpuStorageReservation, CpuStorageWorkingSet,
};
use solarity_rendering::{M2ParticleMeshPlan, VulkanError};

/// Checked count arithmetic shares the existing frame-capacity error boundary.
fn add(left: usize, right: usize) -> Result<usize, RuntimeTerrainFrameError> {
    left.checked_add(right)
        .ok_or(VulkanError::WorldFrameCapacity.into())
}
/// Authored stream multiplicities must fit the process before allocating.
fn twice(value: usize) -> Result<usize, RuntimeTerrainFrameError> {
    value
        .checked_mul(2)
        .ok_or(VulkanError::WorldFrameCapacity.into())
}

/// Immutable stream maxima are computed before the first retained buffer changes.
#[derive(Default)]
struct OutputCounts {
    material_poses: usize,
    shadow_draws: usize,
    visible_draws: usize,
    transparent_elements: usize,
    particle_draws: usize,
    particle_vertices: usize,
    particle_indices: usize,
    ribbon_vertices: usize,
    ribbon_draws: usize,
    recoverable_errors: usize,
}

impl GeometryJob {
    fn output_counts(
        &self,
        input: &GeometryInput,
        source: &M2GpuSource,
    ) -> Result<OutputCounts, RuntimeTerrainFrameError> {
        let shadows = if (input.primary_shadow || input.environment_maps != 0)
            && source.mesh.is_some()
            && !input.shadow.retiring
        {
            source.draws.len()
        } else {
            0
        };
        let Some(visible) = input.visible else {
            return Ok(OutputCounts {
                material_poses: shadows,
                shadow_draws: shadows,
                ..OutputCounts::default()
            });
        };
        if source.particles.len() != source.model.animations().particles().len() {
            return Err(RuntimeTerrainFrameError::M2ParticleResourceCount {
                model: source.model.path().clone(),
                resource_count: source.particles.len(),
                emitter_count: source.model.animations().particles().len(),
            });
        }
        let mut vertices = 0;
        let mut indices = 0;
        for ((emitter, particle), resource) in source
            .model
            .animations()
            .particles()
            .iter()
            .zip(&self.particles)
            .zip(&source.particles)
        {
            if particle.unsupported.is_some() {
                continue;
            }
            let capacity = particle
                .simulation
                .capacity()
                .max(resource.maximum_particles);
            let (emitter_vertices, emitter_indices) =
                M2ParticleMeshPlan::buffer_capacity(emitter, capacity)?;
            vertices = add(vertices, emitter_vertices)?;
            indices = add(indices, emitter_indices)?;
        }
        let meshes = if source.mesh.is_some() && !visible.effect_retiring {
            twice(source.draws.len())?
        } else {
            0
        };
        let mut ribbon_vertices = 0;
        let mut ribbon_draws = 0;
        if !visible.effect_retiring {
            for (trail, passes) in self.ribbons.iter().zip(&source.ribbons) {
                if passes.is_empty() {
                    continue;
                }
                ribbon_vertices = add(ribbon_vertices, twice(trail.capacity())?)?;
                ribbon_draws = add(ribbon_draws, passes.len())?;
            }
        }
        let particles = source.particles.len();
        let transparent = add(add(meshes, particles)?, source.ribbons.len())?;
        Ok(OutputCounts {
            material_poses: shadows,
            shadow_draws: shadows,
            visible_draws: meshes,
            transparent_elements: transparent,
            particle_draws: particles,
            particle_vertices: vertices,
            particle_indices: indices,
            ribbon_vertices,
            ribbon_draws,
            recoverable_errors: particles,
        })
    }

    /// Admission is a separate stage inside this owned worker turn. It completes
    /// palette, bounded draw-stream and simulation storage growth before the
    /// numeric kernel. Required growth reports a typed error rather than waiting
    /// on other jobs, truncating particles, or doing an unbounded main-thread copy.
    pub(super) fn admit_working_set(
        &mut self,
        budget: &CpuStorageBudget,
        input: &GeometryInput,
        source: &M2GpuSource,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let counts = self.output_counts(input, source)?;
        let bones = source.model.animations().bones().len();
        let mut working_set = CpuStorageWorkingSet::default();
        self.pose
            .include_cpu_storage(budget, bones, &mut working_set)?;
        include(
            &self.material_poses,
            budget,
            counts.material_poses,
            &mut working_set,
        )?;
        include(
            &self.shadow_draws,
            budget,
            counts.shadow_draws,
            &mut working_set,
        )?;
        include(
            &self.visible_draws,
            budget,
            counts.visible_draws,
            &mut working_set,
        )?;
        include(
            &self.transparent_elements,
            budget,
            counts.transparent_elements,
            &mut working_set,
        )?;
        include(
            &self.particle_draws,
            budget,
            counts.particle_draws,
            &mut working_set,
        )?;
        include(
            &self.particle_vertices,
            budget,
            counts.particle_vertices,
            &mut working_set,
        )?;
        include(
            &self.particle_indices,
            budget,
            counts.particle_indices,
            &mut working_set,
        )?;
        include(
            &self.ribbon_vertices,
            budget,
            counts.ribbon_vertices,
            &mut working_set,
        )?;
        include(
            &self.ribbon_draws,
            budget,
            counts.ribbon_draws,
            &mut working_set,
        )?;
        include(
            &self.recoverable_errors,
            budget,
            counts.recoverable_errors,
            &mut working_set,
        )?;
        if input.visible.is_some() {
            for (particle, resource) in self.particles.iter().zip(&source.particles) {
                if particle.unsupported.is_none() {
                    particle.simulation.include_cpu_storage(
                        budget,
                        resource.maximum_particles,
                        &mut working_set,
                    )?;
                }
            }
            for trail in &self.ribbons {
                trail.include_cpu_storage(budget, &mut working_set)?;
            }
        }
        // Admission is atomic for the known model working set. Old buffers remain
        // usable on refusal, and competitors cannot consume later replacement space.
        let mut reservation = budget.reserve_working_set(Class::Frame, working_set.bytes())?;
        self.pose
            .reserve_cpu_storage_reserved(&mut reservation, bones)?;
        reserve(
            &mut self.material_poses,
            &mut reservation,
            counts.material_poses,
        )?;
        reserve(
            &mut self.shadow_draws,
            &mut reservation,
            counts.shadow_draws,
        )?;
        reserve(
            &mut self.visible_draws,
            &mut reservation,
            counts.visible_draws,
        )?;
        reserve(
            &mut self.transparent_elements,
            &mut reservation,
            counts.transparent_elements,
        )?;
        reserve(
            &mut self.particle_draws,
            &mut reservation,
            counts.particle_draws,
        )?;
        reserve(
            &mut self.particle_vertices,
            &mut reservation,
            counts.particle_vertices,
        )?;
        reserve(
            &mut self.particle_indices,
            &mut reservation,
            counts.particle_indices,
        )?;
        reserve(
            &mut self.ribbon_vertices,
            &mut reservation,
            counts.ribbon_vertices,
        )?;
        reserve(
            &mut self.ribbon_draws,
            &mut reservation,
            counts.ribbon_draws,
        )?;
        reserve(
            &mut self.recoverable_errors,
            &mut reservation,
            counts.recoverable_errors,
        )?;
        if input.visible.is_some() {
            for (particle, resource) in self.particles.iter_mut().zip(&source.particles) {
                if particle.unsupported.is_none() {
                    particle.simulation.reserve_cpu_storage_reserved(
                        &mut reservation,
                        resource.maximum_particles,
                    )?;
                }
            }
            for trail in &mut self.ribbons {
                trail.reserve_cpu_storage_reserved(&mut reservation)?;
            }
        }
        Ok(())
    }
}

fn include<T>(
    buffer: &CpuBuffer<T>,
    budget: &CpuStorageBudget,
    capacity: usize,
    working_set: &mut CpuStorageWorkingSet,
) -> Result<(), CpuError> {
    working_set.include(
        buffer.reservation_bytes(budget, Class::Frame, capacity)?,
        buffer.replacement_credit(capacity),
    )
}

fn reserve<T>(
    buffer: &mut CpuBuffer<T>,
    reservation: &mut CpuStorageReservation,
    capacity: usize,
) -> Result<(), CpuError> {
    buffer.reserve_reserved(reservation, Kind::Result, capacity)
}
