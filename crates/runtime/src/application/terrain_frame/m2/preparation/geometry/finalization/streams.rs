//! Retained contiguous streams transfer by ownership and retain renderer APIs.

use super::super::super::super::M2Frame;
use super::super::output::GeometryOutput;

/// Scalar palette prefix accompanies the original output containers.
#[derive(Default)]
pub(super) struct FinalStreams {
    pub(super) published_bones: usize,
    pub(super) visible_draws: Vec<solarity_rendering::M2PreparedDraw>,
    pub(super) shadow_draws: Vec<solarity_rendering::M2PreparedDraw>,
    pub(super) environment_shadow_draws: Vec<solarity_rendering::WorldEnvironmentM2Caster>,
    pub(super) particle_draws: Vec<solarity_rendering::M2ParticlePreparedDraw>,
    pub(super) ribbon_draws: Vec<solarity_rendering::M2RibbonPreparedDraw>,
    pub(super) transparent_elements: Vec<super::super::super::super::M2TransparentElement>,
    pub(super) particle_vertices: Vec<solarity_rendering::M2ParticleRenderVertex>,
    pub(super) particle_indices: Vec<u32>,
    pub(super) ribbon_vertices: Vec<solarity_rendering::M2RibbonRenderVertex>,
    pub(super) recoverable_errors: Vec<String>,
}

impl FinalStreams {
    /// Capture and return use the same swap, including partially written failures.
    pub(super) fn exchange(&mut self, frame: &mut M2Frame) {
        std::mem::swap(
            &mut self.published_bones,
            &mut frame.geometry_batch.published_bones,
        );
        std::mem::swap(&mut self.visible_draws, &mut frame.visible_draws);
        std::mem::swap(&mut self.shadow_draws, &mut frame.shadow_draws);
        std::mem::swap(
            &mut self.environment_shadow_draws,
            &mut frame.environment_shadow_draws,
        );
        std::mem::swap(&mut self.particle_draws, &mut frame.particle_draws);
        std::mem::swap(&mut self.ribbon_draws, &mut frame.ribbon_draws);
        std::mem::swap(
            &mut self.transparent_elements,
            &mut frame.transparent_elements,
        );
        std::mem::swap(&mut self.particle_vertices, &mut frame.particle_vertices);
        std::mem::swap(&mut self.particle_indices, &mut frame.particle_indices);
        std::mem::swap(&mut self.ribbon_vertices, &mut frame.ribbon_vertices);
        std::mem::swap(&mut self.recoverable_errors, &mut frame.recoverable_errors);
    }

    /// The assembly writer shares relocation rules with completed model outputs.
    pub(super) fn output(&mut self) -> GeometryOutput<'_> {
        GeometryOutput {
            published_bones: &mut self.published_bones,
            visible_draws: &mut self.visible_draws,
            shadow_draws: &mut self.shadow_draws,
            environment_shadow_draws: &mut self.environment_shadow_draws,
            particle_draws: &mut self.particle_draws,
            ribbon_draws: &mut self.ribbon_draws,
            transparent_elements: &mut self.transparent_elements,
            particle_vertices: &mut self.particle_vertices,
            particle_indices: &mut self.particle_indices,
            ribbon_vertices: &mut self.ribbon_vertices,
            recoverable_errors: &mut self.recoverable_errors,
            vertex_capacity: 0,
            index_capacity: 0,
        }
    }
}
