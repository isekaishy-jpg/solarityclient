//! Renderer-local resources for the shared resident placed-M2 scene.

use std::sync::Arc;

use glam::Mat4;
use solarity_asset::DecodedM2Model;
use solarity_rendering::{
    BlpColorSpace, M2AnimationClock, M2BonePose, M2DrawCall, M2LocalLightCount, M2MaterialPose,
    M2MaterialState, M2MaterialUniform, M2MeshHandle, M2MeshPlan, M2PipelineHandle, M2PreparedDraw,
    M2SampledTexture, M2ShaderPermutation, M2ShaderPlan, M2ShadowFiltering, M2ShadowPermutation,
    M2TextureSet, M2TextureSetHandle, M2TransparentSortKey, VulkanRenderer, WorldCameraFrame,
    WorldFrustum, compare_m2_transparent, m2_section_distance_key,
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

/// One selected M2/SKIN generation uploaded once for all of its placements.
struct M2GpuSource {
    model: Arc<DecodedM2Model>,
    plan: Arc<M2MeshPlan>,
    mesh: M2MeshHandle,
    draws: Vec<M2GpuDraw>,
}

/// Fixed renderer objects paired with one exact SKIN material batch.
struct M2GpuDraw {
    pipeline: M2PipelineHandle,
    runtime_fade_pipeline: Option<M2PipelineHandle>,
    texture_set: M2TextureSetHandle,
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
            placements.push(M2GpuPlacement {
                source_index: placement.source_index(),
                transform: placement.transform(),
                owner: placement.owner(),
                flags: placement.flags(),
                color: placement.color(),
                playback: match sources[placement.source_index()].as_ref() {
                    Some(source) => M2Playback::new(&source.model, random)?,
                    None => None,
                },
            });
        }
        Ok(Self {
            sources,
            placements,
            animation_started_at: std::time::Instant::now(),
            bone_transforms: Vec::new(),
            visible_draws: Vec::new(),
            transparent_draws: Vec::new(),
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
    ) -> Result<(&[Mat4], &[M2PreparedDraw]), RuntimeTerrainFrameError> {
        self.bone_transforms.clear();
        self.visible_draws.clear();
        self.transparent_draws.clear();
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
        }
        self.transparent_draws
            .sort_unstable_by(|left, right| compare_m2_transparent(&left.key, &right.key));
        self.visible_draws
            .extend(self.transparent_draws.iter().map(|queued| queued.draw));
        Ok((&self.bone_transforms, &self.visible_draws))
    }
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
            let ResidentM2Texture::Authored(source_texture) = &source.textures()[texture_index]
            else {
                unreachable!("selected replacement textures returned before GPU preparation");
            };
            let texture = renderer.upload_blp_texture(source_texture, BlpColorSpace::Srgb)?;
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
    Ok(Some(M2GpuSource {
        model: Arc::clone(model),
        plan,
        mesh,
        draws,
    }))
}
