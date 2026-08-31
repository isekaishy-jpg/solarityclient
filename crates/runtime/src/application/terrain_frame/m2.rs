//! Renderer-local resources for the shared resident placed-M2 scene.

use std::sync::Arc;

use glam::Mat4;
use solarity_asset::{DecodedM2Model, M2ParticleEmitter};
use solarity_rendering::{
    BlpColorSpace, BlpTextureUploadRequest, M2AnimationClock, M2BonePose, M2DrawCall,
    M2LocalLightCount, M2MaterialPose, M2MaterialState, M2MaterialUniform, M2MeshHandle,
    M2MeshPlan, M2ParticleMeshPlan, M2ParticlePipelineHandle, M2ParticlePose,
    M2ParticlePreparedDraw, M2ParticleRenderVertex, M2ParticleSimulation, M2PipelineHandle,
    M2PreparedDraw, M2RibbonControlPoint, M2RibbonMeshPlan, M2RibbonPipelineHandle, M2RibbonPose,
    M2RibbonPreparedDraw, M2RibbonRenderVertex, M2RibbonTrail, M2SampledTexture,
    M2ShaderPermutation, M2ShaderPlan, M2ShadowFiltering, M2ShadowPermutation, M2TextureSet,
    M2TextureSetHandle, M2TransparentSortKey, VulkanRenderer, WorldCameraFrame, WorldFrustum,
    compare_m2_transparent, m2_section_distance_key,
};

use crate::application::terrain_coordinator::m2_residency::{
    ResidentM2Owner, ResidentM2Scene, ResidentM2Source, ResidentM2Texture,
};
use crate::random::CrtRand;

use super::RuntimeTerrainFrameError;

/// Build 12340's highest-capability external SKIN selection.
///
/// The stock client chooses one `%02d.skin` companion when the shared model is
/// loaded. Its world-distance policy culls and fades whole placements; it does
/// not swap geometry profiles per placement. Vulkan 1.3 exceeds the original
/// hardware capability gate, so this renderer selects the authored `00.skin`.
const STOCK_HIGH_CAPABILITY_PROFILE: usize = 0;

/// Runtime opacity boundary stored at build-12340 address `0x00A45528`.
const STOCK_OPAQUE_ALPHA_THRESHOLD: f32 = 0.999_99;

/// Initial `particleDensity` CVar registered by the stock UI environment.
const STOCK_DEFAULT_PARTICLE_DENSITY: f32 = 1.0;

/// One selected M2/SKIN generation uploaded once for all of its placements.
struct M2GpuSource {
    model: Arc<DecodedM2Model>,
    plan: Arc<M2MeshPlan>,
    mesh: M2MeshHandle,
    draws: Vec<M2GpuDraw>,
    particles: Vec<M2GpuParticle>,
    ribbons: Vec<Vec<M2GpuRibbonPass>>,
}

/// Fixed renderer objects paired with one exact SKIN material batch.
struct M2GpuDraw {
    pipeline: M2PipelineHandle,
    runtime_fade_pipeline: Option<M2PipelineHandle>,
    texture_set: M2TextureSetHandle,
}

/// Shared renderer objects for one ordinary particle declaration.
struct M2GpuParticle {
    pipeline: M2ParticlePipelineHandle,
    texture_set: M2TextureSetHandle,
}

/// One stock ribbon pass pairing parallel material and texture entries.
struct M2GpuRibbonPass {
    pipeline: M2RibbonPipelineHandle,
    texture_set: M2TextureSetHandle,
    material: solarity_asset::M2Material,
}

/// One pass-one mesh packet retained until the shared comparator runs.
struct M2TransparentDraw {
    key: M2TransparentSortKey,
    draw: M2PreparedDraw,
}

/// Exact per-instance state required by later animation and material assembly.
struct M2GpuPlacement {
    source_index: usize,
    transform: Mat4,
    owner: ResidentM2Owner,
    flags: u16,
    color: [u8; 4],
    playback: Option<M2Playback>,
    particles: Vec<M2ParticleSimulation>,
    ribbons: Vec<M2RibbonTrail>,
}

/// Per-instance sequence state retained by stock's `CM2Model` owner.
struct M2Playback {
    sequence: usize,
    sequence_duration_ms: f32,
    cycle_count: u32,
    cycle_started_ms: f32,
    has_variations: bool,
}

