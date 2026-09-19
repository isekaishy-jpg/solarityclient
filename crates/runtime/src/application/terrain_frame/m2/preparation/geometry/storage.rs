//! Authored output bounds are admitted before placement simulation changes owner.

use super::super::super::{M2GpuPlacement, M2GpuSource, RuntimeTerrainFrameError};
use super::{GeometryInput, GeometryJob};
use solarity_cpu::{CpuStorageBudget, CpuStorageClass as Class, CpuStorageKind as Kind};
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

impl GeometryJob {
    /// Declares packet and stream maxima from existing stock simulation capacities.
    /// Failure leaves placement-owned pose/particle/ribbon state untouched.
    pub(super) fn reserve_outputs(
        &mut self,
        budget: &CpuStorageBudget,
        input: &GeometryInput,
        placement: &M2GpuPlacement,
        source: &M2GpuSource,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let shadows = if (input.primary_shadow || input.environment_maps != 0)
            && source.mesh.is_some()
            && !input.shadow.retiring
        {
            source.draws.len()
        } else {
            0
        };
        self.shadow_draws
            .reserve(budget, Class::Frame, Kind::Result, shadows)?;
        let Some(visible) = input.visible else {
            return Ok(());
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
        let mut sorting = 0;
        for ((emitter, particle), resource) in source
            .model
            .animations()
            .particles()
            .iter()
            .zip(&placement.particles)
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
            sorting = sorting.max(capacity);
        }
        let meshes = if source.mesh.is_some() && !visible.effect_retiring {
            twice(source.draws.len())?
        } else {
            0
        };
        let mut ribbon_vertices = 0;
        let mut ribbon_draws = 0;
        if !visible.effect_retiring {
            for (trail, passes) in placement.ribbons.iter().zip(&source.ribbons) {
                if passes.is_empty() {
                    continue;
                }
                ribbon_vertices = add(ribbon_vertices, twice(trail.capacity())?)?;
                ribbon_draws = add(ribbon_draws, passes.len())?;
            }
        }
        let particles = source.particles.len();
        let transparent = add(add(meshes, particles)?, source.ribbons.len())?;
        self.visible_draws
            .reserve(budget, Class::Frame, Kind::Result, meshes)?;
        self.transparent_elements
            .reserve(budget, Class::Frame, Kind::Result, transparent)?;
        self.particle_draws
            .reserve(budget, Class::Frame, Kind::Result, particles)?;
        self.particle_vertices
            .reserve(budget, Class::Frame, Kind::Result, vertices)?;
        self.particle_indices
            .reserve(budget, Class::Frame, Kind::Result, indices)?;
        self.particle_sort_indices
            .reserve(budget, Class::Frame, Kind::Scratch, sorting)?;
        self.ribbon_vertices
            .reserve(budget, Class::Frame, Kind::Result, ribbon_vertices)?;
        self.ribbon_draws
            .reserve(budget, Class::Frame, Kind::Result, ribbon_draws)?;
        Ok(())
    }
}
