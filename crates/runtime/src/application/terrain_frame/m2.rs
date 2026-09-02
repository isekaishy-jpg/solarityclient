//! Renderer-local resources for the shared resident placed-M2 scene.

use std::sync::Arc;

use glam::Mat4;
use solarity_asset::{BlpTextureSource, DecodedM2Model, M2ParticleEmitter};
use solarity_ecs::WorldTransform;
use solarity_rendering::{
    BlpColorSpace, BlpTextureUploadRequest, CharacterAtlasTexture, CharacterAttachmentPoint,
    CharacterGeosetPlan, CreatureGeosetPlan, M2AnimationClock, M2BonePose, M2DrawCall,
    M2EventTimeWindow, M2LocalLightCount, M2MaterialPose, M2MaterialState, M2MaterialUniform,
    M2MeshHandle, M2MeshPlan, M2ParticleColorReplacement, M2ParticleMeshPlan,
    M2ParticlePipelineHandle, M2ParticlePose, M2ParticlePreparedDraw, M2ParticleRenderVertex,
    M2ParticleSimulation, M2ParticleTwinkleTable, M2PipelineHandle, M2PreparedDraw,
    M2RibbonControlPoint, M2RibbonMeshPlan, M2RibbonPipelineHandle, M2RibbonPose,
    M2RibbonPreparedDraw, M2RibbonRenderVertex, M2RibbonTrail, M2SampledTexture, M2SceneLightBank,
    M2ShaderPermutation, M2ShaderPlan, M2ShadowFiltering, M2ShadowPermutation,
    M2TextureImageHandle, M2TextureSet, M2TextureSetHandle, M2TransparentSortKey, VulkanRenderer,
    WorldCameraFrame, WorldFrustum, compare_m2_transparent, m2_model_distance_key,
    m2_section_distance_key, triggered_m2_event_indices,
};

use crate::application::player_coordinator::{
    ResidentCreatureFrameInput, ResidentCreatureGeosets, ResidentCreatureTexture,
    ResidentGlueCharacterFrameInput, ResidentPlayerFrameInput, ResidentPlayerTexture,
};
use crate::application::terrain_coordinator::m2_residency::{
    ResidentM2Owner, ResidentM2Scene, ResidentM2Source, ResidentM2Texture,
};
use crate::application::transport_coordinator::{ResidentTransport, ResidentTransportResource};
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
    mesh: Option<M2MeshHandle>,
    draws: Vec<Option<M2GpuDraw>>,
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
    /// Placement-local transform retained across animated parent resolution.
    local_transform: Mat4,
    transform: Mat4,
    /// Stock parent attachment used by Glue character preview models.
    glue_parent_attachment: Option<u32>,
    owner: M2GpuPlacementOwner,
    flags: u16,
    color: [u8; 4],
    opacity: f32,
    particle_colors: Option<M2ParticleColorReplacement>,
    playback: Option<M2Playback>,
    particles: Vec<M2ParticleSimulation>,
    ribbons: Vec<M2RibbonTrail>,
}

/// Placement category retained for diagnostics and player replacement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum M2GpuPlacementOwner {
    /// An ADT or WMO owner from the resident terrain generation.
    Static(ResidentM2Owner),
    /// A pre-world `Model` or `ModelFFX` widget retained by Glue.
    GlueModel { object_index: usize },
    /// Character-selection pet attached to the Glue environment marker.
    GluePet,
    /// The one authoritative character controlled by this client.
    PlayerBody { guid: u64 },
    /// Mount-main parent beneath the controlled character.
    PlayerMount { guid: u64 },
    /// One visible player character controlled by another client.
    RemotePlayerBody { guid: u64 },
    /// Mount-main parent beneath one visible remote character.
    RemotePlayerMount { guid: u64 },
    /// One visible non-player unit projected from authoritative ECS state.
    CreatureBody { guid: u64 },
    /// The controlled player's current movement-parent GameObject.
    Transport { guid: u64 },
    /// One equipment M2 driven by an animated player attachment point.
    PlayerItem {
        guid: u64,
        point: CharacterAttachmentPoint,
    },
    /// One enchant/display effect driven by its equipped item M2.
    PlayerItemVisual {
        guid: u64,
        item_point: CharacterAttachmentPoint,
        effect_point: u32,
    },
}

/// One texture slot resolved for this exact source/placement generation.
enum M2ResolvedTexture<'source> {
    /// A shared authored BLP selected through archive precedence.
    Authored(&'source solarity_asset::BlpTextureSource),
    /// A dynamic body image composed for one character placement.
    CharacterAtlas(&'source CharacterAtlasTexture),
    /// A replacement category not supplied by this presentation owner.
    Unresolved(solarity_asset::M2TextureKind),
}

/// One resident submesh-selection scheme consumed during GPU preparation.
#[derive(Clone, Copy)]
enum M2GeosetSelection<'source> {
    /// Full player-character component selection.
    Character(&'source CharacterGeosetPlan),
    /// Eight four-bit selectors from `CreatureDisplayInfo.geoset_data`.
    Creature(&'source CreatureGeosetPlan),
}

impl M2GeosetSelection<'_> {
    /// Tests one SKIN submesh against the source table's exact selection rule.
    fn is_visible(self, geoset_id: u16) -> bool {
        match self {
            Self::Character(plan) => plan.is_visible(geoset_id),
            Self::Creature(plan) => plan.is_visible(geoset_id),
        }
    }
}

impl<'source> From<&'source ResidentCreatureGeosets> for M2GeosetSelection<'source> {
    fn from(geosets: &'source ResidentCreatureGeosets) -> Self {
        match geosets {
            ResidentCreatureGeosets::Character(plan) => Self::Character(plan),
            ResidentCreatureGeosets::Packed(plan) => Self::Creature(plan),
        }
    }
}

/// Per-instance sequence state retained by stock's `CM2Model` owner.
struct M2Playback {
    animation_id: u16,
    sequence: usize,
    sequence_duration_ms: f32,
    cycle_count: u32,
    cycle_started_ms: f32,
    has_variations: bool,
    previous_event_elapsed_ms: f32,
    previous_global_event_elapsed_ms: f32,
    event_timeline_started: bool,
}

/// Current bone clock plus an expired variation tail awaiting event dispatch.
struct M2PlaybackAdvance {
    clock: M2AnimationClock,
    expired_variation: Option<M2ExpiredVariation>,
}

/// Final event interval and pose clock from one replaced sequence variation.
struct M2ExpiredVariation {
    clock: M2AnimationClock,
    event_window: M2EventTimeWindow,
}

impl M2Playback {
    /// Selects one base animation and consumes its authored cycle-count roll.
    fn new(
        model: &DecodedM2Model,
        animation_id: u16,
        random: &mut CrtRand,
    ) -> Result<Option<Self>, RuntimeTerrainFrameError> {
        let animations = model.animations();
        if animations.sequences().is_empty() {
            return Ok(Some(Self {
                animation_id,
                sequence: 0,
                sequence_duration_ms: 0.0,
                cycle_count: 1,
                cycle_started_ms: 0.0,
                has_variations: false,
                previous_event_elapsed_ms: 0.0,
                previous_global_event_elapsed_ms: 0.0,
                event_timeline_started: false,
            }));
        }
        let sequence = animations
            .sequence_for_variation(animation_id, 0)
            .or_else(|| {
                animations.select_sequence(animation_id, None, u32::from(random.next_u15()))
            });
        let Some(sequence) = sequence else {
            tracing::debug!(
                path = %model.path(),
                animation_id,
                "placed M2 omitted because its selected animation is unavailable"
            );
            return Ok(None);
        };
        let sequence_duration_ms = resolved_sequence_duration(model, sequence)?;
        let cycle_count = animations.sequences()[sequence].cycle_count(random.next_u15());
        let variation_count = animations
            .available_variation_count(animation_id)
            .ok_or_else(|| RuntimeTerrainFrameError::M2AnimationSelection {
                model: model.path().clone(),
                animation_id,
            })?;
        Ok(Some(Self {
            animation_id,
            sequence,
            sequence_duration_ms,
            cycle_count,
            cycle_started_ms: 0.0,
            has_variations: variation_count > 1,
            previous_event_elapsed_ms: 0.0,
            previous_global_event_elapsed_ms: 0.0,
            event_timeline_started: false,
        }))
    }

    /// Restarts playback when authoritative gameplay selects another base ID.
    fn select_animation(
        &mut self,
        model: &DecodedM2Model,
        animation_id: u16,
        animation_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if self.animation_id == animation_id || model.animations().sequences().is_empty() {
            self.animation_id = animation_id;
            return Ok(());
        }
        let animations = model.animations();
        let sequence = animations
            .sequence_for_variation(animation_id, 0)
            .or_else(|| {
                animations.select_sequence(animation_id, None, u32::from(random.next_u15()))
            })
            .ok_or_else(|| RuntimeTerrainFrameError::M2AnimationSelection {
                model: model.path().clone(),
                animation_id,
            })?;
        self.animation_id = animation_id;
        self.sequence = sequence;
        self.sequence_duration_ms = resolved_sequence_duration(model, sequence)?;
        self.cycle_count = animations.sequences()[sequence].cycle_count(random.next_u15());
        self.cycle_started_ms = animation_time_ms;
        self.previous_event_elapsed_ms = 0.0;
        self.event_timeline_started = false;
        self.has_variations = animations
            .available_variation_count(animation_id)
            .ok_or_else(|| RuntimeTerrainFrameError::M2AnimationSelection {
                model: model.path().clone(),
                animation_id,
            })?
            > 1;
        Ok(())
    }