impl M2Playback {
    /// Selects Stand variation zero and consumes its authored cycle-count roll.
    fn new(
        model: &DecodedM2Model,
        random: &mut CrtRand,
    ) -> Result<Option<Self>, RuntimeTerrainFrameError> {
        let animations = model.animations();
        if animations.sequences().is_empty() {
            return Ok(Some(Self {
                sequence: 0,
                sequence_duration_ms: 0.0,
                cycle_count: 1,
                cycle_started_ms: 0.0,
                has_variations: false,
            }));
        }
        let sequence = animations
            .sequence_for_variation(0, 0)
            .or_else(|| animations.select_sequence(0, None, u32::from(random.next_u15())));
        let Some(sequence) = sequence else {
            tracing::debug!(
                path = %model.path(),
                "placed world M2 omitted because animation ID zero is unavailable"
            );
            return Ok(None);
        };
        let sequence_duration_ms = resolved_sequence_duration(model, sequence)?;
        let cycle_count = animations.sequences()[sequence].cycle_count(random.next_u15());
        let variation_count = animations.available_variation_count(0).ok_or_else(|| {
            RuntimeTerrainFrameError::M2AnimationSelection {
                model: model.path().clone(),
                animation_id: 0,
            }
        })?;
        Ok(Some(Self {
            sequence,
            sequence_duration_ms,
            cycle_count,
            cycle_started_ms: 0.0,
            has_variations: variation_count > 1,
        }))
    }

    /// Advances one expired stock timer and returns the selected sequence clock.
    fn clock(
        &mut self,
        model: &DecodedM2Model,
        animation_time_ms: f32,
        global_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<M2AnimationClock, RuntimeTerrainFrameError> {
        let elapsed_ms = (animation_time_ms - self.cycle_started_ms).max(0.0);
        let selected_span_ms = self.sequence_duration_ms * self.cycle_count as f32;
        if self.has_variations && self.sequence_duration_ms > 0.0 && elapsed_ms >= selected_span_ms
        {
            let animation_id = model.animations().sequences()[self.sequence].animation_id();
            self.sequence = model
                .animations()
                .select_sequence(animation_id, None, u32::from(random.next_u15()))
                .ok_or_else(|| RuntimeTerrainFrameError::M2AnimationSelection {
                    model: model.path().clone(),
                    animation_id,
                })?;
            self.sequence_duration_ms = resolved_sequence_duration(model, self.sequence)?;
            self.cycle_count =
                model.animations().sequences()[self.sequence].cycle_count(random.next_u15());
            // Stock builds the replacement bone-sequence timer from the
            // current client tick, so a stalled presentation does not replay
            // an unbounded backlog of expired fidget variations.
            self.cycle_started_ms = animation_time_ms;
        }
        Ok(world_animation_clock(
            self.sequence,
            self.sequence_duration_ms,
            animation_time_ms - self.cycle_started_ms,
            global_time_ms,
        ))
    }
}

/// All resident M2 geometry and transforms owned by one terrain generation.
pub(super) struct M2Frame {
    sources: Vec<Option<M2GpuSource>>,
    placements: Vec<M2GpuPlacement>,
    animation_started_at: std::time::Instant,
    bone_transforms: Vec<Mat4>,
    visible_draws: Vec<M2PreparedDraw>,
    transparent_draws: Vec<M2TransparentDraw>,
    particle_vertices: Vec<M2ParticleRenderVertex>,
    particle_indices: Vec<u32>,
    particle_draws: Vec<M2ParticlePreparedDraw>,
    ribbon_vertices: Vec<M2RibbonRenderVertex>,
    ribbon_draws: Vec<M2RibbonPreparedDraw>,
    last_effect_time_ms: Option<f32>,
}

/// Borrowed dynamic streams assembled for one unified world submission.
pub(super) struct M2VisibleFrame<'frame> {
    pub(super) bone_transforms: &'frame [Mat4],
    pub(super) draws: &'frame [M2PreparedDraw],
    pub(super) particle_vertices: &'frame [M2ParticleRenderVertex],
    pub(super) particle_indices: &'frame [u32],
    pub(super) particle_draws: &'frame [M2ParticlePreparedDraw],
    pub(super) ribbon_vertices: &'frame [M2RibbonRenderVertex],
    pub(super) ribbon_draws: &'frame [M2RibbonPreparedDraw],
}

