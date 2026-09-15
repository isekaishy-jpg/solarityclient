//! Independent simulation, mesh, particle and ribbon packets for one admitted model.

use super::super::super::{
    M2EffectOrder, M2ElementAlphaState, M2GpuSource, M2MaterialPose, M2MaterialState,
    M2MaterialUniform, M2ParticleMeshPlan, M2ParticleMeshPlanError, M2ParticlePose,
    M2RibbonMeshPlan, M2RibbonPose, M2TransparentDrawIndex, M2TransparentElement,
    M2TransparentPass, M2TransparentSortKey, RuntimeTerrainFrameError, advance_ribbons,
    particle_emission_density, particle_lod_origin, scene_element_count, section_distance_key,
};
use super::{GeometryInput, GeometryJob};
use glam::Mat4;
use solarity_rendering::{
    M2CameraEffectScale, M2EffectDrawCatalog, M2ParticleTwinkleTable, WorldCameraFrame,
};

impl GeometryJob {
    /// Samples only owned simulation and immutable scene/resource inputs.
    pub(super) fn prepare(
        &mut self,
        source: &M2GpuSource,
        renderer: M2EffectDrawCatalog<'_>,
        camera: WorldCameraFrame,
        effect_scale: M2CameraEffectScale,
        twinkle: &M2ParticleTwinkleTable,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let Some(input) = self.input else {
            unreachable!("only admitted geometry jobs execute");
        };
        let GeometryInput {
            clock,
            effect_delta_seconds,
            instance_color,
            model_view,
            placement_fog_color,
            light_bank,
            scene_index,
            bone_offset,
            effect_retiring,
            particle_liquid,
            model_liquid,
            instance_distance,
            instance_identity,
            ..
        } = input;
        input.trace.link("m2.geometry.execute");
        let _trace = input.trace.enter();
        let mut placement_profile = solarity_profiling::detail_profile!("m2.geometry");
        placement_profile.trace_owner(
            input.placement_index as u64 + 1,
            input.source_index as u64 + 1,
        );
        if self.palette.pending {
            self.pose.recompose_with_overrides(
                source.model.animations(),
                clock,
                model_view,
                self.palette
                    .overrides(&source.model_oriented_billboard_bones),
            )?;
        }
        placement_profile.mark("palette");
        let bone_pose = &self.pose;
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
            self.particle_vertices.reserve(
                self.particle_vertex_capacity
                    .saturating_sub(self.particle_vertices.len()),
            );
            self.particle_indices.reserve(
                self.particle_index_capacity
                    .saturating_sub(self.particle_indices.len()),
            );
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
                    input.particle_colors.as_ref(),
                    &mut self.particle_sort_indices,
                    &mut self.particle_vertices,
                    &mut self.particle_indices,
                )?;
            let effect_order = u32::try_from(self.transparent_elements.len())
                .map_err(|_source| solarity_rendering::VulkanError::M2ParticleDrawIndexRange)?;
            let prepared = renderer
                .prepare_m2_particle_draw_range(
                    if instance_color.w < 0.999_99 {
                        resources.runtime_fade_pipeline
                    } else {
                        resources.pipeline
                    },
                    resources.texture_set,
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
            });
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
                });
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
        placement_profile.mark("particle simulation and geometry");
        advance_ribbons(
            &source.model,
            input.transform,
            &mut self.ribbons,
            bone_pose,
            clock,
            effect_delta_seconds,
            effect_scale,
            instance_color.w,
        )?;
        placement_profile.mark("ribbon simulation");
        if source.mesh.is_some() && !effect_retiring {
            for (draw_index, resources) in source.draws.iter().enumerate() {
                let Some(resources) = resources else {
                    continue;
                };
                let pose = self
                    .material_poses
                    .get(draw_index)
                    .copied()
                    .flatten()
                    .map_or_else(
                        || M2MaterialPose::sample(&source.model, &source.plan, draw_index, clock),
                        Ok,
                    )?;
                let draw = &source.plan.draws()[draw_index];
                let material_state = M2MaterialState::from_material(draw.material());
                let element_alpha = pose.mesh_color().w * instance_color.w;
                let alpha_state = M2ElementAlphaState::classify(element_alpha);
                if alpha_state == M2ElementAlphaState::Hidden {
                    continue;
                }
                let runtime_alpha_fade = alpha_state == M2ElementAlphaState::Translucent
                    && !material_state.blend_enabled();
                let material = M2MaterialUniform::new(
                    input.transform,
                    pose.texture_transforms(),
                    model_view,
                    pose.mesh_color() * instance_color,
                    placement_fog_color.extend(1.0),
                    glam::Vec4::new(
                        material_state.alpha_reference(instance_color.w),
                        material_state.fog_mode().shader_code(),
                        0.0,
                        0.0,
                    ),
                );
                let template = if runtime_alpha_fade {
                    resources.fade_template.ok_or(
                        RuntimeTerrainFrameError::M2RuntimeFadePipeline {
                            model: source.model.path().clone(),
                            draw_index,
                        },
                    )?
                } else {
                    resources.template
                };
                let prepared = template
                    .instantiate(material, bone_offset, 0)?
                    .with_light_bank(light_bank)
                    .with_scene_index(scene_index);
                if draw.transparent_sort_unit() || alpha_state == M2ElementAlphaState::Translucent {
                    let section_distance = section_distance_key(draw, bone_pose, model_view)?;
                    let primary_distance = if input.model_distance_sort {
                        instance_distance
                    } else {
                        section_distance
                    };
                    let producer_order = u32::try_from(self.transparent_elements.len())
                        .map_err(|_source| solarity_rendering::VulkanError::M2DrawIndexRange)?;
                    let key = M2TransparentSortKey::new(
                        primary_distance,
                        false,
                        i16::from(draw.batch().priority_plane),
                        section_distance,
                        instance_identity,
                        draw.batch().material_layer,
                    )
                    .with_scene_element(0, producer_order);
                    for (pass, admitted) in [
                        (M2TransparentPass::One, model_liquid.above()),
                        (M2TransparentPass::Two, model_liquid.below()),
                    ] {
                        if !admitted {
                            continue;
                        }
                        let prepared_index = self.visible_draws.len();
                        self.visible_draws
                            .push(prepared.with_liquid_clip_plane(model_liquid.clip_plane(pass)));
                        self.transparent_elements.push(M2TransparentElement {
                            pass,
                            key,
                            draw: M2TransparentDrawIndex::Mesh(prepared_index),
                        });
                    }
                } else {
                    let scene_order = u32::try_from(scene_element_count(
                        self.visible_draws.len(),
                        self.particle_draws.len(),
                        self.ribbon_draws.len(),
                    )?)
                    .map_err(|_source| solarity_rendering::VulkanError::M2DrawIndexRange)?;
                    self.visible_draws
                        .push(prepared.with_scene_order(scene_order));
                }
            }
        } else {
            debug_assert!(effect_retiring || source.draws.iter().all(Option::is_none));
        }
        placement_profile.mark("material and mesh packets");
        for (ribbon_index, ((emitter, trail), passes)) in source
            .model
            .animations()
            .ribbons()
            .iter()
            .zip(&self.ribbons)
            .zip(&source.ribbons)
            .enumerate()
        {
            // 6F87C0 removes normal model submission; 828A00 retains only
            // the particle pass while model bit 0x400 remains live.
            if effect_retiring {
                continue;
            }
            let Some(root_pass) = passes.first() else {
                continue;
            };
            if trail.sections().len() < 2 {
                continue;
            }
            let first_vertex = u32::try_from(self.ribbon_vertices.len())
                .map_err(|_source| solarity_rendering::VulkanError::M2RibbonDrawVertexRange)?;
            let vertex_count = M2RibbonMeshPlan::append(emitter, trail, &mut self.ribbon_vertices)?;
            let ribbon_alpha = M2RibbonPose::sample(source.model.animations(), emitter, clock)?
                .color()
                .w
                * instance_color.w;
            // 821A20 registers one type-3 scene element for the emitter.
            // Its first material and owner/track alpha classify the whole
            // ribbon; 820F40/980B70 then submit every pass consecutively.
            let effect_order = u32::try_from(self.transparent_elements.len())
                .map_err(|_source| solarity_rendering::VulkanError::M2RibbonDrawVertexRange)?;
            let first_draw = self.ribbon_draws.len();
            let opaque = !M2MaterialState::from_material(root_pass.material).blend_enabled()
                && M2ElementAlphaState::classify(ribbon_alpha) == M2ElementAlphaState::Authored;
            let order = scene_element_count(
                self.visible_draws.len(),
                self.particle_draws.len(),
                first_draw,
            )?;
            for (pass_index, pass) in passes.iter().enumerate() {
                let prepared = renderer
                    .prepare_m2_ribbon_draw_range(
                        pass.pipeline,
                        pass.texture_set,
                        pass.material,
                        M2EffectOrder::new(emitter.priority_plane(), effect_order),
                        first_vertex,
                        vertex_count,
                    )?
                    .with_light_bank(light_bank)
                    .with_scene_index(scene_index)
                    .with_first_material_pass(pass_index == 0);
                self.ribbon_draws.push(if opaque {
                    prepared.with_scene_order(
                        u32::try_from(order)
                            .map_err(|_| solarity_rendering::VulkanError::M2DrawIndexRange)?,
                    )
                } else {
                    prepared
                });
            }
            if !opaque {
                self.transparent_elements.push(M2TransparentElement {
                    pass: model_liquid.ribbon_pass(),
                    key: M2TransparentSortKey::new(
                        instance_distance,
                        false,
                        emitter.priority_plane(),
                        instance_distance,
                        instance_identity,
                        0,
                    )
                    .with_scene_element(3, effect_order),
                    draw: M2TransparentDrawIndex::Ribbon {
                        first: first_draw,
                        count: passes.len(),
                    },
                });
            }
            tracing::trace!(
                model = %source.model.path(),
                ribbon_index,
                vertex_count,
                pass_count = passes.len(),
                "placement-local ribbon entered unified world frame"
            );
        }
        Ok(())
    }
}
