//! Visible particle simulation and packet construction.

use super::super::super::RuntimeTerrainFrameError;
use super::super::super::{
    M2EffectOrder, M2ElementAlphaState, M2MaterialState, M2ParticleMeshPlan,
    M2ParticleMeshPlanError, M2ParticlePose, M2TransparentDrawIndex, M2TransparentElement,
    M2TransparentPass, M2TransparentSortKey, particle_emission_density, particle_lod_origin,
    scene_element_count,
};
use super::{GeometryContext, GeometryInput, GeometryJob, VisibleGeometryInput};
use glam::Mat4;

impl GeometryJob {
    /// Consumes only this job's owned state and immutable frame inputs.
    pub(super) fn prepare_particles(
        &mut self,
        context: &GeometryContext,
        input: GeometryInput,
        visible: VisibleGeometryInput,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let source = &context.source;
        let camera = context.camera;
        let effect_scale = context.effect_scale;
        let twinkle = &context.twinkle;
        let clock = input.clock;
        let bone_pose = &self.pose;
        let VisibleGeometryInput {
            effect_delta_seconds,
            instance_color,
            light_bank,
            scene_index,
            effect_retiring,
            particle_liquid,
            instance_distance,
            instance_identity,
            ..
        } = visible;
        if self.particles.len() != source.model.animations().particles().len() {
            return Err(RuntimeTerrainFrameError::M2ParticleSimulationCount {
                model: source.model.path().clone(),
                simulation_count: self.particles.len(),
                emitter_count: source.model.animations().particles().len(),
            });
        }
        if source.particles.len() != source.model.animations().particles().len() {
            return Err(RuntimeTerrainFrameError::M2ParticleResourceCount {
                model: source.model.path().clone(),
                resource_count: source.particles.len(),
                emitter_count: source.model.animations().particles().len(),
            });
        }
        for (particle_index, ((emitter, placement_particle), resources)) in source
            .model
            .animations()
            .particles()
            .iter()
            .zip(&mut self.particles)
            .zip(&source.particles)
            .enumerate()
        {
            if placement_particle.unsupported.is_some() {
                if let Some(message) =
                    placement_particle.diagnostic(source.model.path(), particle_index)
                {
                    self.recoverable_errors.push(message);
                }
                continue;
            }
            let simulation = &mut placement_particle.simulation;
            if effect_retiring {
                simulation.set_emission_enabled(false);
            }
            let pose = M2ParticlePose::sample(source.model.animations(), emitter, clock)?;
            let emitter_transform =
                bone_pose.particle_emitter_transform(emitter, input.transform)?;
            let model_lod_position = particle_lod_origin(input.transform);
            let particle_density = particle_emission_density(
                emitter.flags(),
                model_lod_position,
                camera.camera().position(),
            );
            match emitter.emitter_type() {
                1 => simulation.advance_planar_bounded(
                    emitter,
                    pose,
                    effect_delta_seconds,
                    emitter_transform,
                    particle_density,
                ),
                2 => simulation.advance_sphere_bounded(
                    emitter,
                    pose,
                    effect_delta_seconds,
                    emitter_transform,
                    particle_density,
                ),
                emitter_type => {
                    return Err(RuntimeTerrainFrameError::M2ParticleEmitterType {
                        model: source.model.path().clone(),
                        particle_index,
                        emitter_type,
                    });
                }
            }
            .map_err(|source_error| {
                RuntimeTerrainFrameError::M2PlacedParticleSimulation {
                    model: source.model.path().clone(),
                    particle_index,
                    time_ms: clock.animation_time_ms(),
                    source: source_error,
                }
            })?;
            let (emitter_vertex_capacity, emitter_index_capacity) =
                M2ParticleMeshPlan::buffer_capacity(emitter, simulation.capacity())?;
            self.particle_vertex_capacity = self
                .particle_vertex_capacity
                .checked_add(emitter_vertex_capacity)
                .ok_or(M2ParticleMeshPlanError::VertexCount)?;
            self.particle_index_capacity = self
                .particle_index_capacity
                .checked_add(emitter_index_capacity)
                .ok_or(M2ParticleMeshPlanError::IndexCount)?;
            if simulation.particles().is_empty() {
                continue;
            }
            let particle_to_world = if emitter.particles_in_model_space() {
                emitter_transform
            } else {
                Mat4::IDENTITY
            };
            // Build 12340 `0x0097AC20` retains the complete view-model and
            // animated-emitter axis length. Its later flag-`0x20` card path
            // consequently includes native M2 camera aspect correction.
            let inherited_scale =
                emitter_transform.x_axis.truncate().length() * effect_scale.factor();
            let first_vertex = u32::try_from(self.particle_vertices.len())
                .map_err(|_source| solarity_rendering::VulkanError::M2ParticleDrawVertexRange)?;
            let first_index = u32::try_from(self.particle_indices.len())
                .map_err(|_source| solarity_rendering::VulkanError::M2ParticleDrawIndexRange)?;
            let (vertex_count, index_count) =
                M2ParticleMeshPlan::append_transformed_with_particle_color(
                    emitter,
                    pose,
                    simulation.particles(),
                    camera,
                    particle_to_world,
                    inherited_scale,
                    instance_color.w,
                    twinkle,
                    visible.particle_colors.as_ref(),
                    &mut self.particle_sort_indices.writer(),
                    &mut self.particle_vertices.writer(),
                    &mut self.particle_indices.writer(),
                )?;
            let effect_order = u32::try_from(self.transparent_elements.len())
                .map_err(|_source| solarity_rendering::VulkanError::M2ParticleDrawIndexRange)?;
            let template = if instance_color.w < 0.999_99 {
                resources.fade_template
            } else {
                resources.template
            };
            let prepared = template
                .prepare(
                    emitter.blending_type(),
                    emitter.flags(),
                    instance_color.w,
                    M2EffectOrder::new(emitter.priority_plane(), effect_order),
                    first_vertex,
                    first_index,
                    vertex_count,
                    index_count,
                )?
                .with_light_bank(light_bank)
                .with_scene_index(scene_index);
            let prepared_index = self.particle_draws.len();
            let opaque = !M2MaterialState::from_particle(emitter.blending_type(), emitter.flags())
                .blend_enabled()
                && M2ElementAlphaState::classify(instance_color.w) == M2ElementAlphaState::Authored;
            let order = scene_element_count(
                self.visible_draws.len(),
                self.particle_draws.len(),
                self.ribbon_draws.len(),
            )?;
            self.particle_draws.push(if opaque {
                prepared.with_scene_order(
                    u32::try_from(order)
                        .map_err(|_| solarity_rendering::VulkanError::M2DrawIndexRange)?,
                )
            } else {
                prepared
            })?;
            if !opaque {
                self.transparent_elements.push(M2TransparentElement {
                    pass: M2TransparentPass::for_particle_liquid(
                        emitter.flags(),
                        particle_liquid.above(),
                    ),
                    key: M2TransparentSortKey::new(
                        instance_distance,
                        false,
                        emitter.priority_plane(),
                        instance_distance,
                        instance_identity,
                        0,
                    )
                    .with_scene_element(4, effect_order),
                    draw: M2TransparentDrawIndex::Particle(prepared_index),
                })?;
            }
            tracing::trace!(
                model = %source.model.path(),
                particle_index,
                live_count = simulation.particles().len(),
                vertex_count,
                index_count,
                "placement-local particles entered unified world frame"
            );
        }
        Ok(())
    }
}