impl M2Frame {
    /// Publishes profile zero and its fully resolved static material resources.
    pub(super) fn prepare(
        renderer: &mut VulkanRenderer,
        scene: &ResidentM2Scene,
        random: &mut CrtRand,
    ) -> Result<Self, RuntimeTerrainFrameError> {
        let mut sources = Vec::with_capacity(scene.sources().len());
        for source in scene.sources() {
            sources.push(prepare_source(renderer, source)?);
        }

        let mut placements = Vec::with_capacity(scene.placements().len());
        for placement in scene.placements() {
            if placement.source_index() >= sources.len() {
                return Err(RuntimeTerrainFrameError::M2SourceIndex {
                    source_index: placement.source_index(),
                    source_count: sources.len(),
                });
            }
            let (playback, particles, ribbons) = match sources[placement.source_index()].as_ref() {
                Some(source) => {
                    let playback = M2Playback::new(&source.model, random)?;
                    let particles = source
                        .model
                        .animations()
                        .particles()
                        .iter()
                        .map(|_emitter| {
                            let first = u32::from(random.next_u15());
                            let second = u32::from(random.next_u15());
                            M2ParticleSimulation::new(first << 16 | second)
                        })
                        .collect();
                    let ribbons = source
                        .model
                        .animations()
                        .ribbons()
                        .iter()
                        .map(M2RibbonTrail::new)
                        .collect::<Result<Vec<_>, _>>()?;
                    (playback, particles, ribbons)
                }
                None => (None, Vec::new(), Vec::new()),
            };
            placements.push(M2GpuPlacement {
                source_index: placement.source_index(),
                transform: placement.transform(),
                owner: placement.owner(),
                flags: placement.flags(),
                color: placement.color(),
                playback,
                particles,
                ribbons,
            });
        }
        Ok(Self {
            sources,
            placements,
            animation_started_at: std::time::Instant::now(),
            bone_transforms: Vec::new(),
            visible_draws: Vec::new(),
            transparent_draws: Vec::new(),
            particle_vertices: Vec::new(),
            particle_indices: Vec::new(),
            particle_draws: Vec::new(),
            ribbon_vertices: Vec::new(),
            ribbon_draws: Vec::new(),
            last_effect_time_ms: None,
        })
    }

    /// Returns the number of selected shared M2 GPU generations.
    pub(super) fn mesh_count(&self) -> usize {
        for source in self.sources.iter().flatten() {
            debug_assert_eq!(source.plan.profile_index(), STOCK_HIGH_CAPABILITY_PROFILE);
            tracing::trace!(
                path = %source.plan.path(),
                mesh = ?source.mesh,
                profile_index = source.plan.profile_index(),
                material_count = source.draws.len(),
                decoded_path = %source.model.path(),
                "shared M2 entered renderer generation"
            );
            for draw in &source.draws {
                tracing::trace!(
                    pipeline = ?draw.pipeline,
                    texture_set = ?draw.texture_set,
                    "M2 material resources entered renderer generation"
                );
            }
        }
        self.sources.iter().flatten().count()
    }

    /// Returns independently transformed MDDF and MODD instance count.
    pub(super) fn placement_count(&self) -> usize {
        for placement in &self.placements {
            debug_assert!(placement.source_index < self.sources.len());
            debug_assert!(placement.transform.is_finite());
            tracing::trace!(
                owner = ?placement.owner,
                flags = placement.flags,
                color = ?placement.color,
                "placed M2 retained by renderer generation"
            );
        }
        self.placements.len()
    }

    /// Returns elapsed time on this resident generation's local animation clock.
    pub(super) fn animation_time_ms(&self) -> f32 {
        self.animation_started_at.elapsed().as_secs_f32() * 1_000.0
    }