    /// Advances one expired stock timer and returns the selected sequence clock.
    fn clock(
        &mut self,
        model: &DecodedM2Model,
        animation_time_ms: f32,
        global_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<M2PlaybackAdvance, RuntimeTerrainFrameError> {
        let elapsed_ms = (animation_time_ms - self.cycle_started_ms).max(0.0);
        let selected_span_ms = self.sequence_duration_ms * self.cycle_count as f32;
        let mut expired_variation = None;
        if self.has_variations && self.sequence_duration_ms > 0.0 && elapsed_ms >= selected_span_ms
        {
            // Stock finishes the old sequence's event interval before replacing
            // its timer. Bone-relative callbacks from that tail must also use
            // the old sequence's terminal pose, not the newly selected pose.
            expired_variation = Some(M2ExpiredVariation {
                clock: M2AnimationClock::new(
                    self.sequence,
                    self.sequence_duration_ms,
                    global_time_ms,
                ),
                event_window: M2EventTimeWindow::new(
                    self.sequence,
                    self.previous_event_elapsed_ms,
                    selected_span_ms,
                    !self.event_timeline_started,
                    true,
                ),
            });
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
            self.previous_event_elapsed_ms = 0.0;
            self.event_timeline_started = false;
        }
        Ok(M2PlaybackAdvance {
            clock: world_animation_clock(
                self.sequence,
                self.sequence_duration_ms,
                animation_time_ms - self.cycle_started_ms,
                global_time_ms,
            ),
            expired_variation,
        })
    }

    /// Advances the unwrapped clocks retained exclusively for event crossing.
    fn event_window(&mut self, animation_time_ms: f32, global_time_ms: f32) -> M2EventTimeWindow {
        let current_event_elapsed_ms = (animation_time_ms - self.cycle_started_ms).max(0.0);
        let window = M2EventTimeWindow::new(
            self.sequence,
            self.previous_event_elapsed_ms,
            current_event_elapsed_ms,
            !self.event_timeline_started,
            true,
        )
        .with_global_time(self.previous_global_event_elapsed_ms, global_time_ms);
        self.previous_event_elapsed_ms = current_event_elapsed_ms;
        self.previous_global_event_elapsed_ms = global_time_ms;
        self.event_timeline_started = true;
        window
    }
}

/// One generic authored M2 callback resolved into world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::application) struct RuntimeM2Event {
    identifier: [u8; 4],
    data: u32,
    position: glam::Vec3,
    owner_guid: Option<u64>,
}

/// Camera markers sampled from the controlled player's current mount pose.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(in crate::application) struct RuntimeMountCameraSample {
    animated_height: Option<f32>,
    fixed_height: Option<f32>,
    time_ms: f32,
}

impl RuntimeMountCameraSample {
    pub(in crate::application) const fn animated_height(self) -> Option<f32> {
        self.animated_height
    }

    pub(in crate::application) const fn fixed_height(self) -> Option<f32> {
        self.fixed_height
    }

    pub(in crate::application) const fn time_ms(self) -> f32 {
        self.time_ms
    }
}

impl RuntimeM2Event {
    pub(in crate::application) const fn identifier(self) -> [u8; 4] {
        self.identifier
    }

    pub(in crate::application) const fn data(self) -> u32 {
        self.data
    }

    pub(in crate::application) const fn position(self) -> glam::Vec3 {
        self.position
    }

    pub(in crate::application) const fn owner_guid(self) -> Option<u64> {
        self.owner_guid
    }
}

/// All resident M2 geometry and transforms owned by one terrain generation.
pub(in crate::application) struct M2Frame {
    sources: Vec<Option<M2GpuSource>>,
    placements: Vec<M2GpuPlacement>,
    particle_twinkle: Arc<M2ParticleTwinkleTable>,
    animation_started_at: std::time::Instant,
    bone_transforms: Vec<Mat4>,
    visible_draws: Vec<M2PreparedDraw>,
    transparent_draws: Vec<M2TransparentDraw>,
    model_distance_sort: Vec<bool>,
    particle_vertices: Vec<M2ParticleRenderVertex>,
    particle_indices: Vec<u32>,
    particle_draws: Vec<M2ParticlePreparedDraw>,
    ribbon_vertices: Vec<M2RibbonRenderVertex>,
    ribbon_draws: Vec<M2RibbonPreparedDraw>,
    triggered_events: Vec<RuntimeM2Event>,
    mount_camera_sample: Option<RuntimeMountCameraSample>,
    last_effect_time_ms: Option<f32>,
}

/// Borrowed dynamic streams assembled for one unified world submission.
pub(in crate::application) struct M2VisibleFrame<'frame> {
    pub(in crate::application) bone_transforms: &'frame [Mat4],
    pub(in crate::application) draws: &'frame [M2PreparedDraw],
    pub(in crate::application) particle_vertices: &'frame [M2ParticleRenderVertex],
    pub(in crate::application) particle_indices: &'frame [u32],
    pub(in crate::application) particle_draws: &'frame [M2ParticlePreparedDraw],
    pub(in crate::application) ribbon_vertices: &'frame [M2RibbonRenderVertex],
    pub(in crate::application) ribbon_draws: &'frame [M2RibbonPreparedDraw],
}