    /// Culls placements and builds their bone/material draw packets in place.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_visible_draws(
        &mut self,
        renderer: &VulkanRenderer,
        frustum: WorldFrustum,
        camera: WorldCameraFrame,
        fog_color: glam::Vec3,
        animation_time_ms: f32,
        global_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<M2VisibleFrame<'_>, RuntimeTerrainFrameError> {
        self.bone_transforms.clear();
        self.visible_draws.clear();
        self.transparent_draws.clear();
        self.particle_vertices.clear();
        self.particle_indices.clear();
        self.particle_draws.clear();
        self.ribbon_vertices.clear();
        self.ribbon_draws.clear();
        let effect_delta_seconds = self.last_effect_time_ms.map_or(0.0, |previous| {
            (animation_time_ms - previous).max(0.0) * 0.001
        });
        self.last_effect_time_ms = Some(animation_time_ms);
        for placement in &mut self.placements {
            let Some(source) = self.sources[placement.source_index].as_ref() else {
                continue;
            };
            let Some(playback) = placement.playback.as_mut() else {
                continue;
            };
            let clock = playback.clock(&source.model, animation_time_ms, global_time_ms, random)?;
            let bounds = source.model.bounds();
            let center = placement
                .transform
                .transform_point3((bounds.minimum() + bounds.maximum()) * 0.5);
            let maximum_scale = placement.transform.x_axis.truncate().length().max(
                placement
                    .transform
                    .y_axis
                    .truncate()
                    .length()
                    .max(placement.transform.z_axis.truncate().length()),
            );
            if !frustum.contains_sphere(center, bounds.sphere_radius() * maximum_scale)? {
                continue;
            }

            let bone_offset = u32::try_from(self.bone_transforms.len())
                .map_err(|_source| solarity_rendering::VulkanError::M2BoneTransformRange)?;
            let model_view = camera.view() * placement.transform;
            let bone_pose =
                M2BonePose::compose_with_model_view(source.model.animations(), clock, model_view)?;
            if placement.particles.len() != source.model.animations().particles().len() {
                return Err(RuntimeTerrainFrameError::M2ParticleSimulationCount {
                    model: source.model.path().clone(),
                    simulation_count: placement.particles.len(),
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
            for (particle_index, ((emitter, simulation), resources)) in source
                .model
                .animations()
                .particles()
                .iter()
                .zip(&mut placement.particles)
                .zip(&source.particles)
                .enumerate()
            {
                let pose = M2ParticlePose::sample(source.model.animations(), emitter, clock)?;
                let emitter_transform = particle_emitter_transform(
                    &source.model,
                    placement.transform,
                    &bone_pose,
                    particle_index,
                    emitter,
                )?;
                match emitter.emitter_type() {
                    1 => simulation.advance_planar(
                        emitter,
                        pose,
                        effect_delta_seconds,
                        emitter_transform,
                        STOCK_DEFAULT_PARTICLE_DENSITY,
                    )?,
                    2 => simulation.advance_sphere(
                        emitter,
                        pose,
                        effect_delta_seconds,
                        emitter_transform,
                        STOCK_DEFAULT_PARTICLE_DENSITY,
                    )?,
                    emitter_type => {
                        return Err(RuntimeTerrainFrameError::M2ParticleEmitterType {
                            model: source.model.path().clone(),
                            particle_index,
                            emitter_type,
                        });
                    }
                };
                if simulation.particles().is_empty() {
                    continue;
                }
                let particle_to_world = if emitter.particles_in_model_space() {
                    emitter_transform
                } else {
                    Mat4::IDENTITY
                };
                let mesh = M2ParticleMeshPlan::prepare_transformed(
                    emitter,
                    pose,
                    simulation.particles(),
                    camera,
                    particle_to_world,
                    placement_color(placement.color).w,
                )?;
                let first_vertex =
                    u32::try_from(self.particle_vertices.len()).map_err(|_source| {
                        solarity_rendering::VulkanError::M2ParticleDrawVertexRange
                    })?;
                let first_index = u32::try_from(self.particle_indices.len())
                    .map_err(|_source| solarity_rendering::VulkanError::M2ParticleDrawIndexRange)?;
                self.particle_draws.push(renderer.prepare_m2_particle_draw(
                    resources.pipeline,
                    resources.texture_set,
                    emitter.blending_type(),
                    emitter.flags(),
                    first_vertex,
                    first_index,
                    &mesh,
                )?);
                self.particle_vertices.extend_from_slice(mesh.vertices());
                self.particle_indices.extend_from_slice(mesh.indices());
                tracing::trace!(
                    model = %source.model.path(),
                    particle_index,
                    live_count = simulation.particles().len(),
                    vertex_count = mesh.vertices().len(),
                    index_count = mesh.indices().len(),
                    "placement-local particles entered unified world frame"
                );
            }
            advance_ribbons(
                &source.model,
                placement,
                &bone_pose,
                clock,
                effect_delta_seconds,
            )?;
            self.bone_transforms
                .extend_from_slice(bone_pose.transforms());
            let instance_color = placement_color(placement.color);
            let instance_identity = std::ptr::from_ref(&*placement).addr();
            for (draw_index, resources) in source.draws.iter().enumerate() {
                let pose = M2MaterialPose::sample(&source.model, &source.plan, draw_index, clock)?;
                let draw = &source.plan.draws()[draw_index];
                let material_state = M2MaterialState::from_material(draw.material());
                let element_alpha = pose.mesh_color().w * instance_color.w;
                let runtime_alpha_fade =
                    element_alpha < STOCK_OPAQUE_ALPHA_THRESHOLD && !material_state.blend_enabled();
                let material = M2MaterialUniform::new(
                    placement.transform,
                    pose.texture_transforms(),
                    model_view,
                    pose.mesh_color() * instance_color,
                    fog_color.extend(1.0),
                    glam::Vec4::new(
                        material_state.alpha_reference(instance_color.w),
                        0.0,
                        0.0,
                        0.0,
                    ),
                );
                let pipeline = if runtime_alpha_fade {
                    resources.runtime_fade_pipeline.ok_or(
                        RuntimeTerrainFrameError::M2RuntimeFadePipeline {
                            model: source.model.path().clone(),
                            draw_index,
                        },
                    )?
                } else {
                    resources.pipeline
                };
                let prepared = renderer.prepare_m2_draw(
                    source.mesh,
                    pipeline,
                    resources.texture_set,
                    &source.plan,
                    draw_index,
                    runtime_alpha_fade,
                    material,
                    bone_offset,
                    0,
                )?;
                if draw.transparent_sort_unit() || element_alpha < STOCK_OPAQUE_ALPHA_THRESHOLD {
                    let distance = section_distance_key(draw, &bone_pose, model_view)?;
                    self.transparent_draws.push(M2TransparentDraw {
                        key: M2TransparentSortKey::new(
                            distance,
                            false,
                            draw.batch().priority_plane,
                            distance,
                            instance_identity,
                            draw.batch().material_layer,
                        ),
                        draw: prepared,
                    });
                } else {
                    self.visible_draws.push(prepared);
                }
            }
            for (ribbon_index, ((emitter, trail), passes)) in source
                .model
                .animations()
                .ribbons()
                .iter()
                .zip(&placement.ribbons)
                .zip(&source.ribbons)
                .enumerate()
            {
                if passes.is_empty() {
                    continue;
                }
                let mesh = M2RibbonMeshPlan::prepare(emitter, trail)?;
                if mesh.vertices().len() < 4 {
                    continue;
                }
                let first_vertex = u32::try_from(self.ribbon_vertices.len())
                    .map_err(|_source| solarity_rendering::VulkanError::M2RibbonDrawVertexRange)?;
                for pass in passes {
                    self.ribbon_draws.push(renderer.prepare_m2_ribbon_draw(
                        pass.pipeline,
                        pass.texture_set,
                        pass.material,
                        first_vertex,
                        &mesh,
                    )?);
                }
                self.ribbon_vertices.extend_from_slice(mesh.vertices());
                tracing::trace!(
                    model = %source.model.path(),
                    ribbon_index,
                    vertex_count = mesh.vertices().len(),
                    pass_count = passes.len(),
                    "placement-local ribbon entered unified world frame"
                );
            }
        }
        self.transparent_draws
            .sort_unstable_by(|left, right| compare_m2_transparent(&left.key, &right.key));
        self.visible_draws
            .extend(self.transparent_draws.iter().map(|queued| queued.draw));
        Ok(M2VisibleFrame {
            bone_transforms: &self.bone_transforms,
            draws: &self.visible_draws,
            particle_vertices: &self.particle_vertices,
            particle_indices: &self.particle_indices,
            particle_draws: &self.particle_draws,
            ribbon_vertices: &self.ribbon_vertices,
            ribbon_draws: &self.ribbon_draws,
        })
    }
}

/// Resolves one emitter's current bone-relative local-to-world matrix.
fn particle_emitter_transform(
    model: &DecodedM2Model,
    placement_transform: Mat4,
    bone_pose: &M2BonePose,
    particle_index: usize,
    emitter: &M2ParticleEmitter,
) -> Result<Mat4, RuntimeTerrainFrameError> {
    let bone = match emitter.bone_index() {
        Some(bone_index) => bone_pose
            .transforms()
            .get(usize::from(bone_index))
            .copied()
            .ok_or_else(|| RuntimeTerrainFrameError::M2ParticleBoneIndex {
                model: model.path().clone(),
                particle_index,
                bone_index: u32::from(bone_index),
            })?,
        None => Mat4::IDENTITY,
    };
    Ok(placement_transform * bone * Mat4::from_translation(emitter.position()))
}

/// Advances every shared declaration through its placement-owned edge history.
fn advance_ribbons(
    model: &DecodedM2Model,
    placement: &mut M2GpuPlacement,
    bone_pose: &M2BonePose,
    clock: M2AnimationClock,
    delta_seconds: f32,
) -> Result<(), RuntimeTerrainFrameError> {
    let emitters = model.animations().ribbons();
    if placement.ribbons.len() != emitters.len() {
        return Err(RuntimeTerrainFrameError::M2RibbonTrailCount {
            model: model.path().clone(),
            trail_count: placement.ribbons.len(),
            emitter_count: emitters.len(),
        });
    }
    for (ribbon_index, (emitter, trail)) in emitters.iter().zip(&mut placement.ribbons).enumerate()
    {
        let bone = match emitter.bone_index() {
            Some(bone_index) => bone_pose
                .transforms()
                .get(bone_index as usize)
                .copied()
                .ok_or_else(|| RuntimeTerrainFrameError::M2RibbonBoneIndex {
                    model: model.path().clone(),
                    ribbon_index,
                    bone_index,
                })?,
            None => Mat4::IDENTITY,
        };
        // Stock appends the emitter-local translation to the animated bone,
        // then composes the placement. Its column-major matrix passes Y as the
        // strip width axis and Z as the interpolation tangent.
        let transform = placement.transform * bone * Mat4::from_translation(emitter.position());
        let control = M2RibbonControlPoint::new(
            transform.w_axis.truncate(),
            transform.y_axis.truncate(),
            transform.z_axis.truncate(),
        );
        let pose = M2RibbonPose::sample(model.animations(), emitter, clock)?;
        trail.advance(delta_seconds, control, pose)?;
    }
    Ok(())
}

/// Computes stock's animated section-center key, including SKIN radius flags.
fn section_distance_key(
    draw: &M2DrawCall,
    bone_pose: &M2BonePose,
    model_view: Mat4,
) -> Result<f32, RuntimeTerrainFrameError> {
    let bone_index = usize::from(draw.center_bone_index());
    let bone_transform = match bone_pose.transforms().get(bone_index).copied() {
        Some(transform) => transform,
        None if bone_index == 0 && bone_pose.transforms().is_empty() => Mat4::IDENTITY,
        None => return Err(solarity_rendering::VulkanError::M2BoneTransformRange.into()),
    };
    let view_transform = model_view * bone_transform;
    Ok(m2_section_distance_key(
        draw.sort_center(),
        draw.sort_radius(),
        draw.batch().flags,
        view_transform,
    ))
}

/// Resolves the immutable duration owned by an alias target.
fn resolved_sequence_duration(
    model: &DecodedM2Model,
    sequence: usize,
) -> Result<f32, RuntimeTerrainFrameError> {
    let resolved = model
        .animations()
        .resolve_sequence_alias(sequence)
        .ok_or_else(|| RuntimeTerrainFrameError::M2SequenceIndex {
            model: model.path().clone(),
            sequence,
        })?;
    Ok(model.animations().sequences()[resolved].duration_ms() as f32)
}

/// Advances the sequence selected for one placed model instance.
fn world_animation_clock(
    sequence: usize,
    duration_ms: f32,
    animation_time_ms: f32,
    global_time_ms: f32,
) -> M2AnimationClock {
    let animation_time_ms = if duration_ms > 0.0 {
        animation_time_ms.rem_euclid(duration_ms)
    } else {
        0.0
    };
    M2AnimationClock::new(sequence, animation_time_ms, global_time_ms)
}

/// Converts MODD's BGRA bytes to shader RGBA; MDDF already stores white.
fn placement_color(color: [u8; 4]) -> glam::Vec4 {
    const BYTE_TO_UNIT: f32 = 1.0 / 255.0;
    glam::Vec4::new(
        f32::from(color[2]) * BYTE_TO_UNIT,
        f32::from(color[1]) * BYTE_TO_UNIT,
        f32::from(color[0]) * BYTE_TO_UNIT,
        f32::from(color[3]) * BYTE_TO_UNIT,
    )
}

/// Publishes a source only when every selected draw has concrete BLP stages.
fn prepare_source(
    renderer: &mut VulkanRenderer,
    source: &ResidentM2Source,
) -> Result<Option<M2GpuSource>, RuntimeTerrainFrameError> {
    let model = source.model();
    if source.textures().len() != model.textures().len() {
        return Err(RuntimeTerrainFrameError::M2TextureTableCount {
            model: model.path().clone(),
            source_count: source.textures().len(),
            model_count: model.textures().len(),
        });
    }
    let plan = Arc::new(M2MeshPlan::prepare(model, STOCK_HIGH_CAPABILITY_PROFILE)?);
    for draw in plan.draws() {
        for binding in draw.texture_bindings() {
            let texture = source
                .textures()
                .get(usize::from(binding.texture_index()))
                .ok_or_else(|| RuntimeTerrainFrameError::M2TextureIndex {
                    model: model.path().clone(),
                    texture_index: binding.texture_index(),
                })?;
            if let ResidentM2Texture::Replaceable(kind) = texture {
                tracing::debug!(
                    path = %model.path(),
                    ?kind,
                    "static world M2 omitted until its replacement texture is supplied"
                );
                return Ok(None);
            }
        }
    }
    for emitter in model.animations().ribbons() {
        for texture_index in emitter.texture_indices() {
            let texture = source
                .textures()
                .get(usize::from(*texture_index))
                .ok_or_else(|| RuntimeTerrainFrameError::M2TextureIndex {
                    model: model.path().clone(),
                    texture_index: *texture_index,
                })?;
            if let ResidentM2Texture::Replaceable(kind) = texture {
                tracing::debug!(
                    path = %model.path(),
                    ?kind,
                    "static world M2 omitted until its ribbon replacement texture is supplied"
                );
                return Ok(None);
            }
        }
    }
    for (particle_index, emitter) in model.animations().particles().iter().enumerate() {
        let texture_index = ordinary_particle_texture_index(model, particle_index, emitter)?;
        let texture = source
            .textures()
            .get(usize::from(texture_index))
            .ok_or_else(|| RuntimeTerrainFrameError::M2TextureIndex {
                model: model.path().clone(),
                texture_index,
            })?;
        if let ResidentM2Texture::Replaceable(kind) = texture {
            tracing::debug!(
                path = %model.path(),
                particle_index,
                ?kind,
                "static world M2 omitted until its particle replacement texture is supplied"
            );
            return Ok(None);
        }
    }

    let mut upload_indices = Vec::new();
    let mut uploads = Vec::new();
    for (texture_index, texture) in source.textures().iter().enumerate() {
        if let ResidentM2Texture::Authored(source_texture) = texture {
            upload_indices.push(texture_index);
            uploads.push(BlpTextureUploadRequest::new(
                source_texture,
                BlpColorSpace::Srgb,
            ));
        }
    }
    let uploaded = renderer.upload_blp_textures(&uploads)?;
    let mut texture_handles = vec![None; source.textures().len()];
    for (texture_index, handle) in upload_indices.into_iter().zip(uploaded) {
        texture_handles[texture_index] = Some(handle);
    }

    let mesh = renderer.upload_m2_mesh(&plan)?;
    let mut texture_requests = Vec::with_capacity(plan.draws().len());
    let mut pipelines = Vec::with_capacity(plan.draws().len());
    for draw in plan.draws() {
        let shader = M2ShaderPlan::resolve(model, draw)?;
        let permutation = M2ShaderPermutation::resolve(
            draw,
            M2LocalLightCount::Zero,
            M2ShadowPermutation::Disabled,
            M2ShadowFiltering::Direct,
        );
        let pipeline = renderer.prepare_m2_pipeline(shader, permutation)?;
        let material = M2MaterialState::from_material(draw.material());
        let runtime_fade_pipeline = if material.blend_enabled() {
            None
        } else {
            Some(renderer.prepare_m2_pipeline(shader.with_runtime_alpha_fade(), permutation)?)
        };
        pipelines.push((pipeline, runtime_fade_pipeline));

        let mut stages = Vec::with_capacity(draw.texture_bindings().len());
        for binding in draw.texture_bindings() {
            let texture_index = usize::from(binding.texture_index());
            let ResidentM2Texture::Authored(_source_texture) = &source.textures()[texture_index]
            else {
                unreachable!("selected replacement textures returned before GPU preparation");
            };
            let texture = texture_handles[texture_index].ok_or_else(|| {
                RuntimeTerrainFrameError::M2TextureIndex {
                    model: model.path().clone(),
                    texture_index: binding.texture_index(),
                }
            })?;
            let sampler = renderer.prepare_m2_sampler(&model.textures()[texture_index])?;
            stages.push(M2SampledTexture::new(texture, sampler));
        }
        texture_requests.push(match stages.as_slice() {
            [stage] => M2TextureSet::One(*stage),
            [first, second] => M2TextureSet::Two([*first, *second]),
            _ => unreachable!("stock shader resolution accepts only one or two stages"),
        });
    }
    let texture_sets = renderer.prepare_m2_texture_sets(&texture_requests)?;
    let draws = pipelines
        .into_iter()
        .zip(texture_sets)
        .map(
            |((pipeline, runtime_fade_pipeline), texture_set)| M2GpuDraw {
                pipeline,
                runtime_fade_pipeline,
                texture_set,
            },
        )
        .collect();
    let mut particle_pipelines = Vec::with_capacity(model.animations().particles().len());
    let mut particle_texture_requests = Vec::with_capacity(model.animations().particles().len());
    for (particle_index, emitter) in model.animations().particles().iter().enumerate() {
        let texture_index = ordinary_particle_texture_index(model, particle_index, emitter)?;
        let texture_slot = usize::from(texture_index);
        let texture = texture_handles[texture_slot].ok_or_else(|| {
            RuntimeTerrainFrameError::M2TextureIndex {
                model: model.path().clone(),
                texture_index,
            }
        })?;
        let sampler = renderer.prepare_m2_sampler(&model.textures()[texture_slot])?;
        particle_pipelines
            .push(renderer.prepare_m2_particle_pipeline(emitter.blending_type(), emitter.flags())?);
        particle_texture_requests.push(M2TextureSet::One(M2SampledTexture::new(texture, sampler)));
    }
    let particle_texture_sets = renderer.prepare_m2_texture_sets(&particle_texture_requests)?;
    let particles = particle_pipelines
        .into_iter()
        .zip(particle_texture_sets)
        .map(|(pipeline, texture_set)| M2GpuParticle {
            pipeline,
            texture_set,
        })
        .collect();
    let mut ribbons = Vec::with_capacity(model.animations().ribbons().len());
    for emitter in model.animations().ribbons() {
        let mut pass_pipelines = Vec::with_capacity(emitter.material_indices().len());
        let mut pass_textures = Vec::with_capacity(emitter.texture_indices().len());
        for (material_index, texture_index) in emitter
            .material_indices()
            .iter()
            .copied()
            .zip(emitter.texture_indices().iter().copied())
        {
            let material = model.materials()[usize::from(material_index)];
            let pipeline = renderer.prepare_m2_ribbon_pipeline(material)?;
            let texture_slot = usize::from(texture_index);
            let texture = texture_handles[texture_slot].ok_or_else(|| {
                RuntimeTerrainFrameError::M2TextureIndex {
                    model: model.path().clone(),
                    texture_index,
                }
            })?;
            let sampler = renderer.prepare_m2_sampler(&model.textures()[texture_slot])?;
            pass_pipelines.push((pipeline, material));
            pass_textures.push(M2TextureSet::One(M2SampledTexture::new(texture, sampler)));
        }
        let texture_sets = renderer.prepare_m2_texture_sets(&pass_textures)?;
        ribbons.push(
            pass_pipelines
                .into_iter()
                .zip(texture_sets)
                .map(|((pipeline, material), texture_set)| M2GpuRibbonPass {
                    pipeline,
                    texture_set,
                    material,
                })
                .collect(),
        );
    }
    Ok(Some(M2GpuSource {
        model: Arc::clone(model),
        plan,
        mesh,
        draws,
        particles,
        ribbons,
    }))
}

/// Selects the single BLP slot consumed by the ordinary particle shader.
fn ordinary_particle_texture_index(
    model: &DecodedM2Model,
    particle_index: usize,
    emitter: &M2ParticleEmitter,
) -> Result<u16, RuntimeTerrainFrameError> {
    let mut selected = None;
    let mut texture_count = 0;
    for texture_index in emitter.texture_indices().into_iter().flatten() {
        selected = Some(texture_index);
        texture_count += 1;
    }
    if texture_count != 1 {
        return Err(RuntimeTerrainFrameError::M2ParticleTextureCount {
            model: model.path().clone(),
            particle_index,
            texture_count,
        });
    }
    selected.ok_or_else(|| RuntimeTerrainFrameError::M2ParticleTextureCount {
        model: model.path().clone(),
        particle_index,
        texture_count,
    })
}