impl M2Frame {
    /// Publishes profile zero and its fully resolved static material resources.
    pub(super) fn prepare(
        renderer: &mut VulkanRenderer,
        scene: &ResidentM2Scene,
        random: &mut CrtRand,
        particle_twinkle: Arc<M2ParticleTwinkleTable>,
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
                    let playback = M2Playback::new(&source.model, 0, random)?;
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
                local_transform: placement.transform(),
                transform: placement.transform(),
                glue_parent_attachment: None,
                owner: M2GpuPlacementOwner::Static(placement.owner()),
                flags: placement.flags(),
                color: placement.color(),
                opacity: 1.0,
                particle_colors: None,
                playback,
                particles,
                ribbons,
            });
        }
        Ok(Self {
            sources,
            placements,
            particle_twinkle,
            animation_started_at: std::time::Instant::now(),
            bone_transforms: Vec::new(),
            visible_draws: Vec::new(),
            transparent_draws: Vec::new(),
            model_distance_sort: Vec::new(),
            particle_vertices: Vec::new(),
            particle_indices: Vec::new(),
            particle_draws: Vec::new(),
            ribbon_vertices: Vec::new(),
            ribbon_draws: Vec::new(),
            triggered_events: Vec::new(),
            mount_camera_sample: None,
            last_effect_time_ms: None,
        })
    }

    /// Replaces the dynamic movement-parent M2 without disturbing terrain M2s.
    pub(super) fn replace_transport(
        &mut self,
        renderer: &mut VulkanRenderer,
        transport: Option<&ResidentTransport>,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let prepared = match transport {
            Some(transport) => match (
                transport.resource(),
                transport.transform(),
                transport.scale(),
            ) {
                (ResidentTransportResource::M2(source), Some(transform), Some(scale)) => {
                    match prepare_source(renderer, source)? {
                        Some(gpu) => {
                            let matrix = transport_placement_transform(transform, scale)?;
                            let placement = unit_gpu_placement(
                                0,
                                matrix,
                                M2GpuPlacementOwner::Transport {
                                    guid: transport.guid(),
                                },
                                source.model(),
                                transport_animation_id(transport.state()),
                                None,
                                random,
                            )?;
                            Some((gpu, placement))
                        }
                        None => None,
                    }
                }
                _ => None,
            },
            None => None,
        };

        self.remove_transport();
        if let Some((source, mut placement)) = prepared {
            let source_index = self.sources.len();
            placement.source_index = source_index;
            self.sources.push(Some(source));
            self.placements.push(placement);
        }
        Ok(())
    }

    /// Applies the latest replicated transport transform in place.
    pub(super) fn update_transport_state(
        &mut self,
        transport: Option<&ResidentTransport>,
        animation_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let Some(transport) = transport else {
            return Ok(());
        };
        let (ResidentTransportResource::M2(_), Some(transform), Some(scale)) = (
            transport.resource(),
            transport.transform(),
            transport.scale(),
        ) else {
            return Ok(());
        };
        let Some(placement) = self.placements.iter_mut().find(|placement| {
            placement.owner
                == M2GpuPlacementOwner::Transport {
                    guid: transport.guid(),
                }
        }) else {
            return Ok(());
        };
        let matrix = transport_placement_transform(transform, scale)?;
        placement.local_transform = matrix;
        placement.transform = matrix;
        let Some(source) = self.sources[placement.source_index].as_ref() else {
            return Ok(());
        };
        if let Some(playback) = placement.playback.as_mut() {
            playback.select_animation(
                &source.model,
                transport_animation_id(transport.state()),
                animation_time_ms,
                random,
            )?;
        }
        Ok(())
    }

    /// Drops renderer references owned only by the current transport.
    fn remove_transport(&mut self) {
        let mut source_indices = Vec::new();
        self.placements.retain(|placement| {
            if matches!(placement.owner, M2GpuPlacementOwner::Transport { .. }) {
                source_indices.push(placement.source_index);
                false
            } else {
                true
            }
        });
        for source_index in source_indices {
            if let Some(source) = self.sources.get_mut(source_index) {
                *source = None;
            }
        }
    }

    /// Publishes one fully authored Glue model without terrain-owner aliases.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::application) fn prepare_glue_model(
        renderer: &mut VulkanRenderer,
        model: Arc<DecodedM2Model>,
        textures: &[Arc<BlpTextureSource>],
        object_index: usize,
        animation_id: u16,
        model_scale: f32,
        local_light_count: M2LocalLightCount,
        random: &mut CrtRand,
        particle_twinkle: Arc<M2ParticleTwinkleTable>,
    ) -> Result<Self, RuntimeTerrainFrameError> {
        if !model_scale.is_finite() || model_scale <= 0.0 {
            return Err(RuntimeTerrainFrameError::InvalidGlueM2Scale);
        }
        let resolved = textures
            .iter()
            .map(|texture| M2ResolvedTexture::Authored(texture.as_ref()))
            .collect::<Vec<_>>();
        let source = prepare_gpu_source(renderer, &model, &resolved, None, local_light_count)?;
        let playback = M2Playback::new(&model, animation_id, random)?;
        let particles = model
            .animations()
            .particles()
            .iter()
            .map(|_emitter| {
                let first = u32::from(random.next_u15());
                let second = u32::from(random.next_u15());
                M2ParticleSimulation::new(first << 16 | second)
            })
            .collect();
        let ribbons = model
            .animations()
            .ribbons()
            .iter()
            .map(M2RibbonTrail::new)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            sources: vec![Some(source)],
            placements: vec![M2GpuPlacement {
                source_index: 0,
                local_transform: Mat4::from_scale(glam::Vec3::splat(model_scale)),
                transform: Mat4::from_scale(glam::Vec3::splat(model_scale)),
                glue_parent_attachment: None,
                owner: M2GpuPlacementOwner::GlueModel { object_index },
                flags: 0,
                color: [u8::MAX; 4],
                opacity: 1.0,
                particle_colors: None,
                playback,
                particles,
                ribbons,
            }],
            particle_twinkle,
            animation_started_at: std::time::Instant::now(),
            bone_transforms: Vec::new(),
            visible_draws: Vec::new(),
            transparent_draws: Vec::new(),
            model_distance_sort: Vec::new(),
            particle_vertices: Vec::new(),
            particle_indices: Vec::new(),
            particle_draws: Vec::new(),
            ribbon_vertices: Vec::new(),
            ribbon_draws: Vec::new(),
            triggered_events: Vec::new(),
            mount_camera_sample: None,
            last_effect_time_ms: None,
        })
    }

    /// Replaces the character body and optional pet beneath the retained Glue environment.
    pub(in crate::application) fn replace_glue_character(
        &mut self,
        renderer: &mut VulkanRenderer,
        input: Option<ResidentGlueCharacterFrameInput<'_>>,
        character_light_count: M2LocalLightCount,
        pet_light_count: M2LocalLightCount,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let Some(input) = input else {
            self.remove_player();
            return Ok(());
        };
        if !input.model_scale().is_finite()
            || input.model_scale() <= 0.0
            || !input.facing_radians().is_finite()
        {
            return Err(RuntimeTerrainFrameError::InvalidUnitM2Transform);
        }
        let resolved = input
            .textures()
            .iter()
            .map(|texture| match texture {
                ResidentPlayerTexture::Authored(source) => {
                    M2ResolvedTexture::Authored(source.as_ref())
                }
                ResidentPlayerTexture::BodyAtlas => {
                    M2ResolvedTexture::CharacterAtlas(input.atlas())
                }
                ResidentPlayerTexture::Unresolved(kind) => M2ResolvedTexture::Unresolved(*kind),
            })
            .collect::<Vec<_>>();
        let source = prepare_gpu_source(
            renderer,
            input.model(),
            &resolved,
            Some(M2GeosetSelection::Character(input.geosets())),
            character_light_count,
        )?;
        let transform = Mat4::from_rotation_z(input.facing_radians())
            * Mat4::from_scale(glam::Vec3::splat(input.model_scale()));
        let placement = unit_gpu_placement(
            0,
            transform,
            M2GpuPlacementOwner::PlayerBody { guid: 0 },
            input.model(),
            input.animation().animation_id(),
            input.particle_colors().cloned(),
            random,
        )?;
        // Wow.exe 0x004E0160/0x004E2E70 installs the Glue body at the scene
        // origin with unit translation and scale. Facing is retained beside
        // that transform and consumed by the model renderer. Attachment zero
        // belongs to the backdrop scene; parenting the body to its rotated
        // marker moves the preview rightward and turns it away from camera.
        let mut prepared = vec![(source, placement)];
        for attachment in input.attachments() {
            if input.model().attachment(attachment.point().id()).is_none() {
                return Err(RuntimeTerrainFrameError::MissingPlayerM2Attachment {
                    model: input.model().path().clone(),
                    attachment_id: attachment.point().id(),
                });
            }
            let resolved = attachment
                .textures()
                .iter()
                .map(|texture| match texture {
                    ResidentPlayerTexture::Authored(source) => {
                        M2ResolvedTexture::Authored(source.as_ref())
                    }
                    ResidentPlayerTexture::BodyAtlas => {
                        M2ResolvedTexture::CharacterAtlas(input.atlas())
                    }
                    ResidentPlayerTexture::Unresolved(kind) => M2ResolvedTexture::Unresolved(*kind),
                })
                .collect::<Vec<_>>();
            let source = prepare_gpu_source(
                renderer,
                attachment.model(),
                &resolved,
                None,
                character_light_count,
            )?;
            let placement = unit_gpu_placement(
                0,
                transform,
                M2GpuPlacementOwner::PlayerItem {
                    guid: 0,
                    point: attachment.point(),
                },
                attachment.model(),
                0,
                attachment.particle_colors().cloned(),
                random,
            )?;
            prepared.push((source, placement));
            for effect in attachment.visual_effects() {
                let resolved = effect
                    .textures()
                    .iter()
                    .map(|texture| match texture {
                        ResidentPlayerTexture::Authored(source) => {
                            M2ResolvedTexture::Authored(source.as_ref())
                        }
                        ResidentPlayerTexture::BodyAtlas => {
                            M2ResolvedTexture::CharacterAtlas(input.atlas())
                        }
                        ResidentPlayerTexture::Unresolved(kind) => {
                            M2ResolvedTexture::Unresolved(*kind)
                        }
                    })
                    .collect::<Vec<_>>();
                let source = prepare_gpu_source(
                    renderer,
                    effect.model(),
                    &resolved,
                    None,
                    character_light_count,
                )?;
                let placement = unit_gpu_placement(
                    0,
                    transform,
                    M2GpuPlacementOwner::PlayerItemVisual {
                        guid: 0,
                        item_point: attachment.point(),
                        effect_point: effect.point(),
                    },
                    effect.model(),
                    0,
                    None,
                    random,
                )?;
                prepared.push((source, placement));
            }
        }
        if let Some(pet) = input.pet() {
            if !pet.model_scale().is_finite() || pet.model_scale() <= 0.0 {
                return Err(RuntimeTerrainFrameError::InvalidUnitM2Transform);
            }
            let resolved = pet
                .textures()
                .iter()
                .map(|texture| match texture {
                    ResidentCreatureTexture::Authored(source) => {
                        M2ResolvedTexture::Authored(source.as_ref())
                    }
                    ResidentCreatureTexture::Unresolved(kind) => {
                        M2ResolvedTexture::Unresolved(*kind)
                    }
                })
                .collect::<Vec<_>>();
            let source = prepare_gpu_source(
                renderer,
                pet.model(),
                &resolved,
                pet.geosets().map(M2GeosetSelection::from),
                pet_light_count,
            )?;
            let transform = Mat4::from_scale(glam::Vec3::splat(pet.model_scale()));
            let mut placement = unit_gpu_placement(
                0,
                transform,
                M2GpuPlacementOwner::GluePet,
                pet.model(),
                pet.animation().animation_id(),
                pet.particle_colors().cloned(),
                random,
            )?;
            placement.glue_parent_attachment = Some(1);
            prepared.push((source, placement));
        }
        self.remove_player();
        for (source, placement) in prepared {
            let source_index = self.sources.len();
            self.sources.push(Some(source));
            self.placements.push(M2GpuPlacement {
                source_index,
                ..placement
            });
        }
        Ok(())
    }

    /// Applies the Model frame's effective alpha to its environment,
    /// character, equipment, effects, and pet as one composed widget.
    pub(in crate::application) fn set_glue_opacity(
        &mut self,
        opacity: f32,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
            return Err(RuntimeTerrainFrameError::InvalidGlueM2Opacity { opacity });
        }
        for placement in &mut self.placements {
            if matches!(
                placement.owner,
                M2GpuPlacementOwner::GlueModel { .. }
                    | M2GpuPlacementOwner::GluePet
                    | M2GpuPlacementOwner::PlayerBody { guid: 0 }
                    | M2GpuPlacementOwner::PlayerItem { guid: 0, .. }
                    | M2GpuPlacementOwner::PlayerItemVisual { guid: 0, .. }
            ) {
                placement.opacity = opacity;
            }
        }
        Ok(())
    }

    /// Updates rotation without rebuilding the retained Glue character generation.
    pub(in crate::application) fn update_glue_character_transform(
        &mut self,
        model_scale: f32,
        facing_radians: f32,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if !model_scale.is_finite() || model_scale <= 0.0 || !facing_radians.is_finite() {
            return Err(RuntimeTerrainFrameError::InvalidUnitM2Transform);
        }
        let placement = self
            .placements
            .iter_mut()
            .find(|placement| {
                matches!(placement.owner, M2GpuPlacementOwner::PlayerBody { guid: 0 })
            })
            .ok_or(RuntimeTerrainFrameError::MissingGlueM2Placement)?;
        placement.local_transform = Mat4::from_rotation_z(facing_radians)
            * Mat4::from_scale(glam::Vec3::splat(model_scale));
        Ok(())
    }

    /// Replaces the one player-owned source and placement transactionally.
    pub(super) fn replace_player(
        &mut self,
        renderer: &mut VulkanRenderer,
        input: Option<ResidentPlayerFrameInput<'_>>,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let Some(input) = input else {
            self.remove_player();
            return Ok(());
        };
        let prepared = prepare_character_gpu(
            renderer,
            &input,
            M2GpuPlacementOwner::PlayerBody { guid: input.guid() },
            random,
        )?;
        self.remove_player();
        for (source, placement) in prepared {
            let source_index = self.sources.len();
            self.sources.push(Some(source));
            self.placements.push(M2GpuPlacement {
                source_index,
                ..placement
            });
        }
        Ok(())
    }

    /// Replaces all visible creature sources and placements transactionally.
    pub(super) fn replace_creatures(
        &mut self,
        renderer: &mut VulkanRenderer,
        inputs: &[ResidentCreatureFrameInput<'_>],
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let mut prepared = Vec::with_capacity(inputs.len());
        for input in inputs {
            let resolved = input
                .textures()
                .iter()
                .map(|texture| match texture {
                    ResidentCreatureTexture::Authored(source) => {
                        M2ResolvedTexture::Authored(source.as_ref())
                    }
                    ResidentCreatureTexture::Unresolved(kind) => {
                        M2ResolvedTexture::Unresolved(*kind)
                    }
                })
                .collect::<Vec<_>>();
            let source = prepare_gpu_source(
                renderer,
                input.model(),
                &resolved,
                input.geosets().map(M2GeosetSelection::from),
                M2LocalLightCount::Zero,
            )?;
            let transform =
                unit_placement_transform(input.world_transform(), input.object_scale())?;
            let placement = unit_gpu_placement(
                0,
                transform,
                M2GpuPlacementOwner::CreatureBody { guid: input.guid() },
                input.model(),
                input.animation().animation_id(),
                input.particle_colors().cloned(),
                random,
            )?;
            prepared.push((source, placement));
        }

        self.remove_creatures();
        for (source, placement) in prepared {
            let source_index = self.sources.len();
            self.sources.push(Some(source));
            self.placements.push(M2GpuPlacement {
                source_index,
                ..placement
            });
        }
        Ok(())
    }

    /// Replaces every visible remote character and equipped child placement.
    pub(super) fn replace_remote_players(
        &mut self,
        renderer: &mut VulkanRenderer,
        inputs: &[ResidentPlayerFrameInput<'_>],
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let mut prepared = Vec::with_capacity(inputs.len());
        for input in inputs {
            prepared.push(prepare_character_gpu(
                renderer,
                input,
                M2GpuPlacementOwner::RemotePlayerBody { guid: input.guid() },
                random,
            )?);
        }
        self.remove_remote_players();
        for character in prepared {
            for (source, placement) in character {
                let source_index = self.sources.len();
                self.sources.push(Some(source));
                self.placements.push(M2GpuPlacement {
                    source_index,
                    ..placement
                });
            }
        }
        Ok(())
    }

    /// Updates authoritative player movement and base animation in place.
    pub(super) fn update_player_state(
        &mut self,
        input: ResidentPlayerFrameInput<'_>,
        animation_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let transform = if let Some(mount) = input.mount() {
            let transform =
                unit_placement_transform(input.world_transform(), mount.object_scale())?;
            let placement = self
                .placements
                .iter_mut()
                .find(|placement| {
                    placement.owner == (M2GpuPlacementOwner::PlayerMount { guid: input.guid() })
                })
                .ok_or(RuntimeTerrainFrameError::MissingPlayerMountM2Placement {
                    guid: input.guid(),
                })?;
            placement.transform = transform;
            placement.local_transform = transform;
            let Some(source) = self.sources[placement.source_index].as_ref() else {
                return Ok(());
            };
            if let Some(playback) = placement.playback.as_mut() {
                playback.select_animation(
                    &source.model,
                    mount.animation().animation_id(),
                    animation_time_ms,
                    random,
                )?;
            }
            transform
        } else {
            unit_placement_transform(input.world_transform(), input.object_scale())?
        };
        let placement = self
            .placements
            .iter_mut()
            .find(|placement| {
                placement.owner == (M2GpuPlacementOwner::PlayerBody { guid: input.guid() })
            })
            .ok_or(RuntimeTerrainFrameError::MissingPlayerM2Placement { guid: input.guid() })?;
        placement.transform = transform;
        placement.local_transform = transform;
        let Some(source) = self.sources[placement.source_index].as_ref() else {
            return Ok(());
        };
        if let Some(playback) = placement.playback.as_mut() {
            playback.select_animation(
                &source.model,
                input.animation().animation_id(),
                animation_time_ms,
                random,
            )?;
        }
        Ok(())
    }

    /// Updates every authoritative creature transform and selected animation.
    pub(super) fn update_creature_states(
        &mut self,
        inputs: &[ResidentCreatureFrameInput<'_>],
        animation_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        for input in inputs {
            let transform =
                unit_placement_transform(input.world_transform(), input.object_scale())?;
            let placement = self
                .placements
                .iter_mut()
                .find(|placement| {
                    placement.owner == (M2GpuPlacementOwner::CreatureBody { guid: input.guid() })
                })
                .ok_or(RuntimeTerrainFrameError::MissingCreatureM2Placement {
                    guid: input.guid(),
                })?;
            placement.transform = transform;
            placement.local_transform = transform;
            let Some(source) = self.sources[placement.source_index].as_ref() else {
                continue;
            };
            if let Some(playback) = placement.playback.as_mut() {
                playback.select_animation(
                    &source.model,
                    input.animation().animation_id(),
                    animation_time_ms,
                    random,
                )?;
            }
        }
        Ok(())
    }

    /// Updates every authoritative remote-player transform and animation.
    pub(super) fn update_remote_player_states(
        &mut self,
        inputs: &[ResidentPlayerFrameInput<'_>],
        animation_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        for input in inputs {
            let transform = if let Some(mount) = input.mount() {
                let transform =
                    unit_placement_transform(input.world_transform(), mount.object_scale())?;
                let placement = self
                    .placements
                    .iter_mut()
                    .find(|placement| {
                        placement.owner
                            == (M2GpuPlacementOwner::RemotePlayerMount { guid: input.guid() })
                    })
                    .ok_or(
                        RuntimeTerrainFrameError::MissingRemotePlayerMountM2Placement {
                            guid: input.guid(),
                        },
                    )?;
                placement.transform = transform;
                placement.local_transform = transform;
                let Some(source) = self.sources[placement.source_index].as_ref() else {
                    continue;
                };
                if let Some(playback) = placement.playback.as_mut() {
                    playback.select_animation(
                        &source.model,
                        mount.animation().animation_id(),
                        animation_time_ms,
                        random,
                    )?;
                }
                transform
            } else {
                unit_placement_transform(input.world_transform(), input.object_scale())?
            };
            let placement = self
                .placements
                .iter_mut()
                .find(|placement| {
                    placement.owner
                        == (M2GpuPlacementOwner::RemotePlayerBody { guid: input.guid() })
                })
                .ok_or(RuntimeTerrainFrameError::MissingRemotePlayerM2Placement {
                    guid: input.guid(),
                })?;
            placement.transform = transform;
            placement.local_transform = transform;
            let Some(source) = self.sources[placement.source_index].as_ref() else {
                continue;
            };
            if let Some(playback) = placement.playback.as_mut() {
                playback.select_animation(
                    &source.model,
                    input.animation().animation_id(),
                    animation_time_ms,
                    random,
                )?;
            }
        }
        Ok(())
    }

    /// Drops local references to the previous player generation.
    fn remove_player(&mut self) {
        let local_guid = self
            .placements
            .iter()
            .find_map(|placement| match placement.owner {
                M2GpuPlacementOwner::PlayerBody { guid } => Some(guid),
                _ => None,
            });
        let mut player_sources = Vec::new();
        self.placements.retain(|placement| {
            let owned = match placement.owner {
                M2GpuPlacementOwner::PlayerBody { .. }
                | M2GpuPlacementOwner::PlayerMount { .. }
                | M2GpuPlacementOwner::GluePet => true,
                M2GpuPlacementOwner::PlayerItem { guid, .. }
                | M2GpuPlacementOwner::PlayerItemVisual { guid, .. } => local_guid == Some(guid),
                M2GpuPlacementOwner::Static(_)
                | M2GpuPlacementOwner::GlueModel { .. }
                | M2GpuPlacementOwner::RemotePlayerBody { .. }
                | M2GpuPlacementOwner::RemotePlayerMount { .. }
                | M2GpuPlacementOwner::CreatureBody { .. }
                | M2GpuPlacementOwner::Transport { .. } => false,
            };
            if owned {
                player_sources.push(placement.source_index);
            }
            !owned
        });
        for source_index in player_sources {
            if let Some(source) = self.sources.get_mut(source_index) {
                *source = None;
            }
        }
    }

    /// Drops local references to the previous visible-creature generation.
    fn remove_creatures(&mut self) {
        let mut creature_sources = Vec::new();
        self.placements.retain(|placement| {
            if matches!(placement.owner, M2GpuPlacementOwner::CreatureBody { .. }) {
                creature_sources.push(placement.source_index);
                false
            } else {
                true
            }
        });
        for source_index in creature_sources {
            if let Some(source) = self.sources.get_mut(source_index) {
                *source = None;
            }
        }
    }

    /// Drops remote character bodies and every child placement they own.
    fn remove_remote_players(&mut self) {
        let remote_guids = self
            .placements
            .iter()
            .filter_map(|placement| match placement.owner {
                M2GpuPlacementOwner::RemotePlayerBody { guid } => Some(guid),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut remote_sources = Vec::new();
        self.placements.retain(|placement| {
            let owned = match placement.owner {
                M2GpuPlacementOwner::RemotePlayerBody { .. }
                | M2GpuPlacementOwner::RemotePlayerMount { .. } => true,
                M2GpuPlacementOwner::PlayerItem { guid, .. }
                | M2GpuPlacementOwner::PlayerItemVisual { guid, .. } => {
                    remote_guids.contains(&guid)
                }
                M2GpuPlacementOwner::Static(_)
                | M2GpuPlacementOwner::GlueModel { .. }
                | M2GpuPlacementOwner::GluePet
                | M2GpuPlacementOwner::PlayerBody { .. }
                | M2GpuPlacementOwner::PlayerMount { .. }
                | M2GpuPlacementOwner::CreatureBody { .. }
                | M2GpuPlacementOwner::Transport { .. } => false,
            };
            if owned {
                remote_sources.push(placement.source_index);
            }
            !owned
        });
        for source_index in remote_sources {
            if let Some(source) = self.sources.get_mut(source_index) {
                *source = None;
            }
        }
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
            for draw in source.draws.iter().flatten() {
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
    pub(in crate::application) fn animation_time_ms(&self) -> f32 {
        self.animation_started_at.elapsed().as_secs_f32() * 1_000.0
    }

    /// Advances the one Glue placement before its authored camera is sampled.
    pub(in crate::application) fn advance_glue_animation_clock(
        &mut self,
        animation_time_ms: f32,
        global_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<M2AnimationClock, RuntimeTerrainFrameError> {
        let placement = self
            .placements
            .iter_mut()
            .find(|placement| matches!(placement.owner, M2GpuPlacementOwner::GlueModel { .. }))
            .ok_or(RuntimeTerrainFrameError::MissingGlueM2Placement)?;
        let source = self
            .sources
            .get(placement.source_index)
            .and_then(Option::as_ref)
            .ok_or(RuntimeTerrainFrameError::MissingGlueM2Placement)?;
        let playback = placement
            .playback
            .as_mut()
            .ok_or(RuntimeTerrainFrameError::MissingGlueM2Placement)?;
        Ok(playback
            .clock(&source.model, animation_time_ms, global_time_ms, random)?
            .clock)
    }

    /// Culls placements and builds their bone/material draw packets in place.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::application) fn prepare_visible_draws(
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
        self.triggered_events.clear();
        self.mount_camera_sample = None;
        let effect_delta_seconds = self.last_effect_time_ms.map_or(0.0, |previous| {
            (animation_time_ms - previous).max(0.0) * 0.001
        });
        self.last_effect_time_ms = Some(animation_time_ms);
        let requested_items = self
            .placements
            .iter()
            .filter_map(|placement| match placement.owner {
                M2GpuPlacementOwner::PlayerItem { guid, point } => Some((guid, point)),
                M2GpuPlacementOwner::Static(_)
                | M2GpuPlacementOwner::GlueModel { .. }
                | M2GpuPlacementOwner::GluePet
                | M2GpuPlacementOwner::PlayerBody { .. }
                | M2GpuPlacementOwner::PlayerMount { .. }
                | M2GpuPlacementOwner::RemotePlayerBody { .. }
                | M2GpuPlacementOwner::RemotePlayerMount { .. }
                | M2GpuPlacementOwner::CreatureBody { .. }
                | M2GpuPlacementOwner::Transport { .. }
                | M2GpuPlacementOwner::PlayerItemVisual { .. } => None,
            })
            .collect::<Vec<_>>();
        let requested_visuals = self
            .placements
            .iter()
            .filter_map(|placement| match placement.owner {
                M2GpuPlacementOwner::PlayerItemVisual {
                    guid,
                    item_point,
                    effect_point,
                } => Some((guid, item_point, effect_point)),
                M2GpuPlacementOwner::Static(_)
                | M2GpuPlacementOwner::GlueModel { .. }
                | M2GpuPlacementOwner::GluePet
                | M2GpuPlacementOwner::PlayerBody { .. }
                | M2GpuPlacementOwner::PlayerMount { .. }
                | M2GpuPlacementOwner::RemotePlayerBody { .. }
                | M2GpuPlacementOwner::RemotePlayerMount { .. }
                | M2GpuPlacementOwner::CreatureBody { .. }
                | M2GpuPlacementOwner::Transport { .. }
                | M2GpuPlacementOwner::PlayerItem { .. } => None,
            })
            .collect::<Vec<_>>();
        let mounted_guids = self
            .placements
            .iter()
            .filter_map(|placement| match placement.owner {
                M2GpuPlacementOwner::PlayerMount { guid }
                | M2GpuPlacementOwner::RemotePlayerMount { guid } => Some(guid),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut rider_transforms = Vec::with_capacity(mounted_guids.len());
        let mut item_transforms = Vec::with_capacity(requested_items.len());
        let mut visual_transforms = Vec::with_capacity(requested_visuals.len());
        let mut glue_attachment_ids = self
            .placements
            .iter()
            .filter_map(|placement| placement.glue_parent_attachment)
            .collect::<Vec<_>>();
        glue_attachment_ids.sort_unstable();
        glue_attachment_ids.dedup();
        let mut glue_attachment_transforms = Vec::with_capacity(glue_attachment_ids.len());
        update_model_distance_sort_flags(
            &self.placements,
            &self.sources,
            &mut self.model_distance_sort,
        );
        for (placement_index, placement) in self.placements.iter_mut().enumerate() {
            if let Some(attachment_id) = placement.glue_parent_attachment {
                let parent = glue_attachment_transforms
                    .iter()
                    .find_map(|(id, transform)| (*id == attachment_id).then_some(*transform))
                    .ok_or(RuntimeTerrainFrameError::MissingGlueM2AttachmentPose {
                        attachment_id,
                    })?;
                let Some(parent) = parent else {
                    continue;
                };
                placement.transform = parent * placement.local_transform;
            }
            if let M2GpuPlacementOwner::PlayerBody { guid }
            | M2GpuPlacementOwner::RemotePlayerBody { guid } = placement.owner
                && mounted_guids.contains(&guid)
            {
                let transform = rider_transforms
                    .iter()
                    .find_map(|(owner_guid, transform)| (*owner_guid == guid).then_some(*transform))
                    .ok_or(RuntimeTerrainFrameError::MissingMountM2AttachmentPose {
                        guid,
                        attachment_id: 0,
                    })?;
                let Some(transform) = transform else {
                    continue;
                };
                placement.transform = transform;
            }
            if let M2GpuPlacementOwner::PlayerItem { guid, point } = placement.owner {
                if rider_transforms
                    .iter()
                    .any(|(owner_guid, transform)| *owner_guid == guid && transform.is_none())
                {
                    continue;
                }
                let transform = item_transforms
                    .iter()
                    .find_map(|(owner_guid, owner_point, transform)| {
                        (*owner_guid == guid && *owner_point == point).then_some(*transform)
                    })
                    .ok_or(RuntimeTerrainFrameError::MissingPlayerM2AttachmentPose {
                        guid,
                        attachment_id: point.id(),
                    })?;
                let Some(transform) = transform else {
                    continue;
                };
                placement.transform = transform;
            }
            if let M2GpuPlacementOwner::PlayerItemVisual {
                guid,
                item_point,
                effect_point,
            } = placement.owner
            {
                if rider_transforms
                    .iter()
                    .any(|(owner_guid, transform)| *owner_guid == guid && transform.is_none())
                {
                    continue;
                }
                let transform = visual_transforms
                    .iter()
                    .find_map(
                        |(owner_guid, owner_item_point, owner_effect_point, transform)| {
                            (*owner_guid == guid
                                && *owner_item_point == item_point
                                && *owner_effect_point == effect_point)
                                .then_some(*transform)
                        },
                    )
                    .ok_or(RuntimeTerrainFrameError::MissingPlayerM2AttachmentPose {
                        guid,
                        attachment_id: effect_point,
                    })?;
                let Some(transform) = transform else {
                    continue;
                };
                placement.transform = transform;
            }
            let Some(source) = self.sources[placement.source_index].as_ref() else {
                continue;
            };
            let Some(playback) = placement.playback.as_mut() else {
                continue;
            };
            let advance =
                playback.clock(&source.model, animation_time_ms, global_time_ms, random)?;
            if let Some(expired) = advance.expired_variation {
                let expired_pose = M2BonePose::compose_with_model_view(
                    source.model.animations(),
                    expired.clock,
                    camera.view() * placement.transform,
                )?;
                append_triggered_events(
                    &mut self.triggered_events,
                    &source.model,
                    placement.owner,
                    placement.transform,
                    &expired_pose,
                    expired.event_window,
                )?;
            }
            let clock = advance.clock;
            let event_window = playback.event_window(animation_time_ms, global_time_ms);
            let model_view = camera.view() * placement.transform;
            let bone_pose =
                M2BonePose::compose_with_model_view(source.model.animations(), clock, model_view)?;
            append_triggered_events(
                &mut self.triggered_events,
                &source.model,
                placement.owner,
                placement.transform,
                &bone_pose,
                event_window,
            )?;
            if matches!(placement.owner, M2GpuPlacementOwner::GlueModel { .. }) {
                for attachment_id in &glue_attachment_ids {
                    let attachment = source.model.attachment(*attachment_id).ok_or_else(|| {
                        RuntimeTerrainFrameError::MissingGlueM2Attachment {
                            model: source.model.path().clone(),
                            attachment_id: *attachment_id,
                        }
                    })?;
                    let transform = bone_pose.attachment_transform(
                        source.model.animations(),
                        attachment,
                        clock,
                        placement.transform,
                    )?;
                    glue_attachment_transforms.push((*attachment_id, transform));
                }
            }
            if let M2GpuPlacementOwner::PlayerMount { guid }
            | M2GpuPlacementOwner::RemotePlayerMount { guid } = placement.owner
            {
                if matches!(placement.owner, M2GpuPlacementOwner::PlayerMount { .. }) {
                    self.mount_camera_sample = Some(sample_mount_camera(
                        &source.model,
                        placement.transform,
                        &bone_pose,
                        animation_time_ms,
                    )?);
                }
                let attachment = source.model.attachment(0).ok_or_else(|| {
                    RuntimeTerrainFrameError::MissingMountM2Attachment {
                        model: source.model.path().clone(),
                        attachment_id: 0,
                    }
                })?;
                let transform = bone_pose.attachment_transform(
                    source.model.animations(),
                    attachment,
                    clock,
                    placement.transform,
                )?;
                rider_transforms.push((guid, transform));
            }
            if let M2GpuPlacementOwner::PlayerBody { guid }
            | M2GpuPlacementOwner::RemotePlayerBody { guid } = placement.owner
            {
                for (_owner_guid, point) in requested_items
                    .iter()
                    .filter(|(owner_guid, _point)| *owner_guid == guid)
                {
                    let attachment = source.model.attachment(point.id()).ok_or_else(|| {
                        RuntimeTerrainFrameError::MissingPlayerM2Attachment {
                            model: source.model.path().clone(),
                            attachment_id: point.id(),
                        }
                    })?;
                    let transform = bone_pose.attachment_transform(
                        source.model.animations(),
                        attachment,
                        clock,
                        placement.transform,
                    )?;
                    item_transforms.push((guid, *point, transform));
                }
            }
            if let M2GpuPlacementOwner::PlayerItem { guid, point } = placement.owner {
                for (_owner_guid, _owner_item_point, effect_point) in requested_visuals
                    .iter()
                    .filter(|(owner_guid, item_point, _)| {
                        *owner_guid == guid && *item_point == point
                    })
                {
                    let attachment = source.model.attachment(*effect_point).ok_or_else(|| {
                        RuntimeTerrainFrameError::MissingPlayerM2Attachment {
                            model: source.model.path().clone(),
                            attachment_id: *effect_point,
                        }
                    })?;
                    let transform = bone_pose.attachment_transform(
                        source.model.animations(),
                        attachment,
                        clock,
                        placement.transform,
                    )?;
                    visual_transforms.push((guid, point, *effect_point, transform));
                }
            }
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
            let light_bank = placement_light_bank(placement.owner);
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
                let mesh = M2ParticleMeshPlan::prepare_transformed_with_particle_color(
                    emitter,
                    pose,
                    simulation.particles(),
                    camera,
                    particle_to_world,
                    placement_color(placement.color).w * placement.opacity,
                    &self.particle_twinkle,
                    placement.particle_colors.as_ref(),
                )?;
                let first_vertex =
                    u32::try_from(self.particle_vertices.len()).map_err(|_source| {
                        solarity_rendering::VulkanError::M2ParticleDrawVertexRange
                    })?;
                let first_index = u32::try_from(self.particle_indices.len())
                    .map_err(|_source| solarity_rendering::VulkanError::M2ParticleDrawIndexRange)?;
                self.particle_draws.push(
                    renderer
                        .prepare_m2_particle_draw(
                            resources.pipeline,
                            resources.texture_set,
                            emitter.blending_type(),
                            emitter.flags(),
                            first_vertex,
                            first_index,
                            &mesh,
                        )?
                        .with_light_bank(light_bank),
                );
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
            let mut instance_color = placement_color(placement.color);
            instance_color.w *= placement.opacity;
            let instance_identity = std::ptr::from_ref(&*placement).addr();
            if let Some(mesh) = source.mesh {
                for (draw_index, resources) in source.draws.iter().enumerate() {
                    let Some(resources) = resources else {
                        continue;
                    };
                    let pose =
                        M2MaterialPose::sample(&source.model, &source.plan, draw_index, clock)?;
                    let draw = &source.plan.draws()[draw_index];
                    let material_state = M2MaterialState::from_material(draw.material());
                    let element_alpha = pose.mesh_color().w * instance_color.w;
                    let runtime_alpha_fade = element_alpha < STOCK_OPAQUE_ALPHA_THRESHOLD
                        && !material_state.blend_enabled();
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
                    let prepared = renderer
                        .prepare_m2_draw(
                            mesh,
                            pipeline,
                            resources.texture_set,
                            &source.plan,
                            draw_index,
                            runtime_alpha_fade,
                            material,
                            bone_offset,
                            0,
                        )?
                        .with_light_bank(light_bank);
                    if draw.transparent_sort_unit() || element_alpha < STOCK_OPAQUE_ALPHA_THRESHOLD
                    {
                        let section_distance = section_distance_key(draw, &bone_pose, model_view)?;
                        let primary_distance = if self.model_distance_sort[placement_index] {
                            m2_model_distance_key(model_view)
                        } else {
                            section_distance
                        };
                        self.transparent_draws.push(M2TransparentDraw {
                            key: M2TransparentSortKey::new(
                                primary_distance,
                                false,
                                draw.batch().priority_plane,
                                section_distance,
                                instance_identity,
                                draw.batch().material_layer,
                            ),
                            draw: prepared,
                        });
                    } else {
                        self.visible_draws.push(prepared);
                    }
                }
            } else {
                debug_assert!(source.draws.iter().all(Option::is_none));
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
                    self.ribbon_draws.push(
                        renderer
                            .prepare_m2_ribbon_draw(
                                pass.pipeline,
                                pass.texture_set,
                                pass.material,
                                first_vertex,
                                &mesh,
                            )?
                            .with_light_bank(light_bank),
                    );
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

    /// Transfers every event generated by the most recent presentation frame.
    pub(super) fn drain_triggered_events(&mut self) -> Vec<RuntimeM2Event> {
        std::mem::take(&mut self.triggered_events)
    }

    /// Takes the controlled mount's marker sample from the latest model pose.
    pub(super) fn take_mount_camera_sample(&mut self) -> Option<RuntimeMountCameraSample> {
        self.mount_camera_sample.take()
    }
}

/// Replays the shared-model distance bit computed after M2/SKIN publication.
///
/// Stock enables the bit for models authored with at least two external view
/// profiles. A child retains it only when its parent has it, so attachments do
/// not silently change the containing model's transparency domain.
fn update_model_distance_sort_flags(
    placements: &[M2GpuPlacement],
    sources: &[Option<M2GpuSource>],
    enabled: &mut Vec<bool>,
) {
    enabled.clear();
    enabled.reserve(placements.len().saturating_sub(enabled.capacity()));
    for (placement_index, placement) in placements.iter().enumerate() {
        let authored = sources
            .get(placement.source_index)
            .and_then(Option::as_ref)
            .is_some_and(|source| source.model.skin_profile_count() >= 2);
        let parent = placement_parent_index(placements, placement_index, placement);
        enabled.push(authored && parent.is_none_or(|index| enabled[index]));
    }
}

/// Resolves the parent-first placement relations already used for transforms.
fn placement_parent_index(
    placements: &[M2GpuPlacement],
    placement_index: usize,
    placement: &M2GpuPlacement,
) -> Option<usize> {
    let preceding = &placements[..placement_index];
    if placement.glue_parent_attachment.is_some() {
        return preceding.iter().rposition(|candidate| {
            matches!(candidate.owner, M2GpuPlacementOwner::GlueModel { .. })
        });
    }
    match placement.owner {
        M2GpuPlacementOwner::PlayerBody { guid } => preceding
            .iter()
            .rposition(|candidate| candidate.owner == M2GpuPlacementOwner::PlayerMount { guid }),
        M2GpuPlacementOwner::RemotePlayerBody { guid } => preceding.iter().rposition(|candidate| {
            candidate.owner == M2GpuPlacementOwner::RemotePlayerMount { guid }
        }),
        M2GpuPlacementOwner::PlayerItem { guid, .. } => preceding.iter().rposition(|candidate| {
            candidate.owner == M2GpuPlacementOwner::PlayerBody { guid }
                || candidate.owner == M2GpuPlacementOwner::RemotePlayerBody { guid }
        }),
        M2GpuPlacementOwner::PlayerItemVisual {
            guid, item_point, ..
        } => preceding.iter().rposition(|candidate| {
            candidate.owner
                == M2GpuPlacementOwner::PlayerItem {
                    guid,
                    point: item_point,
                }
        }),
        M2GpuPlacementOwner::Static(_)
        | M2GpuPlacementOwner::GlueModel { .. }
        | M2GpuPlacementOwner::GluePet
        | M2GpuPlacementOwner::PlayerMount { .. }
        | M2GpuPlacementOwner::RemotePlayerMount { .. }
        | M2GpuPlacementOwner::CreatureBody { .. }
        | M2GpuPlacementOwner::Transport { .. } => None,
    }
}

/// Reproduces the direct `$CMA`/`$CFM` lookup performed on the active mount M2.
fn sample_mount_camera(
    model: &DecodedM2Model,
    model_transform: Mat4,
    bone_pose: &M2BonePose,
    time_ms: f32,
) -> Result<RuntimeMountCameraSample, RuntimeTerrainFrameError> {
    let animations = model.animations();
    if let Some((event_index, event)) = animations
        .events()
        .iter()
        .enumerate()
        .find(|(_index, event)| event.identifier() == *b"$CMA")
    {
        let bone = match event.bone_index() {
            Some(bone_index) => bone_pose
                .transforms()
                .get(bone_index as usize)
                .copied()
                .ok_or_else(|| RuntimeTerrainFrameError::M2EventBoneIndex {
                    model: model.path().clone(),
                    event_index,
                    bone_index,
                })?,
            None => Mat4::IDENTITY,
        };
        let event_world = (model_transform * bone).transform_point3(event.position());
        let origin_world = model_transform.transform_point3(glam::Vec3::ZERO);
        return Ok(RuntimeMountCameraSample {
            animated_height: Some(event_world.z - origin_world.z),
            fixed_height: None,
            time_ms,
        });
    }
    let fixed_height = animations
        .events()
        .iter()
        .find(|event| event.identifier() == *b"$CFM")
        .map(|event| event.position().z);
    Ok(RuntimeMountCameraSample {
        animated_height: None,
        fixed_height,
        time_ms,
    })
}

/// Resolves declaration callbacks through their authored bone and placement.
fn append_triggered_events(
    destination: &mut Vec<RuntimeM2Event>,
    model: &DecodedM2Model,
    owner: M2GpuPlacementOwner,
    model_transform: Mat4,
    bone_pose: &M2BonePose,
    window: M2EventTimeWindow,
) -> Result<(), RuntimeTerrainFrameError> {
    for event_index in triggered_m2_event_indices(model.animations(), window) {
        let event = &model.animations().events()[event_index];
        let bone = match event.bone_index() {
            Some(bone_index) => bone_pose
                .transforms()
                .get(bone_index as usize)
                .copied()
                .ok_or_else(|| RuntimeTerrainFrameError::M2EventBoneIndex {
                    model: model.path().clone(),
                    event_index,
                    bone_index,
                })?,
            None => Mat4::IDENTITY,
        };
        destination.push(RuntimeM2Event {
            identifier: event.identifier(),
            data: event.data(),
            position: (model_transform * bone).transform_point3(event.position()),
            owner_guid: placement_owner_guid(owner),
        });
    }
    Ok(())
}

const fn placement_owner_guid(owner: M2GpuPlacementOwner) -> Option<u64> {
    match owner {
        M2GpuPlacementOwner::Static(_)
        | M2GpuPlacementOwner::GlueModel { .. }
        | M2GpuPlacementOwner::GluePet => None,
        M2GpuPlacementOwner::PlayerBody { guid }
        | M2GpuPlacementOwner::PlayerMount { guid }
        | M2GpuPlacementOwner::RemotePlayerBody { guid }
        | M2GpuPlacementOwner::RemotePlayerMount { guid }
        | M2GpuPlacementOwner::CreatureBody { guid }
        | M2GpuPlacementOwner::Transport { guid }
        | M2GpuPlacementOwner::PlayerItem { guid, .. }
        | M2GpuPlacementOwner::PlayerItemVisual { guid, .. } => Some(guid),
    }
}

/// Maps resident ownership to the three independent banks populated by Glue Lua.
const fn placement_light_bank(owner: M2GpuPlacementOwner) -> M2SceneLightBank {
    match owner {
        M2GpuPlacementOwner::PlayerBody { guid: 0 }
        | M2GpuPlacementOwner::PlayerItem { guid: 0, .. }
        | M2GpuPlacementOwner::PlayerItemVisual { guid: 0, .. } => M2SceneLightBank::Character,
        M2GpuPlacementOwner::GluePet => M2SceneLightBank::Pet,
        M2GpuPlacementOwner::Static(_)
        | M2GpuPlacementOwner::GlueModel { .. }
        | M2GpuPlacementOwner::PlayerBody { .. }
        | M2GpuPlacementOwner::PlayerMount { .. }
        | M2GpuPlacementOwner::RemotePlayerBody { .. }
        | M2GpuPlacementOwner::RemotePlayerMount { .. }
        | M2GpuPlacementOwner::CreatureBody { .. }
        | M2GpuPlacementOwner::Transport { .. }
        | M2GpuPlacementOwner::PlayerItem { .. }
        | M2GpuPlacementOwner::PlayerItemVisual { .. } => M2SceneLightBank::Environment,
    }
}

/// Prepares one complete player character, including equipment and visuals.
fn prepare_character_gpu(
    renderer: &mut VulkanRenderer,
    input: &ResidentPlayerFrameInput<'_>,
    body_owner: M2GpuPlacementOwner,
    random: &mut CrtRand,
) -> Result<Vec<(M2GpuSource, M2GpuPlacement)>, RuntimeTerrainFrameError> {
    let mount = input.mount();
    let world_transform = if let Some(mount) = mount {
        unit_placement_transform(input.world_transform(), mount.object_scale())?
    } else {
        unit_placement_transform(input.world_transform(), input.object_scale())?
    };
    let mut prepared = Vec::new();
    if let Some(mount) = mount {
        if mount.model().attachment(0).is_none() {
            return Err(RuntimeTerrainFrameError::MissingMountM2Attachment {
                model: mount.model().path().clone(),
                attachment_id: 0,
            });
        }
        let resolved = mount
            .textures()
            .iter()
            .map(|texture| match texture {
                ResidentCreatureTexture::Authored(source) => {
                    M2ResolvedTexture::Authored(source.as_ref())
                }
                ResidentCreatureTexture::Unresolved(kind) => M2ResolvedTexture::Unresolved(*kind),
            })
            .collect::<Vec<_>>();
        let source = prepare_gpu_source(
            renderer,
            mount.model(),
            &resolved,
            None,
            M2LocalLightCount::Zero,
        )?;
        let owner = match body_owner {
            M2GpuPlacementOwner::PlayerBody { guid } => M2GpuPlacementOwner::PlayerMount { guid },
            M2GpuPlacementOwner::RemotePlayerBody { guid } => {
                M2GpuPlacementOwner::RemotePlayerMount { guid }
            }
            _ => unreachable!("character preparation requires a player body owner"),
        };
        let placement = unit_gpu_placement(
            0,
            world_transform,
            owner,
            mount.model(),
            mount.animation().animation_id(),
            mount.particle_colors().cloned(),
            random,
        )?;
        // Parent-first insertion lets the current mount bone pose determine
        // the rider transform before the body and its equipment are visited.
        prepared.push((source, placement));
    }
    let resolved = input
        .textures()
        .iter()
        .map(|texture| match texture {
            ResidentPlayerTexture::Authored(source) => M2ResolvedTexture::Authored(source.as_ref()),
            ResidentPlayerTexture::BodyAtlas => M2ResolvedTexture::CharacterAtlas(input.atlas()),
            ResidentPlayerTexture::Unresolved(kind) => M2ResolvedTexture::Unresolved(*kind),
        })
        .collect::<Vec<_>>();
    let source = prepare_gpu_source(
        renderer,
        input.model(),
        &resolved,
        Some(M2GeosetSelection::Character(input.geosets())),
        M2LocalLightCount::Zero,
    )?;
    let body = unit_gpu_placement(
        0,
        world_transform,
        body_owner,
        input.model(),
        input.animation().animation_id(),
        input.particle_colors().cloned(),
        random,
    )?;
    prepared.push((source, body));
    for attachment in input.attachments() {
        if input.model().attachment(attachment.point().id()).is_none() {
            return Err(RuntimeTerrainFrameError::MissingPlayerM2Attachment {
                model: input.model().path().clone(),
                attachment_id: attachment.point().id(),
            });
        }
        let resolved = attachment
            .textures()
            .iter()
            .map(|texture| match texture {
                ResidentPlayerTexture::Authored(source) => {
                    M2ResolvedTexture::Authored(source.as_ref())
                }
                ResidentPlayerTexture::BodyAtlas => {
                    M2ResolvedTexture::CharacterAtlas(input.atlas())
                }
                ResidentPlayerTexture::Unresolved(kind) => M2ResolvedTexture::Unresolved(*kind),
            })
            .collect::<Vec<_>>();
        let source = prepare_gpu_source(
            renderer,
            attachment.model(),
            &resolved,
            None,
            M2LocalLightCount::Zero,
        )?;
        let placement = unit_gpu_placement(
            0,
            world_transform,
            M2GpuPlacementOwner::PlayerItem {
                guid: input.guid(),
                point: attachment.point(),
            },
            attachment.model(),
            0,
            attachment.particle_colors().cloned(),
            random,
        )?;
        prepared.push((source, placement));
        for effect in attachment.visual_effects() {
            let resolved = effect
                .textures()
                .iter()
                .map(|texture| match texture {
                    ResidentPlayerTexture::Authored(source) => {
                        M2ResolvedTexture::Authored(source.as_ref())
                    }
                    ResidentPlayerTexture::BodyAtlas => {
                        M2ResolvedTexture::CharacterAtlas(input.atlas())
                    }
                    ResidentPlayerTexture::Unresolved(kind) => M2ResolvedTexture::Unresolved(*kind),
                })
                .collect::<Vec<_>>();
            let source = prepare_gpu_source(
                renderer,
                effect.model(),
                &resolved,
                None,
                M2LocalLightCount::Zero,
            )?;
            let placement = unit_gpu_placement(
                0,
                world_transform,
                M2GpuPlacementOwner::PlayerItemVisual {
                    guid: input.guid(),
                    item_point: attachment.point(),
                    effect_point: effect.point(),
                },
                effect.model(),
                0,
                None,
                random,
            )?;
            prepared.push((source, placement));
        }
    }
    Ok(prepared)
}

/// Creates placement-local animation and effect histories for one living M2.
fn unit_gpu_placement(
    source_index: usize,
    transform: Mat4,
    owner: M2GpuPlacementOwner,
    model: &DecodedM2Model,
    animation_id: u16,
    particle_colors: Option<M2ParticleColorReplacement>,
    random: &mut CrtRand,
) -> Result<M2GpuPlacement, RuntimeTerrainFrameError> {
    let playback = M2Playback::new(model, animation_id, random)?;
    let particles = model
        .animations()
        .particles()
        .iter()
        .map(|_emitter| {
            let first = u32::from(random.next_u15());
            let second = u32::from(random.next_u15());
            M2ParticleSimulation::new(first << 16 | second)
        })
        .collect();
    let ribbons = model
        .animations()
        .ribbons()
        .iter()
        .map(M2RibbonTrail::new)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(M2GpuPlacement {
        source_index,
        local_transform: transform,
        transform,
        glue_parent_attachment: None,
        owner,
        flags: 0,
        color: [255; 4],
        opacity: 1.0,
        particle_colors,
        playback,
        particles,
        ribbons,
    })
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

/// Converts authoritative unit movement into the character's model matrix.
fn unit_placement_transform(
    transform: WorldTransform,
    object_scale: f32,
) -> Result<Mat4, RuntimeTerrainFrameError> {
    if !transform.position().is_finite()
        || !transform.orientation().is_finite()
        || !object_scale.is_finite()
        || object_scale <= 0.0
    {
        return Err(RuntimeTerrainFrameError::InvalidUnitM2Transform);
    }
    let matrix = Mat4::from_translation(transform.position())
        * Mat4::from_rotation_z(transform.orientation())
        * Mat4::from_scale(glam::Vec3::splat(object_scale));
    if !matrix.is_finite() || matrix.determinant().abs() <= f32::EPSILON {
        return Err(RuntimeTerrainFrameError::InvalidUnitM2Transform);
    }
    Ok(matrix)
}

/// Converts a GameObject movement parent into the ordinary M2 world basis.
fn transport_placement_transform(
    transform: WorldTransform,
    object_scale: f32,
) -> Result<Mat4, RuntimeTerrainFrameError> {
    if !transform.position().is_finite()
        || !transform.orientation().is_finite()
        || !object_scale.is_finite()
        || object_scale <= 0.0
    {
        return Err(RuntimeTerrainFrameError::InvalidTransportM2Transform);
    }
    let matrix = Mat4::from_translation(transform.position())
        * Mat4::from_rotation_z(transform.orientation())
        * Mat4::from_scale(glam::Vec3::splat(object_scale));
    if !matrix.is_finite() || matrix.determinant().abs() <= f32::EPSILON {
        return Err(RuntimeTerrainFrameError::InvalidTransportM2Transform);
    }
    Ok(matrix)
}

/// Selects the stable generic GameObject sequence recovered at `FUN_00710460`.
const fn transport_animation_id(state: u8) -> u16 {
    if state == 1 { 147 } else { 149 }
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
    let resolved = source
        .textures()
        .iter()
        .map(|texture| match texture {
            ResidentM2Texture::Authored(source) => M2ResolvedTexture::Authored(source.as_ref()),
            ResidentM2Texture::Replaceable(kind) => M2ResolvedTexture::Unresolved(*kind),
        })
        .collect::<Vec<_>>();
    let plan = Arc::new(M2MeshPlan::prepare(model, STOCK_HIGH_CAPABILITY_PROFILE)?);
    for draw in plan.draws() {
        for binding in draw.texture_bindings() {
            let texture = resolved
                .get(usize::from(binding.texture_index()))
                .ok_or_else(|| RuntimeTerrainFrameError::M2TextureIndex {
                    model: model.path().clone(),
                    texture_index: binding.texture_index(),
                })?;
            if let M2ResolvedTexture::Unresolved(kind) = texture {
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
            let texture = resolved.get(usize::from(*texture_index)).ok_or_else(|| {
                RuntimeTerrainFrameError::M2TextureIndex {
                    model: model.path().clone(),
                    texture_index: *texture_index,
                }
            })?;
            if let M2ResolvedTexture::Unresolved(kind) = texture {
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
        let texture = resolved.get(usize::from(texture_index)).ok_or_else(|| {
            RuntimeTerrainFrameError::M2TextureIndex {
                model: model.path().clone(),
                texture_index,
            }
        })?;
        if let M2ResolvedTexture::Unresolved(kind) = texture {
            tracing::debug!(
                path = %model.path(),
                particle_index,
                ?kind,
                "static world M2 omitted until its particle replacement texture is supplied"
            );
            return Ok(None);
        }
    }
    Ok(Some(prepare_gpu_source(
        renderer,
        model,
        &resolved,
        None,
        M2LocalLightCount::Zero,
    )?))
}

/// Publishes immutable model resources using one owner's resolved texture table.
fn prepare_gpu_source(
    renderer: &mut VulkanRenderer,
    model: &Arc<DecodedM2Model>,
    textures: &[M2ResolvedTexture<'_>],
    geosets: Option<M2GeosetSelection<'_>>,
    local_light_count: M2LocalLightCount,
) -> Result<M2GpuSource, RuntimeTerrainFrameError> {
    if textures.len() != model.textures().len() {
        return Err(RuntimeTerrainFrameError::M2TextureTableCount {
            model: model.path().clone(),
            source_count: textures.len(),
            model_count: model.textures().len(),
        });
    }
    let plan = Arc::new(M2MeshPlan::prepare(model, STOCK_HIGH_CAPABILITY_PROFILE)?);
    validate_gpu_texture_coverage(model, &plan, textures, geosets)?;
    let mut upload_indices = Vec::new();
    let mut uploads = Vec::new();
    for (texture_index, texture) in textures.iter().enumerate() {
        if let M2ResolvedTexture::Authored(source_texture) = texture {
            upload_indices.push(texture_index);
            uploads.push(BlpTextureUploadRequest::new(
                source_texture,
                // Build 12340's fixed-function M2 combiners multiply and add
                // sampled byte values directly. An sRGB image view would
                // linearize them first, darkening smoke and suppressing glow.
                BlpColorSpace::Linear,
            ));
        }
    }
    let uploaded = renderer.upload_blp_textures(&uploads)?;
    let mut texture_handles = vec![None; textures.len()];
    for (texture_index, handle) in upload_indices.into_iter().zip(uploaded) {
        texture_handles[texture_index] = Some(M2TextureImageHandle::Blp(handle));
    }
    let mut atlas_handle = None;
    for (texture_index, texture) in textures.iter().enumerate() {
        if let M2ResolvedTexture::CharacterAtlas(atlas) = texture {
            let handle = match atlas_handle {
                Some(handle) => handle,
                None => {
                    let handle = renderer.upload_character_atlas_texture(atlas)?;
                    atlas_handle = Some(handle);
                    handle
                }
            };
            texture_handles[texture_index] = Some(M2TextureImageHandle::CharacterAtlas(handle));
        }
    }

    let mesh = if plan.has_drawable_geometry() {
        Some(renderer.upload_m2_mesh(&plan)?)
    } else {
        None
    };
    let mut texture_requests = Vec::with_capacity(plan.draws().len());
    let mut pipelines = Vec::with_capacity(plan.draws().len());
    for (draw_index, draw) in plan.draws().iter().enumerate() {
        if geosets.is_some_and(|geosets| !geosets.is_visible(draw.geoset_id())) {
            continue;
        }
        let shader = M2ShaderPlan::resolve(model, draw)?;
        let permutation = M2ShaderPermutation::resolve(
            draw,
            local_light_count,
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
        pipelines.push((draw_index, pipeline, runtime_fade_pipeline));

        let mut stages = Vec::with_capacity(draw.texture_bindings().len());
        for binding in draw.texture_bindings() {
            let texture_index = usize::from(binding.texture_index());
            let texture = require_texture_handle(model, textures, &texture_handles, texture_index)?;
            let sampler = renderer.prepare_m2_sampler(&model.textures()[texture_index])?;
            stages.push(sampled_texture(texture, sampler));
        }
        texture_requests.push(texture_set(model, draw_index, &stages)?);
    }
    let texture_sets = renderer.prepare_m2_texture_sets(&texture_requests)?;
    let mut draws = Vec::with_capacity(plan.draws().len());
    draws.resize_with(plan.draws().len(), || None);
    for ((draw_index, pipeline, runtime_fade_pipeline), texture_set) in
        pipelines.into_iter().zip(texture_sets)
    {
        draws[draw_index] = Some(M2GpuDraw {
            pipeline,
            runtime_fade_pipeline,
            texture_set,
        });
    }
    let mut particle_pipelines = Vec::with_capacity(model.animations().particles().len());
    let mut particle_texture_requests = Vec::with_capacity(model.animations().particles().len());
    for (particle_index, emitter) in model.animations().particles().iter().enumerate() {
        let texture_index = ordinary_particle_texture_index(model, particle_index, emitter)?;
        let texture_slot = usize::from(texture_index);
        let texture = require_texture_handle(model, textures, &texture_handles, texture_slot)?;
        let sampler = renderer.prepare_m2_sampler(&model.textures()[texture_slot])?;
        particle_pipelines
            .push(renderer.prepare_m2_particle_pipeline(emitter.blending_type(), emitter.flags())?);
        particle_texture_requests.push(M2TextureSet::One(sampled_texture(texture, sampler)));
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
            let texture = require_texture_handle(model, textures, &texture_handles, texture_slot)?;
            let sampler = renderer.prepare_m2_sampler(&model.textures()[texture_slot])?;
            pass_pipelines.push((pipeline, material));
            pass_textures.push(M2TextureSet::One(sampled_texture(texture, sampler)));
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
    Ok(M2GpuSource {
        model: Arc::clone(model),
        plan,
        mesh,
        draws,
        particles,
        ribbons,
    })
}

/// Rejects selected holes before any renderer registry mutates.
fn validate_gpu_texture_coverage(
    model: &DecodedM2Model,
    plan: &M2MeshPlan,
    textures: &[M2ResolvedTexture<'_>],
    geosets: Option<M2GeosetSelection<'_>>,
) -> Result<(), RuntimeTerrainFrameError> {
    for draw in plan.draws() {
        if geosets.is_some_and(|geosets| !geosets.is_visible(draw.geoset_id())) {
            continue;
        }
        for binding in draw.texture_bindings() {
            require_resolved_texture(model, textures, usize::from(binding.texture_index()))?;
        }
    }
    for emitter in model.animations().ribbons() {
        for texture_index in emitter.texture_indices() {
            require_resolved_texture(model, textures, usize::from(*texture_index))?;
        }
    }
    for (particle_index, emitter) in model.animations().particles().iter().enumerate() {
        let texture_index = ordinary_particle_texture_index(model, particle_index, emitter)?;
        require_resolved_texture(model, textures, usize::from(texture_index))?;
    }
    Ok(())
}

/// Verifies that one selected slot has an owner-provided image source.
fn require_resolved_texture(
    model: &DecodedM2Model,
    textures: &[M2ResolvedTexture<'_>],
    texture_index: usize,
) -> Result<(), RuntimeTerrainFrameError> {
    match textures.get(texture_index) {
        Some(M2ResolvedTexture::Authored(_) | M2ResolvedTexture::CharacterAtlas(_)) => Ok(()),
        Some(M2ResolvedTexture::Unresolved(kind)) => {
            Err(RuntimeTerrainFrameError::M2UnresolvedTexture {
                model: model.path().clone(),
                texture_index,
                kind: *kind,
            })
        }
        None => {
            let texture_index = u16::try_from(texture_index).map_err(|_source| {
                RuntimeTerrainFrameError::M2TextureIndexCapacity {
                    model: model.path().clone(),
                    texture_index,
                }
            })?;
            Err(RuntimeTerrainFrameError::M2TextureIndex {
                model: model.path().clone(),
                texture_index,
            })
        }
    }
}

/// Requires one resolved image handle for a selected visible/effect texture.
fn require_texture_handle(
    model: &DecodedM2Model,
    textures: &[M2ResolvedTexture<'_>],
    handles: &[Option<M2TextureImageHandle>],
    texture_index: usize,
) -> Result<M2TextureImageHandle, RuntimeTerrainFrameError> {
    let diagnostic_index = u16::try_from(texture_index).map_err(|_source| {
        RuntimeTerrainFrameError::M2TextureIndexCapacity {
            model: model.path().clone(),
            texture_index,
        }
    })?;
    let texture =
        textures
            .get(texture_index)
            .ok_or_else(|| RuntimeTerrainFrameError::M2TextureIndex {
                model: model.path().clone(),
                texture_index: diagnostic_index,
            })?;
    match texture {
        M2ResolvedTexture::Unresolved(kind) => Err(RuntimeTerrainFrameError::M2UnresolvedTexture {
            model: model.path().clone(),
            texture_index,
            kind: *kind,
        }),
        M2ResolvedTexture::Authored(_) | M2ResolvedTexture::CharacterAtlas(_) => handles
            .get(texture_index)
            .copied()
            .flatten()
            .ok_or_else(|| RuntimeTerrainFrameError::M2TextureIndex {
                model: model.path().clone(),
                texture_index: diagnostic_index,
            }),
    }
}

/// Couples a typed image source to the model declaration's stock sampler.
fn sampled_texture(
    image: M2TextureImageHandle,
    sampler: solarity_rendering::M2SamplerHandle,
) -> M2SampledTexture {
    match image {
        M2TextureImageHandle::Blp(handle) => M2SampledTexture::new(handle, sampler),
        M2TextureImageHandle::CharacterAtlas(handle) => {
            M2SampledTexture::character_atlas(handle, sampler)
        }
    }
}

/// Closes the stock shader's one-or-two-stage material domain without panic.
fn texture_set(
    model: &DecodedM2Model,
    draw_index: usize,
    stages: &[M2SampledTexture],
) -> Result<M2TextureSet, RuntimeTerrainFrameError> {
    match stages {
        [stage] => Ok(M2TextureSet::One(*stage)),
        [first, second] => Ok(M2TextureSet::Two([*first, *second])),
        _ => Err(RuntimeTerrainFrameError::M2TextureStageCount {
            model: model.path().clone(),
            draw_index,
            stage_count: stages.len(),
        }),
    }
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
