//! Renderer-local resources for the shared resident placed-M2 scene.

#[cfg(test)]
#[path = "../../../tests/application/model_playback.rs"]
mod model_playback_tests;

mod streaming;

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};

use glam::Mat4;
use solarity_asset::{
    AnimationDataCatalog, AssetPath, BlpTextureSource, DecodedM2Model, M2ModelAnimationMode,
    M2ParticleEmitter,
};
use solarity_ecs::WorldTransform;
use solarity_rendering::{
    BlpColorSpace, BlpTextureUploadRequest, CharacterAtlasTexture, CharacterAttachmentPoint,
    CharacterGeosetPlan, CreatureGeosetPlan, M2AnimationClock, M2BonePose, M2CameraEffectScale,
    M2DrawCall, M2EffectOrder, M2ElementAlphaState, M2EventTimeWindow, M2FingerPoseHands,
    M2LocalLightCount, M2MaterialPose, M2MaterialState, M2MaterialUniform, M2MeshHandle,
    M2MeshPlan, M2ModelOrientation, M2ModelSequenceBlend, M2ModelSequenceTimer,
    M2ParticleColorReplacement, M2ParticleMeshPlan, M2ParticleMeshPlanError,
    M2ParticlePipelineHandle, M2ParticlePose, M2ParticlePreparedDraw, M2ParticleRenderVertex,
    M2ParticleSimulation, M2ParticleSpirvCompiler, M2ParticleSpirvProgram, M2ParticleTwinkleTable,
    M2PipelineHandle, M2PreparedDraw, M2RibbonControlPoint, M2RibbonMeshPlan,
    M2RibbonPipelineHandle, M2RibbonPose, M2RibbonPreparedDraw, M2RibbonRenderVertex,
    M2RibbonSpirvCompiler, M2RibbonSpirvProgram, M2RibbonTrail, M2SampledTexture, M2SceneLightBank,
    M2SequenceStartPhase, M2ShaderPermutation, M2ShaderPlan, M2ShadowFiltering,
    M2ShadowPermutation, M2SpirvCompiler, M2SpirvKey, M2SpirvProgram, M2TextureImageHandle,
    M2TextureSet, M2TextureSetHandle, M2TransparentPass, M2TransparentSortKey, VulkanRenderer,
    WorldCameraFrame, WorldFrustum, compare_m2_transparent, m2_model_distance_key,
    m2_section_distance_key, sample_m2_lights_into, triggered_m2_event_indices,
};

use crate::application::player_coordinator::{
    ResidentCreatureFrameInput, ResidentCreatureGeosets, ResidentCreatureTexture,
    ResidentGlueCharacterFrameInput, ResidentPlayerAttachment, ResidentPlayerFrameInput,
    ResidentPlayerTexture,
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

/// Initial `particleDensity` CVar registered by the stock UI environment.
const STOCK_DEFAULT_PARTICLE_DENSITY: f32 = 1.0;

/// Disables build-12340's camera-distance emission reduction.
const PARTICLE_IGNORE_DISTANCE_LOD: u32 = 0x0040_0000;

/// One selected M2/SKIN generation uploaded once for all of its placements.
#[derive(Clone)]
struct M2GpuSource {
    model: Arc<DecodedM2Model>,
    plan: Arc<M2MeshPlan>,
    /// Character-eye billboard bones retained in model orientation.
    model_oriented_billboard_bones: Vec<bool>,
    mesh: Option<M2MeshHandle>,
    draws: Vec<Option<M2GpuDraw>>,
    particles: Vec<M2GpuParticle>,
    ribbons: Vec<Vec<M2GpuRibbonPass>>,
}

/// Immutable renderer resources for one Glue model prepared before activation.
///
/// Keeping this separate from [`M2Frame`] lets startup upload AccountLogin
/// before constructing the hidden live owner that the cinematic advances.
#[derive(Clone)]
pub(in crate::application) struct M2GlueGpuSource {
    source: M2GpuSource,
}

impl M2GlueGpuSource {
    /// Clones immutable renderer handles for a new live placement generation.
    pub(in crate::application) fn instantiate(&self) -> Self {
        Self {
            source: self.source.clone(),
        }
    }
}

/// Fixed renderer objects paired with one exact SKIN material batch.
#[derive(Clone)]
struct M2GpuDraw {
    pipeline: M2PipelineHandle,
    runtime_fade_pipeline: Option<M2PipelineHandle>,
    texture_set: M2TextureSetHandle,
}

/// Shared renderer objects for one ordinary particle declaration.
#[derive(Clone)]
struct M2GpuParticle {
    pipeline: M2ParticlePipelineHandle,
    texture_set: M2TextureSetHandle,
}

/// One stock ribbon pass pairing parallel material and texture entries.
#[derive(Clone)]
struct M2GpuRibbonPass {
    pipeline: M2RibbonPipelineHandle,
    texture_set: M2TextureSetHandle,
    material: solarity_asset::M2Material,
}

/// One stock transparent scene element retained with its outer pass selection.
struct M2TransparentElement {
    pass: M2TransparentPass,
    key: M2TransparentSortKey,
    draw: M2TransparentDrawIndex,
}

/// Typed payload dispatched after the stock common scene-element sort.
#[derive(Clone, Copy)]
enum M2TransparentDrawIndex {
    Mesh(usize),
    Particle(usize),
    Ribbon(usize),
}

/// Exact per-instance state required by later animation and material assembly.
struct M2GpuPlacement {
    source_index: usize,
    /// Placement-local transform retained across animated parent resolution.
    local_transform: Mat4,
    transform: Mat4,
    /// Local reflection paired with the source's Vulkan front-face state.
    orientation: M2ModelOrientation,
    /// Stock item-display sequence relationship to another placed model.
    animation_binding: M2AnimationBinding,
    /// Stock parent attachment used by Glue character preview models.
    glue_parent_attachment: Option<u32>,
    owner: M2GpuPlacementOwner,
    flags: u16,
    color: [u8; 4],
    opacity: f32,
    particle_colors: Option<M2ParticleColorReplacement>,
    playback: Option<M2Playback>,
    particles: Vec<M2ParticlePlacement>,
    ribbons: Vec<M2RibbonTrail>,
}

/// Sequence source selected by build-12340 item-display component flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum M2AnimationBinding {
    /// Advance the model's own animation selection.
    #[default]
    Independent,
    /// Follow the owning character's active animation and variation.
    Character,
    /// Attachment six follows the sibling installed at attachment five.
    OppositeShoulder,
    /// Follow attachment five, or the character when that sibling is absent.
    OppositeShoulderOrCharacter,
}

/// One placement-local simulation plus an unsupported-path containment latch.
struct M2ParticlePlacement {
    simulation: M2ParticleSimulation,
    unsupported: Option<M2UnsupportedParticle>,
    reported: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum M2UnsupportedParticle {
    EmitterType(u8),
    BehaviorFlags(u32),
}

impl M2ParticlePlacement {
    fn new(emitter: &M2ParticleEmitter, simulation: M2ParticleSimulation) -> Self {
        let unsupported = classify_particle_support(
            emitter.emitter_type(),
            M2ParticleSimulation::unsupported_behavior_flags(emitter),
        );
        Self {
            simulation,
            unsupported,
            reported: false,
        }
    }

    fn diagnostic(&mut self, model: &AssetPath, particle_index: usize) -> Option<String> {
        let unsupported = self.unsupported?;
        if self.reported {
            return None;
        }
        self.reported = true;
        Some(match unsupported {
            M2UnsupportedParticle::EmitterType(emitter_type) => format!(
                "contained M2 particle emitter: model={model} particle={particle_index} unsupported emitter type {emitter_type}"
            ),
            M2UnsupportedParticle::BehaviorFlags(flags) => format!(
                "contained M2 particle emitter: model={model} particle={particle_index} unsupported behavior flags 0x{flags:08X}"
            ),
        })
    }
}

fn classify_particle_support(
    emitter_type: u8,
    unsupported_flags: u32,
) -> Option<M2UnsupportedParticle> {
    match emitter_type {
        1 | 2 if unsupported_flags == 0 => None,
        1 | 2 => Some(M2UnsupportedParticle::BehaviorFlags(unsupported_flags)),
        emitter_type => Some(M2UnsupportedParticle::EmitterType(emitter_type)),
    }
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
    /// M2Shared's generated opaque white texture for an empty filename.
    StockWhite,
    /// Texture.cpp's generated opaque green texture for a failed request.
    StockFailure,
    /// A replacement category not supplied by this presentation owner.
    Unresolved(solarity_asset::M2TextureKind),
}

/// One Glue-environment M2 texture after stock loader fallback resolution.
#[derive(Clone)]
pub(in crate::application) enum GlueM2Texture {
    /// A shared authored BLP selected through archive precedence.
    Authored(Arc<BlpTextureSource>),
    /// M2Shared's generated opaque white texture for an empty filename.
    StockWhite,
    /// Texture.cpp's generated opaque green texture for a failed request.
    StockFailure,
}

/// CPU-only M2 generation prepared away from the presentation thread.
pub(in crate::application) struct M2CpuSource {
    plan: Arc<M2MeshPlan>,
    mesh_programs: HashMap<M2SpirvKey, M2SpirvProgram>,
    particle_programs: HashMap<M2MaterialState, M2ParticleSpirvProgram>,
    ribbon_programs: HashMap<M2MaterialState, M2RibbonSpirvProgram>,
}

/// Driver pipeline work for one worker-prepared Glue M2 source.
///
/// SPIR-V compilation is already complete. Keeping a cursor over the remaining
/// Vulkan pipelines lets the presentation owner admit one potentially costly
/// driver creation per frame instead of one 20-30 ms burst when a character
/// with several equipped models first becomes visible.
pub(in crate::application) struct M2GluePipelineWarmup {
    programs: VecDeque<M2GluePipelineProgram>,
}

enum M2GluePipelineProgram {
    Mesh(M2SpirvProgram, M2ModelOrientation),
    Particle(M2ParticleSpirvProgram),
    Ribbon(M2RibbonSpirvProgram),
}

impl M2GluePipelineWarmup {
    /// Captures immutable programs for one exact model orientation.
    pub(in crate::application) fn new(
        source: &M2CpuSource,
        orientation: M2ModelOrientation,
    ) -> Self {
        let mut programs = VecDeque::with_capacity(
            source.mesh_programs.len()
                + source.particle_programs.len()
                + source.ribbon_programs.len(),
        );
        programs.extend(
            source
                .mesh_programs
                .values()
                .cloned()
                .map(|program| M2GluePipelineProgram::Mesh(program, orientation)),
        );
        programs.extend(
            source
                .particle_programs
                .values()
                .cloned()
                .map(M2GluePipelineProgram::Particle),
        );
        programs.extend(
            source
                .ribbon_programs
                .values()
                .cloned()
                .map(M2GluePipelineProgram::Ribbon),
        );
        Self { programs }
    }

    /// Creates at most one driver pipeline and reports full residency.
    pub(in crate::application) fn service_one(
        &mut self,
        renderer: &mut VulkanRenderer,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        let Some(program) = self.programs.pop_front() else {
            return Ok(true);
        };
        match program {
            M2GluePipelineProgram::Mesh(program, orientation) => {
                renderer.prepare_precompiled_oriented_m2_pipeline(&program, orientation)?;
            }
            M2GluePipelineProgram::Particle(program) => {
                renderer.prepare_precompiled_m2_particle_pipeline(&program)?;
            }
            M2GluePipelineProgram::Ribbon(program) => {
                renderer.prepare_precompiled_m2_ribbon_pipeline(&program)?;
            }
        }
        Ok(self.programs.is_empty())
    }
}

/// Exact immutable CPU generation shared by Glue character placements.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::application) struct M2GlueCpuSourceKey {
    path: AssetPath,
    local_light_count: M2LocalLightCount,
}

impl M2GlueCpuSourceKey {
    /// Identifies one model and stock local-light shader permutation.
    pub(in crate::application) fn new(
        path: AssetPath,
        local_light_count: M2LocalLightCount,
    ) -> Self {
        Self {
            path,
            local_light_count,
        }
    }

    /// Returns the exact shader light count represented by this key.
    pub(in crate::application) const fn local_light_count(&self) -> M2LocalLightCount {
        self.local_light_count
    }

    /// Returns the canonical archive model path represented by this key.
    pub(in crate::application) const fn path(&self) -> &AssetPath {
        &self.path
    }
}

/// Process-wide worker bytecode indexed by complete stock shader identity.
#[derive(Default)]
struct M2GlueProgramCache {
    mesh: HashMap<M2SpirvKey, M2SpirvProgram>,
    particles: HashMap<M2MaterialState, M2ParticleSpirvProgram>,
    ribbons: HashMap<M2MaterialState, M2RibbonSpirvProgram>,
}

/// Shares immutable worker results across every finite Glue backdrop.
static M2_PROGRAMS: OnceLock<Mutex<M2GlueProgramCache>> = OnceLock::new();

/// Builds the immutable mesh plan and every required shader permutation.
pub(in crate::application) fn prepare_m2_cpu_source(
    model: &Arc<DecodedM2Model>,
    local_light_count: M2LocalLightCount,
) -> Result<M2CpuSource, RuntimeTerrainFrameError> {
    let plan = Arc::new(M2MeshPlan::prepare(model, STOCK_HIGH_CAPABILITY_PROFILE)?);
    let cache_lock = M2_PROGRAMS.get_or_init(|| Mutex::new(M2GlueProgramCache::default()));
    let mut mesh_compiler = None;
    let mut mesh_programs = HashMap::new();
    for draw in plan.draws() {
        let shader = M2ShaderPlan::resolve(model, draw)?;
        let permutation = M2ShaderPermutation::resolve(
            draw,
            local_light_count,
            M2ShadowPermutation::Disabled,
            M2ShadowFiltering::Direct,
        );
        let material = M2MaterialState::from_material(draw.material());
        let shaders = [
            Some(shader),
            (!material.blend_enabled()).then(|| shader.with_runtime_alpha_fade()),
        ];
        for shader in shaders.into_iter().flatten() {
            let key = M2SpirvKey::new(shader, permutation);
            if let std::collections::hash_map::Entry::Vacant(entry) = mesh_programs.entry(key) {
                let cached = match cache_lock.lock() {
                    Ok(cache) => cache.mesh.get(&key).cloned(),
                    Err(poisoned) => poisoned.into_inner().mesh.get(&key).cloned(),
                };
                let program = if let Some(program) = cached {
                    program
                } else {
                    let compiler = match mesh_compiler.as_ref() {
                        Some(compiler) => compiler,
                        None => mesh_compiler.insert(M2SpirvCompiler::new()?),
                    };
                    let program = compiler.compile(shader, permutation)?;
                    match cache_lock.lock() {
                        Ok(mut cache) => cache.mesh.entry(key).or_insert(program).clone(),
                        Err(poisoned) => poisoned
                            .into_inner()
                            .mesh
                            .entry(key)
                            .or_insert(program)
                            .clone(),
                    }
                };
                entry.insert(program);
            }
        }
    }

    let mut particle_programs = HashMap::new();
    if !model.animations().particles().is_empty() {
        let mut particle_compiler = None;
        for emitter in model.animations().particles() {
            let material = M2MaterialState::from_particle(emitter.blending_type(), emitter.flags());
            if let std::collections::hash_map::Entry::Vacant(entry) =
                particle_programs.entry(material)
            {
                let cached = match cache_lock.lock() {
                    Ok(cache) => cache.particles.get(&material).cloned(),
                    Err(poisoned) => poisoned.into_inner().particles.get(&material).cloned(),
                };
                let program = if let Some(program) = cached {
                    program
                } else {
                    let compiler = match particle_compiler.as_ref() {
                        Some(compiler) => compiler,
                        None => particle_compiler.insert(M2ParticleSpirvCompiler::new()?),
                    };
                    let program = compiler.compile(material)?;
                    match cache_lock.lock() {
                        Ok(mut cache) => cache.particles.entry(material).or_insert(program).clone(),
                        Err(poisoned) => poisoned
                            .into_inner()
                            .particles
                            .entry(material)
                            .or_insert(program)
                            .clone(),
                    }
                };
                entry.insert(program);
            }
        }
    }

    let mut ribbon_programs = HashMap::new();
    if !model.animations().ribbons().is_empty() {
        let mut ribbon_compiler = None;
        for emitter in model.animations().ribbons() {
            for material_index in emitter.material_indices() {
                let material =
                    M2MaterialState::from_material(model.materials()[usize::from(*material_index)]);
                if let std::collections::hash_map::Entry::Vacant(entry) =
                    ribbon_programs.entry(material)
                {
                    let cached = match cache_lock.lock() {
                        Ok(cache) => cache.ribbons.get(&material).cloned(),
                        Err(poisoned) => poisoned.into_inner().ribbons.get(&material).cloned(),
                    };
                    let program = if let Some(program) = cached {
                        program
                    } else {
                        let compiler = match ribbon_compiler.as_ref() {
                            Some(compiler) => compiler,
                            None => ribbon_compiler.insert(M2RibbonSpirvCompiler::new()?),
                        };
                        let program = compiler.compile(material)?;
                        match cache_lock.lock() {
                            Ok(mut cache) => {
                                cache.ribbons.entry(material).or_insert(program).clone()
                            }
                            Err(poisoned) => poisoned
                                .into_inner()
                                .ribbons
                                .entry(material)
                                .or_insert(program)
                                .clone(),
                        }
                    };
                    entry.insert(program);
                }
            }
        }
    }
    Ok(M2CpuSource {
        plan,
        mesh_programs,
        particle_programs,
        ribbon_programs,
    })
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
pub(in crate::application) struct M2Playback {
    animation_id: u16,
    sequence: usize,
    sequence_duration_ms: f32,
    cycle_count: u32,
    cycle_started_ms: f32,
    has_variations: bool,
    previous_event_elapsed_ms: f32,
    previous_global_event_elapsed_ms: f32,
    event_timeline_started: bool,
    /// Explicit Model calls use native integer scene timers and event intervals.
    script_timer: Option<M2ModelSequenceTimer>,
    script_blend: Option<M2ModelSequenceBlend>,
    script_mode: M2ModelAnimationMode,
    scene_time_ms: u32,
    previous_event_scene_time_ms: u32,
}

/// Current bone clock plus an expired variation tail awaiting event dispatch.
struct M2PlaybackAdvance {
    clock: M2AnimationClock,
    expired_variations: Vec<M2ExpiredVariation>,
}

/// Final event interval and pose clock from one replaced sequence variation.
struct M2ExpiredVariation {
    clock: M2AnimationClock,
    event_window: M2EventTimeWindow,
}

/// Cross-model animation identity copied by stock equipment components.
#[derive(Clone, Copy)]
struct M2PlaybackSynchronization {
    animation_id: u16,
    variation_index: u16,
    cycle_count: u32,
    cycle_started_ms: f32,
    previous_global_event_elapsed_ms: f32,
}

impl M2Playback {
    /// Starts a newly resident world owner against the existing scene clock.
    /// Native 0x00826B00 anchors sequence construction to that clock; loading a
    /// neighbor must not age its new local sequence from world entry time zero.
    fn new_at(
        model: &DecodedM2Model,
        animation_id: u16,
        scene_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<Option<Self>, RuntimeTerrainFrameError> {
        let mut playback = Self::new(model, animation_id, random)?;
        if let Some(playback) = playback.as_mut() {
            playback.cycle_started_ms = scene_time_ms;
            playback.scene_time_ms = scene_time_ms as u32;
            playback.previous_event_scene_time_ms = scene_time_ms as u32;
        }
        Ok(playback)
    }

    /// Selects one base animation and consumes its authored cycle-count roll.
    pub(in crate::application) fn new(
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
                script_timer: None,
                script_blend: None,
                script_mode: M2ModelAnimationMode::Forward,
                scene_time_ms: 0,
                previous_event_scene_time_ms: 0,
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
            script_timer: None,
            script_blend: None,
            script_mode: M2ModelAnimationMode::Forward,
            scene_time_ms: 0,
            previous_event_scene_time_ms: 0,
        }))
    }

    /// Applies each Model Lua request without replacing the mutable M2 owner.
    pub(in crate::application) fn apply_model_sequence(
        &mut self,
        model: &DecodedM2Model,
        catalog: &AnimationDataCatalog,
        requested_animation: u32,
        time_offset_ms: i32,
        scene_time_ms: u32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        // 0x00832840 refuses to clear bone zero. The -1 animation sentinel
        // therefore leaves a Model widget's primary timer and RNG untouched.
        let animations = model.animations();
        if requested_animation == u32::MAX || animations.bones().is_empty() {
            return Ok(());
        }
        let Some(resolved) = animations.resolve_model_animation(catalog, requested_animation)
        else {
            return Ok(());
        };
        let animation_id = resolved.animation_id();
        let sequence = animations
            .select_model_sequence(animation_id, random.next_u15())
            .ok_or_else(|| RuntimeTerrainFrameError::M2AnimationSelection {
                model: model.path().clone(),
                animation_id,
            })?;
        if animations.is_sequence_available(sequence) != Some(true) {
            // Native selection queues unavailable external animation data
            // after consuming its variation roll. The previous timer survives.
            return Ok(());
        }
        let timer = M2ModelSequenceTimer::new(
            &animations.sequences()[sequence],
            resolved.mode(),
            // 0x00826B00 reads the owning scene clock at the request, even
            // when this model has not been sampled while its widget is hidden.
            scene_time_ms,
            time_offset_ms,
            random.next_u15(),
            M2SequenceStartPhase::BeforeSceneUpdate,
        );
        self.animation_id = animation_id;
        self.sequence = sequence;
        self.sequence_duration_ms = animations.sequences()[sequence].duration_ms() as f32;
        self.cycle_count = timer.cycle_count();
        self.cycle_started_ms = timer.start_time_ms() as f32;
        self.has_variations = animations.sequences()[sequence].variation_index() != 0
            || animations.sequences()[sequence].variation_next().is_some();
        self.script_timer = Some(timer);
        self.script_blend = None;
        self.script_mode = resolved.mode();
        Ok(())
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
        self.scene_time_ms = animation_time_ms as u32;
        if let Some(timer) = self.script_timer {
            return self.advance_model_timer(model, timer, global_time_ms, random);
        }
        let elapsed_ms = (animation_time_ms - self.cycle_started_ms).max(0.0);
        let selected_span_ms = self.sequence_duration_ms * self.cycle_count as f32;
        let mut expired_variations = Vec::new();
        if self.has_variations && self.sequence_duration_ms > 0.0 && elapsed_ms >= selected_span_ms
        {
            // Stock finishes the old sequence's event interval before replacing
            // its timer. Bone-relative callbacks from that tail must also use
            // the old sequence's terminal pose, not the newly selected pose.
            expired_variations.push(M2ExpiredVariation {
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
            // This legacy gameplay path still restarts at the current frame.
            // Explicit Model calls below retain native callback overdue time.
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
            expired_variations,
        })
    }

    /// Dispatches native cycle boundaries before sampling the remaining frame interval.
    fn advance_model_timer(
        &mut self,
        model: &DecodedM2Model,
        mut timer: M2ModelSequenceTimer,
        global_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<M2PlaybackAdvance, RuntimeTerrainFrameError> {
        let animations = model.animations();
        let mut expired_variations = Vec::new();
        while self.has_variations {
            let Some(boundary) =
                timer.next_loop_boundary_ms(self.previous_event_scene_time_ms, self.scene_time_ms)
            else {
                break;
            };
            expired_variations.push(M2ExpiredVariation {
                clock: M2AnimationClock::new(
                    self.sequence,
                    timer.animation_time_ms(boundary) as f32,
                    global_time_ms,
                ),
                event_window: M2EventTimeWindow::new(self.sequence, 0.0, 0.0, false, false)
                    .with_scene_timer(timer, self.previous_event_scene_time_ms, boundary),
            });
            self.previous_event_scene_time_ms = boundary;
            let sequence = animations
                .select_model_sequence(self.animation_id, random.next_u15())
                .ok_or_else(|| RuntimeTerrainFrameError::M2AnimationSelection {
                    model: model.path().clone(),
                    animation_id: self.animation_id,
                })?;
            if animations.is_sequence_available(sequence) != Some(true) {
                break;
            }
            // 0x00826C40 keeps an existing secondary while its contribution
            // is strictly above one half. Otherwise the outgoing primary
            // replaces it, using the incoming sequence's blend duration.
            if self
                .script_blend
                .is_none_or(|blend| blend.weight(self.scene_time_ms) <= 0.5)
            {
                self.script_blend = Some(M2ModelSequenceBlend::new(
                    self.sequence,
                    timer,
                    self.scene_time_ms,
                    animations.sequences()[sequence].blend_time_ms(),
                ));
            }
            timer = timer.restart_variation(
                &animations.sequences()[sequence],
                self.script_mode,
                self.scene_time_ms,
                boundary,
                random.next_u15(),
            );
            self.sequence = sequence;
            self.sequence_duration_ms = animations.sequences()[sequence].duration_ms() as f32;
            self.cycle_count = timer.cycle_count();
            self.cycle_started_ms = timer.start_time_ms() as f32;
            self.has_variations = animations.sequences()[sequence].variation_index() != 0
                || animations.sequences()[sequence].variation_next().is_some();
        }
        self.script_timer = Some(timer);
        let mut clock = M2AnimationClock::new(
            self.sequence,
            timer.animation_time_ms(self.scene_time_ms) as f32,
            global_time_ms,
        );
        if let Some(blend) = self.script_blend {
            if blend.weight(self.scene_time_ms) == 0.0 {
                self.script_blend = None;
            } else {
                clock = blend.apply_to_clock(clock, self.scene_time_ms);
            }
        }
        Ok(M2PlaybackAdvance {
            clock,
            expired_variations,
        })
    }

    /// Advances the unwrapped clocks retained exclusively for event crossing.
    fn event_window(&mut self, animation_time_ms: f32, global_time_ms: f32) -> M2EventTimeWindow {
        if let Some(timer) = self.script_timer {
            let window = M2EventTimeWindow::new(self.sequence, 0.0, 0.0, false, false)
                .with_scene_timer(
                    timer,
                    self.previous_event_scene_time_ms,
                    animation_time_ms as u32,
                );
            self.previous_event_scene_time_ms = animation_time_ms as u32;
            return window;
        }
        self.previous_event_scene_time_ms = animation_time_ms as u32;
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

    /// Captures the stock sequence identity shared with an equipment model.
    fn synchronization(&self, model: &DecodedM2Model) -> M2PlaybackSynchronization {
        let variation_index = model
            .animations()
            .sequences()
            .get(self.sequence)
            .map_or(0, |sequence| sequence.variation_index());
        M2PlaybackSynchronization {
            animation_id: self.animation_id,
            variation_index,
            cycle_count: self.cycle_count,
            cycle_started_ms: self.cycle_started_ms,
            previous_global_event_elapsed_ms: self.previous_global_event_elapsed_ms,
        }
    }

    /// Maps another model's active sequence identity onto this model's table.
    fn synchronize_from(
        &mut self,
        model: &DecodedM2Model,
        source: M2PlaybackSynchronization,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let sequence = model
            .animations()
            .select_sequence(source.animation_id, Some(source.variation_index), 0)
            .ok_or_else(|| RuntimeTerrainFrameError::M2AnimationSelection {
                model: model.path().clone(),
                animation_id: source.animation_id,
            })?;
        let identity_changed = self.animation_id != source.animation_id
            || self.sequence != sequence
            || self.cycle_started_ms != source.cycle_started_ms;
        self.animation_id = source.animation_id;
        self.sequence = sequence;
        self.sequence_duration_ms = resolved_sequence_duration(model, sequence)?;
        self.cycle_count = source.cycle_count;
        self.cycle_started_ms = source.cycle_started_ms;
        self.has_variations = model
            .animations()
            .available_variation_count(source.animation_id)
            .ok_or_else(|| RuntimeTerrainFrameError::M2AnimationSelection {
                model: model.path().clone(),
                animation_id: source.animation_id,
            })?
            > 1;
        if identity_changed {
            self.previous_event_elapsed_ms = 0.0;
            self.previous_global_event_elapsed_ms = source.previous_global_event_elapsed_ms;
            self.event_timeline_started = false;
        }
        Ok(())
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
    bone_pose_scratch: M2BonePose,
    bone_transforms: Vec<Mat4>,
    visible_draws: Vec<M2PreparedDraw>,
    transparent_elements: Vec<M2TransparentElement>,
    model_distance_sort: Vec<bool>,
    placement_topology_dirty: bool,
    particle_vertices: Vec<M2ParticleRenderVertex>,
    particle_indices: Vec<u32>,
    particle_sort_indices: Vec<usize>,
    particle_draws: Vec<M2ParticlePreparedDraw>,
    ribbon_vertices: Vec<M2RibbonRenderVertex>,
    ribbon_draws: Vec<M2RibbonPreparedDraw>,
    triggered_events: Vec<RuntimeM2Event>,
    mount_camera_sample: Option<RuntimeMountCameraSample>,
    glue_directional_lights: Vec<solarity_rendering::M2DirectionalLight>,
    glue_point_lights: Vec<solarity_rendering::M2PointLight>,
    requested_items: Vec<(u64, CharacterAttachmentPoint)>,
    requested_visuals: Vec<(u64, CharacterAttachmentPoint, u32)>,
    mounted_guids: Vec<u64>,
    rider_transforms: Vec<(u64, Option<Mat4>)>,
    item_transforms: Vec<(u64, CharacterAttachmentPoint, Option<Mat4>)>,
    visual_transforms: Vec<(u64, CharacterAttachmentPoint, u32, Option<Mat4>)>,
    glue_attachment_ids: Vec<u32>,
    glue_attachment_transforms: Vec<(u32, Option<Mat4>)>,
    character_animation_sync: Vec<(u64, M2PlaybackSynchronization)>,
    shoulder_animation_sync: Vec<(u64, M2PlaybackSynchronization)>,
    recoverable_errors: Vec<String>,
    last_effect_time_ms: Option<f32>,
    pending_glue_playback_advance: Option<M2PlaybackAdvance>,
}

/// Borrowed dynamic streams assembled for one unified world submission.
pub(in crate::application) struct M2VisibleFrame<'frame> {
    pub(in crate::application) bone_transforms: &'frame [Mat4],
    pub(in crate::application) draws: &'frame [M2PreparedDraw],
    pub(in crate::application) particle_vertices: &'frame [M2ParticleRenderVertex],
    pub(in crate::application) particle_indices: &'frame [u32],
    pub(in crate::application) particle_draws: &'frame [M2ParticlePreparedDraw],
    pub(in crate::application) particle_vertex_capacity: usize,
    pub(in crate::application) particle_index_capacity: usize,
    pub(in crate::application) ribbon_vertices: &'frame [M2RibbonRenderVertex],
    pub(in crate::application) ribbon_draws: &'frame [M2RibbonPreparedDraw],
    pub(in crate::application) glue_directional_lights:
        &'frame [solarity_rendering::M2DirectionalLight],
    pub(in crate::application) glue_point_lights: &'frame [solarity_rendering::M2PointLight],
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
            let source = sources.get(placement.source_index()).ok_or(
                RuntimeTerrainFrameError::M2SourceIndex {
                    source_index: placement.source_index(),
                    source_count: sources.len(),
                },
            )?;
            placements.push(streaming::static_gpu_placement(
                placement,
                placement.source_index(),
                source.as_ref(),
                0.0,
                random,
            )?);
        }
        Ok(Self {
            sources,
            placements,
            particle_twinkle,
            animation_started_at: std::time::Instant::now(),
            bone_pose_scratch: M2BonePose::default(),
            bone_transforms: Vec::new(),
            visible_draws: Vec::new(),
            transparent_elements: Vec::new(),
            model_distance_sort: Vec::new(),
            placement_topology_dirty: true,
            particle_vertices: Vec::new(),
            particle_indices: Vec::new(),
            particle_sort_indices: Vec::new(),
            particle_draws: Vec::new(),
            ribbon_vertices: Vec::new(),
            ribbon_draws: Vec::new(),
            triggered_events: Vec::new(),
            mount_camera_sample: None,
            glue_directional_lights: Vec::new(),
            glue_point_lights: Vec::new(),
            requested_items: Vec::new(),
            requested_visuals: Vec::new(),
            mounted_guids: Vec::new(),
            rider_transforms: Vec::new(),
            item_transforms: Vec::new(),
            visual_transforms: Vec::new(),
            glue_attachment_ids: Vec::new(),
            glue_attachment_transforms: Vec::new(),
            character_animation_sync: Vec::new(),
            shoulder_animation_sync: Vec::new(),
            recoverable_errors: Vec::new(),
            last_effect_time_ms: None,
            pending_glue_playback_advance: None,
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
        self.placement_topology_dirty = true;
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

    /// Uploads one immutable Glue model generation without starting playback.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::application) fn prepare_glue_gpu_source(
        renderer: &mut VulkanRenderer,
        model: Arc<DecodedM2Model>,
        textures: &[GlueM2Texture],
        cpu_source: &M2CpuSource,
        local_light_count: M2LocalLightCount,
    ) -> Result<M2GlueGpuSource, RuntimeTerrainFrameError> {
        let resolved = textures
            .iter()
            .map(|texture| match texture {
                GlueM2Texture::Authored(texture) => M2ResolvedTexture::Authored(texture.as_ref()),
                GlueM2Texture::StockWhite => M2ResolvedTexture::StockWhite,
                GlueM2Texture::StockFailure => M2ResolvedTexture::StockFailure,
            })
            .collect::<Vec<_>>();
        let source = prepare_gpu_source_from_cpu(
            renderer,
            &model,
            &resolved,
            None,
            local_light_count,
            cpu_source,
            M2ModelOrientation::Authored,
        )?;
        Ok(M2GlueGpuSource { source })
    }

    /// Activates a pre-uploaded Glue source while retaining its preparation clock.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::application) fn activate_glue_gpu_source(
        gpu_source: M2GlueGpuSource,
        object_index: usize,
        playback: M2Playback,
        model_scale: f32,
        rotation_radians: f32,
        animation_started_at: std::time::Instant,
        particle_twinkle: Arc<M2ParticleTwinkleTable>,
    ) -> Result<Self, RuntimeTerrainFrameError> {
        if !model_scale.is_finite() || model_scale <= 0.0 {
            return Err(RuntimeTerrainFrameError::InvalidGlueM2Scale);
        }
        if !rotation_radians.is_finite() {
            return Err(RuntimeTerrainFrameError::InvalidGlueM2Rotation);
        }
        // Stock ModelFFX native-camera setup (0x95fba0) applies widget yaw
        // and scale to the owning model before publishing its camera.
        let transform = Mat4::from_scale_rotation_translation(
            glam::Vec3::splat(model_scale),
            glam::Quat::from_rotation_z(rotation_radians),
            glam::Vec3::ZERO,
        );
        let M2GlueGpuSource { source, .. } = gpu_source;
        let model = Arc::clone(&source.model);
        let particles = glue_particle_simulations(&model)?;
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
                local_transform: transform,
                transform,
                orientation: M2ModelOrientation::Authored,
                animation_binding: M2AnimationBinding::Independent,
                glue_parent_attachment: None,
                owner: M2GpuPlacementOwner::GlueModel { object_index },
                flags: 0,
                color: [u8::MAX; 4],
                opacity: 1.0,
                particle_colors: None,
                playback: Some(playback),
                particles,
                ribbons,
            }],
            particle_twinkle,
            animation_started_at,
            bone_pose_scratch: M2BonePose::default(),
            bone_transforms: Vec::new(),
            visible_draws: Vec::new(),
            transparent_elements: Vec::new(),
            model_distance_sort: Vec::new(),
            placement_topology_dirty: true,
            particle_vertices: Vec::new(),
            particle_indices: Vec::new(),
            particle_sort_indices: Vec::new(),
            particle_draws: Vec::new(),
            ribbon_vertices: Vec::new(),
            ribbon_draws: Vec::new(),
            triggered_events: Vec::new(),
            mount_camera_sample: None,
            glue_directional_lights: Vec::new(),
            glue_point_lights: Vec::new(),
            requested_items: Vec::new(),
            requested_visuals: Vec::new(),
            mounted_guids: Vec::new(),
            rider_transforms: Vec::new(),
            item_transforms: Vec::new(),
            visual_transforms: Vec::new(),
            glue_attachment_ids: Vec::new(),
            glue_attachment_transforms: Vec::new(),
            character_animation_sync: Vec::new(),
            shoulder_animation_sync: Vec::new(),
            recoverable_errors: Vec::new(),
            last_effect_time_ms: None,
            pending_glue_playback_advance: None,
        })
    }

    /// Replaces the character body and optional pet beneath the retained Glue environment.
    pub(in crate::application) fn replace_glue_character(
        &mut self,
        renderer: &mut VulkanRenderer,
        input: Option<ResidentGlueCharacterFrameInput<'_>>,
        character_light_count: M2LocalLightCount,
        pet_light_count: M2LocalLightCount,
        cpu_sources: &HashMap<M2GlueCpuSourceKey, Arc<M2CpuSource>>,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let Some(input) = input else {
            self.remove_player();
            return Ok(());
        };
        if !input.facing_radians().is_finite() {
            return Err(RuntimeTerrainFrameError::InvalidUnitM2Transform);
        }
        let resolved = input
            .textures()
            .iter()
            .map(|texture| match texture {
                ResidentPlayerTexture::Authored(source) => {
                    M2ResolvedTexture::Authored(source.as_ref())
                }
                ResidentPlayerTexture::StockWhite => M2ResolvedTexture::StockWhite,
                ResidentPlayerTexture::StockFailure => M2ResolvedTexture::StockFailure,
                ResidentPlayerTexture::BodyAtlas => {
                    M2ResolvedTexture::CharacterAtlas(input.atlas())
                }
                ResidentPlayerTexture::Unresolved(kind) => M2ResolvedTexture::Unresolved(*kind),
            })
            .collect::<Vec<_>>();
        let source = prepare_glue_character_gpu_source(
            renderer,
            input.model(),
            &resolved,
            Some(M2GeosetSelection::Character(input.geosets())),
            character_light_count,
            cpu_sources,
            M2ModelOrientation::Authored,
        )?;
        let transform = stock_glue_character_local_transform(input.facing_radians());
        let mut placement = unit_gpu_placement(
            0,
            transform,
            M2GpuPlacementOwner::PlayerBody { guid: 0 },
            input.model(),
            input.animation().animation_id(),
            input.particle_colors().cloned(),
            random,
        )?;
        // CCharacterSelection installs the body with a zero local position,
        // then the ModelFFX scene resolves that local transform beneath the
        // backdrop's authored character stand. Glue backdrops place attachment
        // zero beside their camera (the Human stand is near -225/-81 while the
        // model origin is hundreds of units outside the view).
        placement.glue_parent_attachment = Some(0);
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
                    ResidentPlayerTexture::StockWhite => M2ResolvedTexture::StockWhite,
                    ResidentPlayerTexture::StockFailure => M2ResolvedTexture::StockFailure,
                    ResidentPlayerTexture::BodyAtlas => {
                        M2ResolvedTexture::CharacterAtlas(input.atlas())
                    }
                    ResidentPlayerTexture::Unresolved(kind) => M2ResolvedTexture::Unresolved(*kind),
                })
                .collect::<Vec<_>>();
            let orientation = if attachment.is_model_mirrored() {
                M2ModelOrientation::Mirrored
            } else {
                M2ModelOrientation::Authored
            };
            let source = prepare_glue_character_gpu_source(
                renderer,
                attachment.model(),
                &resolved,
                None,
                character_light_count,
                cpu_sources,
                orientation,
            )?;
            let mut placement = unit_gpu_placement(
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
            placement.orientation = orientation;
            placement.animation_binding = equipment_animation_binding(attachment);
            prepared.push((source, placement));
            for effect in attachment.visual_effects() {
                let resolved = effect
                    .textures()
                    .iter()
                    .map(|texture| match texture {
                        ResidentPlayerTexture::Authored(source) => {
                            M2ResolvedTexture::Authored(source.as_ref())
                        }
                        ResidentPlayerTexture::StockWhite => M2ResolvedTexture::StockWhite,
                        ResidentPlayerTexture::StockFailure => M2ResolvedTexture::StockFailure,
                        ResidentPlayerTexture::BodyAtlas => {
                            M2ResolvedTexture::CharacterAtlas(input.atlas())
                        }
                        ResidentPlayerTexture::Unresolved(kind) => {
                            M2ResolvedTexture::Unresolved(*kind)
                        }
                    })
                    .collect::<Vec<_>>();
                let source = prepare_glue_character_gpu_source(
                    renderer,
                    effect.model(),
                    &resolved,
                    None,
                    character_light_count,
                    cpu_sources,
                    orientation,
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
                    ResidentCreatureTexture::StockWhite => M2ResolvedTexture::StockWhite,
                    ResidentCreatureTexture::StockFailure => M2ResolvedTexture::StockFailure,
                    ResidentCreatureTexture::Unresolved(kind) => {
                        M2ResolvedTexture::Unresolved(*kind)
                    }
                })
                .collect::<Vec<_>>();
            let source = prepare_glue_character_gpu_source(
                renderer,
                pet.model(),
                &resolved,
                pet.geosets().map(M2GeosetSelection::from),
                pet_light_count,
                cpu_sources,
                M2ModelOrientation::Authored,
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
        // Build 12340 changes character textures and geosets without replacing
        // the same-model CM2Model animation timer. Preserve the complete
        // playback/event history only after every replacement resource has
        // prepared successfully, keeping this update transactional.
        let retained_playback = self
            .placements
            .iter()
            .position(|placement| {
                if !matches!(placement.owner, M2GpuPlacementOwner::PlayerBody { guid: 0 }) {
                    return false;
                }
                self.sources
                    .get(placement.source_index)
                    .and_then(Option::as_ref)
                    .is_some_and(|source| source.model.path() == input.model().path())
                    && placement.playback.as_ref().is_some_and(|playback| {
                        playback.animation_id == input.animation().animation_id()
                    })
            })
            .and_then(|index| self.placements[index].playback.take());
        if let Some(playback) = retained_playback
            && let Some((_source, replacement)) = prepared.first_mut()
        {
            replacement.playback = Some(playback);
            tracing::debug!(
                model = %input.model().path(),
                "retained same-model Glue character animation timeline"
            );
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
        facing_radians: f32,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if !facing_radians.is_finite() {
            return Err(RuntimeTerrainFrameError::InvalidUnitM2Transform);
        }
        let placement = self
            .placements
            .iter_mut()
            .find(|placement| {
                matches!(placement.owner, M2GpuPlacementOwner::PlayerBody { guid: 0 })
            })
            .ok_or(RuntimeTerrainFrameError::MissingGlueM2Placement)?;
        let transform = stock_glue_character_local_transform(facing_radians);
        // Retain the stand-local transform. The next draw preparation composes
        // the animated backdrop attachment before sampling body attachments
        // for equipment and item visual effects.
        placement.local_transform = transform;
        placement.transform = transform;
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
                    ResidentCreatureTexture::StockWhite => M2ResolvedTexture::StockWhite,
                    ResidentCreatureTexture::StockFailure => M2ResolvedTexture::StockFailure,
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
                M2ModelOrientation::Authored,
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
        self.placement_topology_dirty = true;
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
        self.placement_topology_dirty = true;
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
        self.placement_topology_dirty = true;
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

    /// Transfers playback back to its widget owner when this GPU frame retires.
    pub(in crate::application) fn take_glue_playback(&mut self) -> Option<M2Playback> {
        self.placements
            .iter_mut()
            .find(|placement| matches!(placement.owner, M2GpuPlacementOwner::GlueModel { .. }))
            .and_then(|placement| placement.playback.take())
    }

    /// Mutates the active sequence while retaining particles, ribbons, and GPU resources.
    pub(in crate::application) fn apply_glue_sequence(
        &mut self,
        catalog: &AnimationDataCatalog,
        animation_id: u32,
        time_offset_ms: i32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let scene_time_ms = self.animation_time_ms() as u32;
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
        playback.apply_model_sequence(
            &source.model,
            catalog,
            animation_id,
            time_offset_ms,
            scene_time_ms,
            random,
        )?;
        self.pending_glue_playback_advance = None;
        Ok(())
    }

    /// Returns the owning model transform for its authored camera.
    pub(in crate::application) fn glue_model_transform(
        &self,
    ) -> Result<Mat4, RuntimeTerrainFrameError> {
        self.placements
            .iter()
            .find(|placement| matches!(placement.owner, M2GpuPlacementOwner::GlueModel { .. }))
            .map(|placement| placement.transform)
            .ok_or(RuntimeTerrainFrameError::MissingGlueM2Placement)
    }

    /// Applies Model scale/yaw without reconstructing animation or emitter state.
    pub(in crate::application) fn update_glue_model_transform(
        &mut self,
        model_scale: f32,
        rotation_radians: f32,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if !model_scale.is_finite() || model_scale <= 0.0 {
            return Err(RuntimeTerrainFrameError::InvalidGlueM2Scale);
        }
        if !rotation_radians.is_finite() {
            return Err(RuntimeTerrainFrameError::InvalidGlueM2Rotation);
        }
        let placement = self
            .placements
            .iter_mut()
            .find(|placement| matches!(placement.owner, M2GpuPlacementOwner::GlueModel { .. }))
            .ok_or(RuntimeTerrainFrameError::MissingGlueM2Placement)?;
        let transform = Mat4::from_scale_rotation_translation(
            glam::Vec3::splat(model_scale),
            glam::Quat::from_rotation_z(rotation_radians),
            glam::Vec3::ZERO,
        );
        placement.local_transform = transform;
        placement.transform = transform;
        Ok(())
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
        let advance = playback.clock(&source.model, animation_time_ms, global_time_ms, random)?;
        let clock = advance.clock;
        self.pending_glue_playback_advance = Some(advance);
        Ok(clock)
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
        effect_scale: M2CameraEffectScale,
        random: &mut CrtRand,
    ) -> Result<M2VisibleFrame<'_>, RuntimeTerrainFrameError> {
        self.bone_transforms.clear();
        self.visible_draws.clear();
        self.transparent_elements.clear();
        self.particle_vertices.clear();
        self.particle_indices.clear();
        self.particle_draws.clear();
        self.ribbon_vertices.clear();
        self.ribbon_draws.clear();
        self.triggered_events.clear();
        self.mount_camera_sample = None;
        self.glue_directional_lights.clear();
        self.glue_point_lights.clear();
        // Stock model instances initialize their effect timestamp to zero, so
        // the first render receives the elapsed local scene clock. The emitter
        // update itself caps that history to one lifetime.
        let elapsed_effect_seconds = self.last_effect_time_ms.map_or_else(
            || animation_time_ms.max(0.0) * 0.001,
            |previous| (animation_time_ms - previous).max(0.0) * 0.001,
        );
        self.last_effect_time_ms = Some(animation_time_ms);
        let effect_delta_seconds = elapsed_effect_seconds;
        let mut particle_vertex_capacity = 0_usize;
        let mut particle_index_capacity = 0_usize;
        if self.placement_topology_dirty {
            self.requested_items.clear();
            self.requested_items
                .extend(
                    self.placements
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
                        }),
                );
            self.requested_visuals.clear();
            self.requested_visuals
                .extend(
                    self.placements
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
                        }),
                );
            self.mounted_guids.clear();
            self.mounted_guids
                .extend(
                    self.placements
                        .iter()
                        .filter_map(|placement| match placement.owner {
                            M2GpuPlacementOwner::PlayerMount { guid }
                            | M2GpuPlacementOwner::RemotePlayerMount { guid } => Some(guid),
                            _ => None,
                        }),
                );
            self.glue_attachment_ids.clear();
            self.glue_attachment_ids.extend(
                self.placements
                    .iter()
                    .filter_map(|placement| placement.glue_parent_attachment),
            );
            self.glue_attachment_ids.sort_unstable();
            self.glue_attachment_ids.dedup();
            update_model_distance_sort_flags(
                &self.placements,
                &self.sources,
                &mut self.model_distance_sort,
            );
            self.placement_topology_dirty = false;
        }
        self.rider_transforms.clear();
        self.rider_transforms.reserve(
            self.mounted_guids
                .len()
                .saturating_sub(self.rider_transforms.capacity()),
        );
        self.item_transforms.clear();
        self.item_transforms.reserve(
            self.requested_items
                .len()
                .saturating_sub(self.item_transforms.capacity()),
        );
        self.visual_transforms.clear();
        self.visual_transforms.reserve(
            self.requested_visuals
                .len()
                .saturating_sub(self.visual_transforms.capacity()),
        );
        self.glue_attachment_transforms.clear();
        self.glue_attachment_transforms.reserve(
            self.glue_attachment_ids
                .len()
                .saturating_sub(self.glue_attachment_transforms.capacity()),
        );
        self.character_animation_sync.clear();
        self.shoulder_animation_sync.clear();
        // Stock installs attachment six before attachment five, so the former
        // observes the latter's retained identity from the preceding frame.
        for placement in &self.placements {
            let M2GpuPlacementOwner::PlayerItem { guid, point } = placement.owner else {
                continue;
            };
            if point != CharacterAttachmentPoint::ShoulderLeft {
                continue;
            }
            let Some(source) = self
                .sources
                .get(placement.source_index)
                .and_then(Option::as_ref)
            else {
                continue;
            };
            let Some(playback) = placement.playback.as_ref() else {
                continue;
            };
            self.shoulder_animation_sync
                .push((guid, playback.synchronization(&source.model)));
        }
        for (placement_index, placement) in self.placements.iter_mut().enumerate() {
            if let Some(attachment_id) = placement.glue_parent_attachment {
                let parent = self
                    .glue_attachment_transforms
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
                && self.mounted_guids.contains(&guid)
            {
                let transform = self
                    .rider_transforms
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
                if self
                    .rider_transforms
                    .iter()
                    .any(|(owner_guid, transform)| *owner_guid == guid && transform.is_none())
                {
                    continue;
                }
                let transform = self
                    .item_transforms
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
                placement.transform = transform * placement.orientation.local_transform();
            }
            if let M2GpuPlacementOwner::PlayerItemVisual {
                guid,
                item_point,
                effect_point,
            } = placement.owner
            {
                if self
                    .rider_transforms
                    .iter()
                    .any(|(owner_guid, transform)| *owner_guid == guid && transform.is_none())
                {
                    continue;
                }
                let transform = self
                    .visual_transforms
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
            let owner = placement.owner;
            // Ordinary ADT/WMO placements have no animated parent transform.
            // Cull them before advancing playback or recomposing bones, just
            // as the stock/SolCL world renderer first builds a visible
            // instance list. Large tiles commonly retain thousands of static
            // placements while only tens intersect the camera frustum.
            let static_visibility_resolved = matches!(owner, M2GpuPlacementOwner::Static(_));
            if static_visibility_resolved {
                let (center, radius) =
                    placement_bounding_sphere(&source.model, placement.transform);
                if !frustum.contains_sphere(center, radius)? {
                    continue;
                }
            }
            let animation_binding = placement.animation_binding;
            let character_source = || {
                let guid = placement_owner_guid(owner)?;
                self.character_animation_sync
                    .iter()
                    .find_map(|(owner, state)| (*owner == guid).then_some(*state))
            };
            let shoulder_source = || {
                let guid = placement_owner_guid(owner)?;
                self.shoulder_animation_sync
                    .iter()
                    .find_map(|(owner, state)| (*owner == guid).then_some(*state))
            };
            let synchronization = match animation_binding {
                M2AnimationBinding::Independent => None,
                M2AnimationBinding::Character => character_source(),
                M2AnimationBinding::OppositeShoulder => shoulder_source(),
                M2AnimationBinding::OppositeShoulderOrCharacter => {
                    shoulder_source().or_else(character_source)
                }
            };
            let Some(playback) = placement.playback.as_mut() else {
                continue;
            };
            if let Some(synchronization) = synchronization {
                playback.synchronize_from(&source.model, synchronization)?;
            }
            let advance = if matches!(owner, M2GpuPlacementOwner::GlueModel { .. }) {
                self.pending_glue_playback_advance.take().map_or_else(
                    || playback.clock(&source.model, animation_time_ms, global_time_ms, random),
                    Ok,
                )?
            } else {
                playback.clock(&source.model, animation_time_ms, global_time_ms, random)?
            };
            if let M2GpuPlacementOwner::PlayerBody { guid }
            | M2GpuPlacementOwner::RemotePlayerBody { guid } = owner
            {
                self.character_animation_sync
                    .push((guid, playback.synchronization(&source.model)));
            }
            for expired in advance.expired_variations {
                self.bone_pose_scratch
                    .recompose_with_model_view_and_orientation_mask(
                        source.model.animations(),
                        expired.clock,
                        camera.view() * placement.transform,
                        &source.model_oriented_billboard_bones,
                    )?;
                append_triggered_events(
                    &mut self.triggered_events,
                    &source.model,
                    placement.owner,
                    placement.transform,
                    &self.bone_pose_scratch,
                    expired.event_window,
                )?;
            }
            let clock = advance.clock;
            let finger_pose_hands = held_item_finger_pose(&self.requested_items, owner);
            let finger_pose = finger_pose_hands.and_then(|hands| {
                source
                    .model
                    .animations()
                    .sequence_for_variation(15, 0)
                    .map(|sequence| (M2AnimationClock::new(sequence, 0.0, global_time_ms), hands))
            });
            let event_window = playback.event_window(animation_time_ms, global_time_ms);
            let model_view = camera.view() * placement.transform;
            let instance_identity = std::ptr::from_ref(&*placement).addr();
            let instance_distance = m2_model_distance_key(model_view);
            self.bone_pose_scratch
                .recompose_with_model_view_orientation_and_finger_pose(
                    source.model.animations(),
                    clock,
                    model_view,
                    &source.model_oriented_billboard_bones,
                    finger_pose,
                )?;
            let bone_pose = &self.bone_pose_scratch;
            append_triggered_events(
                &mut self.triggered_events,
                &source.model,
                placement.owner,
                placement.transform,
                bone_pose,
                event_window,
            )?;
            if matches!(placement.owner, M2GpuPlacementOwner::GlueModel { .. }) {
                sample_m2_lights_into(
                    source.model.animations(),
                    bone_pose,
                    clock,
                    placement.transform,
                    &mut self.glue_directional_lights,
                    &mut self.glue_point_lights,
                )?;
                for attachment_id in &self.glue_attachment_ids {
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
                    self.glue_attachment_transforms
                        .push((*attachment_id, transform));
                }
            }
            if let M2GpuPlacementOwner::PlayerMount { guid }
            | M2GpuPlacementOwner::RemotePlayerMount { guid } = placement.owner
            {
                if matches!(placement.owner, M2GpuPlacementOwner::PlayerMount { .. }) {
                    self.mount_camera_sample = Some(sample_mount_camera(
                        &source.model,
                        placement.transform,
                        bone_pose,
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
                self.rider_transforms.push((guid, transform));
            }
            if let M2GpuPlacementOwner::PlayerBody { guid }
            | M2GpuPlacementOwner::RemotePlayerBody { guid } = placement.owner
            {
                for (_owner_guid, point) in self
                    .requested_items
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
                    self.item_transforms.push((guid, *point, transform));
                }
            }
            if let M2GpuPlacementOwner::PlayerItem { guid, point } = placement.owner {
                for (_owner_guid, _owner_item_point, effect_point) in self
                    .requested_visuals
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
                    self.visual_transforms
                        .push((guid, point, *effect_point, transform));
                }
            }
            if !static_visibility_resolved {
                let (center, radius) =
                    placement_bounding_sphere(&source.model, placement.transform);
                if !frustum.contains_sphere(center, radius)? {
                    continue;
                }
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
            for (particle_index, ((emitter, placement_particle), resources)) in source
                .model
                .animations()
                .particles()
                .iter()
                .zip(&mut placement.particles)
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
                let pose = M2ParticlePose::sample(source.model.animations(), emitter, clock)?;
                let emitter_transform =
                    bone_pose.particle_emitter_transform(emitter, placement.transform)?;
                let model_lod_position = particle_lod_origin(placement.transform);
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
                    )?,
                    2 => simulation.advance_sphere_bounded(
                        emitter,
                        pose,
                        effect_delta_seconds,
                        emitter_transform,
                        particle_density,
                    )?,
                    emitter_type => {
                        return Err(RuntimeTerrainFrameError::M2ParticleEmitterType {
                            model: source.model.path().clone(),
                            particle_index,
                            emitter_type,
                        });
                    }
                };
                let (emitter_vertex_capacity, emitter_index_capacity) =
                    M2ParticleMeshPlan::buffer_capacity(emitter, simulation.capacity())?;
                particle_vertex_capacity = particle_vertex_capacity
                    .checked_add(emitter_vertex_capacity)
                    .ok_or(M2ParticleMeshPlanError::VertexCount)?;
                particle_index_capacity = particle_index_capacity
                    .checked_add(emitter_index_capacity)
                    .ok_or(M2ParticleMeshPlanError::IndexCount)?;
                self.particle_vertices
                    .reserve(particle_vertex_capacity.saturating_sub(self.particle_vertices.len()));
                self.particle_indices
                    .reserve(particle_index_capacity.saturating_sub(self.particle_indices.len()));
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
                let first_vertex =
                    u32::try_from(self.particle_vertices.len()).map_err(|_source| {
                        solarity_rendering::VulkanError::M2ParticleDrawVertexRange
                    })?;
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
                        placement_color(placement.color).w * placement.opacity,
                        &self.particle_twinkle,
                        placement.particle_colors.as_ref(),
                        &mut self.particle_sort_indices,
                        &mut self.particle_vertices,
                        &mut self.particle_indices,
                    )?;
                let effect_order = u32::try_from(self.transparent_elements.len())
                    .map_err(|_source| solarity_rendering::VulkanError::M2ParticleDrawIndexRange)?;
                let prepared = renderer
                    .prepare_m2_particle_draw_range(
                        resources.pipeline,
                        resources.texture_set,
                        emitter.blending_type(),
                        emitter.flags(),
                        M2EffectOrder::new(emitter.priority_plane(), effect_order),
                        first_vertex,
                        first_index,
                        vertex_count,
                        index_count,
                    )?
                    .with_light_bank(light_bank);
                let prepared_index = self.particle_draws.len();
                self.particle_draws.push(prepared);
                self.transparent_elements.push(M2TransparentElement {
                    pass: M2TransparentPass::for_particle_flags(emitter.flags()),
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
                tracing::trace!(
                    model = %source.model.path(),
                    particle_index,
                    live_count = simulation.particles().len(),
                    vertex_count,
                    index_count,
                    "placement-local particles entered unified world frame"
                );
            }
            advance_ribbons(
                &source.model,
                placement,
                bone_pose,
                clock,
                effect_delta_seconds,
                effect_scale,
            )?;
            self.bone_transforms
                .extend_from_slice(bone_pose.transforms());
            let mut instance_color = placement_color(placement.color);
            instance_color.w *= placement.opacity;
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
                    let alpha_state = M2ElementAlphaState::classify(element_alpha);
                    if alpha_state == M2ElementAlphaState::Hidden {
                        continue;
                    }
                    let runtime_alpha_fade = alpha_state == M2ElementAlphaState::Translucent
                        && !material_state.blend_enabled();
                    let material = M2MaterialUniform::new(
                        placement.transform,
                        pose.texture_transforms(),
                        model_view,
                        pose.mesh_color() * instance_color,
                        fog_color.extend(1.0),
                        glam::Vec4::new(
                            material_state.alpha_reference(instance_color.w),
                            material_state.fog_mode().shader_code(),
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
                    if draw.transparent_sort_unit()
                        || alpha_state == M2ElementAlphaState::Translucent
                    {
                        let section_distance = section_distance_key(draw, bone_pose, model_view)?;
                        let primary_distance = if self.model_distance_sort[placement_index] {
                            m2_model_distance_key(model_view)
                        } else {
                            section_distance
                        };
                        let producer_order = u32::try_from(self.transparent_elements.len())
                            .map_err(|_source| solarity_rendering::VulkanError::M2DrawIndexRange)?;
                        let prepared_index = self.visible_draws.len();
                        self.visible_draws.push(prepared);
                        self.transparent_elements.push(M2TransparentElement {
                            pass: M2TransparentPass::One,
                            key: M2TransparentSortKey::new(
                                primary_distance,
                                false,
                                i16::from(draw.batch().priority_plane),
                                section_distance,
                                instance_identity,
                                draw.batch().material_layer,
                            )
                            .with_scene_element(0, producer_order),
                            draw: M2TransparentDrawIndex::Mesh(prepared_index),
                        });
                    } else {
                        let scene_order = u32::try_from(self.visible_draws.len())
                            .map_err(|_source| solarity_rendering::VulkanError::M2DrawIndexRange)?;
                        self.visible_draws
                            .push(prepared.with_scene_order(scene_order));
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
                if trail.sections().len() < 2 {
                    continue;
                }
                let first_vertex = u32::try_from(self.ribbon_vertices.len())
                    .map_err(|_source| solarity_rendering::VulkanError::M2RibbonDrawVertexRange)?;
                let vertex_count =
                    M2RibbonMeshPlan::append(emitter, trail, &mut self.ribbon_vertices)?;
                for pass in passes {
                    let effect_order =
                        u32::try_from(self.transparent_elements.len()).map_err(|_source| {
                            solarity_rendering::VulkanError::M2RibbonDrawVertexRange
                        })?;
                    let prepared = renderer
                        .prepare_m2_ribbon_draw_range(
                            pass.pipeline,
                            pass.texture_set,
                            pass.material,
                            M2EffectOrder::new(emitter.priority_plane(), effect_order),
                            first_vertex,
                            vertex_count,
                        )?
                        .with_light_bank(light_bank);
                    let prepared_index = self.ribbon_draws.len();
                    self.ribbon_draws.push(prepared);
                    self.transparent_elements.push(M2TransparentElement {
                        pass: M2TransparentPass::One,
                        key: M2TransparentSortKey::new(
                            instance_distance,
                            false,
                            emitter.priority_plane(),
                            instance_distance,
                            instance_identity,
                            0,
                        )
                        .with_scene_element(3, effect_order),
                        draw: M2TransparentDrawIndex::Ribbon(prepared_index),
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
        }
        self.transparent_elements.sort_unstable_by(|left, right| {
            left.pass
                .cmp(&right.pass)
                .then_with(|| compare_m2_transparent(&left.key, &right.key))
        });
        let first_transparent_order = self.visible_draws.len();
        for (index, element) in self.transparent_elements.iter().enumerate() {
            let scene_order = first_transparent_order
                .checked_add(index)
                .and_then(|index| u32::try_from(index).ok())
                .ok_or(solarity_rendering::VulkanError::M2DrawIndexRange)?;
            match element.draw {
                M2TransparentDrawIndex::Mesh(draw_index) => {
                    let draw = self
                        .visible_draws
                        .get_mut(draw_index)
                        .ok_or(solarity_rendering::VulkanError::M2DrawIndexRange)?;
                    *draw = draw.with_scene_order(scene_order);
                }
                M2TransparentDrawIndex::Particle(draw_index) => {
                    let draw = self
                        .particle_draws
                        .get_mut(draw_index)
                        .ok_or(solarity_rendering::VulkanError::M2ParticleDrawIndexRange)?;
                    *draw = draw.with_scene_order(scene_order);
                }
                M2TransparentDrawIndex::Ribbon(draw_index) => {
                    let draw = self
                        .ribbon_draws
                        .get_mut(draw_index)
                        .ok_or(solarity_rendering::VulkanError::M2RibbonDrawVertexRange)?;
                    *draw = draw.with_scene_order(scene_order);
                }
            }
        }
        self.visible_draws.sort_by_key(|draw| draw.scene_order());
        self.particle_draws.sort_by_key(|draw| draw.scene_order());
        self.ribbon_draws.sort_by_key(|draw| draw.scene_order());
        Ok(M2VisibleFrame {
            bone_transforms: &self.bone_transforms,
            draws: &self.visible_draws,
            particle_vertices: &self.particle_vertices,
            particle_indices: &self.particle_indices,
            particle_draws: &self.particle_draws,
            particle_vertex_capacity,
            particle_index_capacity,
            ribbon_vertices: &self.ribbon_vertices,
            ribbon_draws: &self.ribbon_draws,
            glue_directional_lights: &self.glue_directional_lights,
            glue_point_lights: &self.glue_point_lights,
        })
    }

    /// Transfers every event generated by the most recent presentation frame.
    pub(in crate::application) fn drain_triggered_events(&mut self) -> Vec<RuntimeM2Event> {
        std::mem::take(&mut self.triggered_events)
    }

    /// Transfers newly contained emitter diagnostics to the process console.
    pub(super) fn drain_recoverable_errors(&mut self) -> Vec<String> {
        std::mem::take(&mut self.recoverable_errors)
    }

    /// Takes the controlled mount's marker sample from the latest model pose.
    pub(super) fn take_mount_camera_sample(&mut self) -> Option<RuntimeMountCameraSample> {
        self.mount_camera_sample.take()
    }
}

fn placement_bounding_sphere(model: &DecodedM2Model, transform: Mat4) -> (glam::Vec3, f32) {
    let bounds = model.bounds();
    let center = transform.transform_point3((bounds.minimum() + bounds.maximum()) * 0.5);
    let maximum_scale = transform.x_axis.truncate().length().max(
        transform
            .y_axis
            .truncate()
            .length()
            .max(transform.z_axis.truncate().length()),
    );
    (center, bounds.sphere_radius() * maximum_scale)
}

/// Selects the held-item finger trees layered by animation 15 (`HandsClosed`).
fn held_item_finger_pose(
    requested_items: &[(u64, CharacterAttachmentPoint)],
    owner: M2GpuPlacementOwner,
) -> Option<M2FingerPoseHands> {
    let guid = match owner {
        M2GpuPlacementOwner::PlayerBody { guid }
        | M2GpuPlacementOwner::RemotePlayerBody { guid } => guid,
        _ => return None,
    };
    let mut right = false;
    let mut left = false;
    for (_guid, point) in requested_items
        .iter()
        .filter(|(item_guid, _point)| *item_guid == guid)
    {
        match point {
            CharacterAttachmentPoint::HandRight => right = true,
            CharacterAttachmentPoint::HandLeft | CharacterAttachmentPoint::Shield => left = true,
            CharacterAttachmentPoint::ShoulderLeft
            | CharacterAttachmentPoint::ShoulderRight
            | CharacterAttachmentPoint::Helmet
            | CharacterAttachmentPoint::SheathMainHand
            | CharacterAttachmentPoint::SheathOffHand
            | CharacterAttachmentPoint::SheathShield
            | CharacterAttachmentPoint::LargeWeaponLeft
            | CharacterAttachmentPoint::LargeWeaponRight
            | CharacterAttachmentPoint::HipWeaponLeft
            | CharacterAttachmentPoint::HipWeaponRight => {}
        }
    }
    match (right, left) {
        (true, true) => Some(M2FingerPoseHands::Both),
        (true, false) => Some(M2FingerPoseHands::Right),
        (false, true) => Some(M2FingerPoseHands::Left),
        (false, false) => None,
    }
}

/// Reproduces `CCharacterSelection::SetFacing` from build 12340.
///
/// Stock passes zero translation, the requested facing, and an explicit scale
/// of one into the model transform. Creature display/model scale belongs to
/// world creature placement and must not be multiplied into a Glue character.
fn stock_glue_character_local_transform(facing_radians: f32) -> Mat4 {
    Mat4::from_rotation_z(facing_radians)
}

/// Publishes one Glue character source from an already completed worker generation.
#[allow(clippy::too_many_arguments)]
fn prepare_glue_character_gpu_source(
    renderer: &mut VulkanRenderer,
    model: &Arc<DecodedM2Model>,
    textures: &[M2ResolvedTexture<'_>],
    geosets: Option<M2GeosetSelection<'_>>,
    local_light_count: M2LocalLightCount,
    cpu_sources: &HashMap<M2GlueCpuSourceKey, Arc<M2CpuSource>>,
    orientation: M2ModelOrientation,
) -> Result<M2GpuSource, RuntimeTerrainFrameError> {
    let key = M2GlueCpuSourceKey::new(model.path().clone(), local_light_count);
    let cpu_source =
        cpu_sources
            .get(&key)
            .ok_or_else(|| RuntimeTerrainFrameError::MissingGlueCpuSource {
                model: model.path().clone(),
                local_light_count,
            })?;
    prepare_gpu_source_from_cpu(
        renderer,
        model,
        textures,
        geosets,
        local_light_count,
        cpu_source,
        orientation,
    )
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
                ResidentCreatureTexture::StockWhite => M2ResolvedTexture::StockWhite,
                ResidentCreatureTexture::StockFailure => M2ResolvedTexture::StockFailure,
                ResidentCreatureTexture::Unresolved(kind) => M2ResolvedTexture::Unresolved(*kind),
            })
            .collect::<Vec<_>>();
        let source = prepare_gpu_source(
            renderer,
            mount.model(),
            &resolved,
            None,
            M2LocalLightCount::Zero,
            M2ModelOrientation::Authored,
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
            ResidentPlayerTexture::StockWhite => M2ResolvedTexture::StockWhite,
            ResidentPlayerTexture::StockFailure => M2ResolvedTexture::StockFailure,
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
        M2ModelOrientation::Authored,
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
                ResidentPlayerTexture::StockWhite => M2ResolvedTexture::StockWhite,
                ResidentPlayerTexture::StockFailure => M2ResolvedTexture::StockFailure,
                ResidentPlayerTexture::BodyAtlas => {
                    M2ResolvedTexture::CharacterAtlas(input.atlas())
                }
                ResidentPlayerTexture::Unresolved(kind) => M2ResolvedTexture::Unresolved(*kind),
            })
            .collect::<Vec<_>>();
        let orientation = if attachment.is_model_mirrored() {
            M2ModelOrientation::Mirrored
        } else {
            M2ModelOrientation::Authored
        };
        let source = prepare_gpu_source(
            renderer,
            attachment.model(),
            &resolved,
            None,
            M2LocalLightCount::Zero,
            orientation,
        )?;
        let mut placement = unit_gpu_placement(
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
        placement.orientation = orientation;
        placement.animation_binding = equipment_animation_binding(attachment);
        prepared.push((source, placement));
        for effect in attachment.visual_effects() {
            let resolved = effect
                .textures()
                .iter()
                .map(|texture| match texture {
                    ResidentPlayerTexture::Authored(source) => {
                        M2ResolvedTexture::Authored(source.as_ref())
                    }
                    ResidentPlayerTexture::StockWhite => M2ResolvedTexture::StockWhite,
                    ResidentPlayerTexture::StockFailure => M2ResolvedTexture::StockFailure,
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
                orientation,
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
    let particles = stock_particle_simulations(model);
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
        orientation: M2ModelOrientation::Authored,
        animation_binding: M2AnimationBinding::Independent,
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

/// Resolves stock's ordered ItemDisplayInfo animation-flag overrides.
fn equipment_animation_binding(attachment: &ResidentPlayerAttachment) -> M2AnimationBinding {
    if attachment.mirrors_opposite_shoulder_animation() {
        if attachment.inherits_character_animation() {
            M2AnimationBinding::OppositeShoulderOrCharacter
        } else {
            M2AnimationBinding::OppositeShoulder
        }
    } else if attachment.inherits_character_animation() {
        M2AnimationBinding::Character
    } else {
        M2AnimationBinding::Independent
    }
}

/// Constructs every placement-local emitter with stock's fixed PRNG seed.
///
/// `CM2ParticleEmitter` initializes its embedded Blizzard generator to zero;
/// it does not consume the process CRT stream used by animation selection.
fn stock_particle_simulations(model: &DecodedM2Model) -> Vec<M2ParticlePlacement> {
    model
        .animations()
        .particles()
        .iter()
        .map(|emitter| M2ParticlePlacement::new(emitter, M2ParticleSimulation::new(0)))
        .collect()
}

/// Reserves each finite Glue emitter's authored maximum before presentation.
fn glue_particle_simulations(
    model: &DecodedM2Model,
) -> Result<Vec<M2ParticlePlacement>, RuntimeTerrainFrameError> {
    model
        .animations()
        .particles()
        .iter()
        .map(|emitter| {
            let mut simulation = M2ParticleSimulation::new(0);
            simulation.reserve_authored_capacity(emitter)?;
            Ok(M2ParticlePlacement::new(emitter, simulation))
        })
        .collect()
}

/// Build 12340 `0x0097EB10` measures the shared model origin for particle LOD;
/// authored emitter offsets and animated bones do not move that origin.
fn particle_lod_origin(placement_transform: Mat4) -> glam::Vec3 {
    placement_transform.transform_point3(glam::Vec3::ZERO)
}

/// Applies stock's camera-distance multiplier to the global particle density.
fn particle_emission_density(flags: u32, emitter_origin: glam::Vec3, camera: glam::Vec3) -> f32 {
    if flags & PARTICLE_IGNORE_DISTANCE_LOD != 0 {
        return STOCK_DEFAULT_PARTICLE_DENSITY;
    }
    let distance = emitter_origin.distance(camera);
    if !distance.is_finite() {
        return STOCK_DEFAULT_PARTICLE_DENSITY;
    }
    STOCK_DEFAULT_PARTICLE_DENSITY * (1.0 - (distance - 50.0) * 0.02).clamp(0.25, 1.0)
}

/// Advances every shared declaration through its placement-owned edge history.
fn advance_ribbons(
    model: &DecodedM2Model,
    placement: &mut M2GpuPlacement,
    bone_pose: &M2BonePose,
    clock: M2AnimationClock,
    delta_seconds: f32,
    effect_scale: M2CameraEffectScale,
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
            transform.y_axis.truncate() * effect_scale.factor(),
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
            ResidentM2Texture::StockWhite => M2ResolvedTexture::StockWhite,
            ResidentM2Texture::StockFailure => M2ResolvedTexture::StockFailure,
            ResidentM2Texture::Replaceable(kind) => M2ResolvedTexture::Unresolved(*kind),
        })
        .collect::<Vec<_>>();
    let plan = Arc::clone(&source.cpu_source().plan);
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
    Ok(Some(prepare_gpu_source_from_cpu(
        renderer,
        model,
        &resolved,
        None,
        M2LocalLightCount::Zero,
        source.cpu_source(),
        M2ModelOrientation::Authored,
    )?))
}

/// Publishes immutable model resources using one owner's resolved texture table.
fn prepare_gpu_source(
    renderer: &mut VulkanRenderer,
    model: &Arc<DecodedM2Model>,
    textures: &[M2ResolvedTexture<'_>],
    geosets: Option<M2GeosetSelection<'_>>,
    local_light_count: M2LocalLightCount,
    orientation: M2ModelOrientation,
) -> Result<M2GpuSource, RuntimeTerrainFrameError> {
    let plan = Arc::new(M2MeshPlan::prepare(model, STOCK_HIGH_CAPABILITY_PROFILE)?);
    prepare_gpu_source_with_plan(
        renderer,
        model,
        textures,
        geosets,
        local_light_count,
        plan,
        None,
        orientation,
    )
}

/// Publishes a Glue source from worker-prepared mesh and shader state.
#[allow(clippy::too_many_arguments)]
fn prepare_gpu_source_from_cpu(
    renderer: &mut VulkanRenderer,
    model: &Arc<DecodedM2Model>,
    textures: &[M2ResolvedTexture<'_>],
    geosets: Option<M2GeosetSelection<'_>>,
    local_light_count: M2LocalLightCount,
    cpu_source: &M2CpuSource,
    orientation: M2ModelOrientation,
) -> Result<M2GpuSource, RuntimeTerrainFrameError> {
    prepare_gpu_source_with_plan(
        renderer,
        model,
        textures,
        geosets,
        local_light_count,
        Arc::clone(&cpu_source.plan),
        Some(cpu_source),
        orientation,
    )
}

/// Joins one CPU plan to texture uploads and render-owner Vulkan resources.
#[allow(clippy::too_many_arguments)]
fn prepare_gpu_source_with_plan(
    renderer: &mut VulkanRenderer,
    model: &Arc<DecodedM2Model>,
    textures: &[M2ResolvedTexture<'_>],
    geosets: Option<M2GeosetSelection<'_>>,
    local_light_count: M2LocalLightCount,
    plan: Arc<M2MeshPlan>,
    cpu_source: Option<&M2CpuSource>,
    orientation: M2ModelOrientation,
) -> Result<M2GpuSource, RuntimeTerrainFrameError> {
    if textures.len() != model.textures().len() {
        return Err(RuntimeTerrainFrameError::M2TextureTableCount {
            model: model.path().clone(),
            source_count: textures.len(),
            model_count: model.textures().len(),
        });
    }
    validate_gpu_texture_coverage(model, &plan, textures, geosets)?;
    let model_oriented_billboard_bones = match geosets {
        Some(M2GeosetSelection::Character(geosets)) => {
            plan.character_eye_orientation_mask(geosets, model.animations().bones().len())
        }
        Some(M2GeosetSelection::Creature(_)) | None => Vec::new(),
    };
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
    let stock_white = textures
        .iter()
        .any(|texture| matches!(texture, M2ResolvedTexture::StockWhite))
        .then(|| renderer.upload_stock_m2_white())
        .transpose()?;
    let stock_failure = textures
        .iter()
        .any(|texture| matches!(texture, M2ResolvedTexture::StockFailure))
        .then(|| renderer.upload_stock_m2_failure())
        .transpose()?;
    for (texture_index, texture) in textures.iter().enumerate() {
        let handle = match texture {
            M2ResolvedTexture::StockWhite => stock_white,
            M2ResolvedTexture::StockFailure => stock_failure,
            M2ResolvedTexture::Authored(_)
            | M2ResolvedTexture::CharacterAtlas(_)
            | M2ResolvedTexture::Unresolved(_) => None,
        };
        if let Some(handle) = handle {
            texture_handles[texture_index] = Some(M2TextureImageHandle::Blp(handle));
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
        let pipeline = match cpu_source {
            Some(cpu_source) => {
                let program = cpu_source
                    .mesh_programs
                    .get(&M2SpirvKey::new(shader, permutation))
                    .ok_or_else(|| RuntimeTerrainFrameError::M2CpuProgram {
                        model: model.path().clone(),
                        domain: "mesh",
                    })?;
                renderer.prepare_precompiled_oriented_m2_pipeline(program, orientation)?
            }
            None => renderer.prepare_oriented_m2_pipeline(shader, permutation, orientation)?,
        };
        let material = M2MaterialState::from_material(draw.material());
        let runtime_fade_pipeline = if material.blend_enabled() {
            None
        } else {
            let fade = shader.with_runtime_alpha_fade();
            Some(match cpu_source {
                Some(cpu_source) => {
                    let program = cpu_source
                        .mesh_programs
                        .get(&M2SpirvKey::new(fade, permutation))
                        .ok_or_else(|| RuntimeTerrainFrameError::M2CpuProgram {
                            model: model.path().clone(),
                            domain: "runtime-fade mesh",
                        })?;
                    renderer.prepare_precompiled_oriented_m2_pipeline(program, orientation)?
                }
                None => renderer.prepare_oriented_m2_pipeline(fade, permutation, orientation)?,
            })
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
        let material = M2MaterialState::from_particle(emitter.blending_type(), emitter.flags());
        particle_pipelines.push(match cpu_source {
            Some(cpu_source) => {
                let program = cpu_source.particle_programs.get(&material).ok_or_else(|| {
                    RuntimeTerrainFrameError::M2CpuProgram {
                        model: model.path().clone(),
                        domain: "particle",
                    }
                })?;
                renderer.prepare_precompiled_m2_particle_pipeline(program)?
            }
            None => {
                renderer.prepare_m2_particle_pipeline(emitter.blending_type(), emitter.flags())?
            }
        });
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
            let pipeline = match cpu_source {
                Some(cpu_source) => {
                    let state = M2MaterialState::from_material(material);
                    let program = cpu_source.ribbon_programs.get(&state).ok_or_else(|| {
                        RuntimeTerrainFrameError::M2CpuProgram {
                            model: model.path().clone(),
                            domain: "ribbon",
                        }
                    })?;
                    renderer.prepare_precompiled_m2_ribbon_pipeline(program)?
                }
                None => renderer.prepare_m2_ribbon_pipeline(material)?,
            };
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
        model_oriented_billboard_bones,
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
        Some(
            M2ResolvedTexture::Authored(_)
            | M2ResolvedTexture::CharacterAtlas(_)
            | M2ResolvedTexture::StockWhite
            | M2ResolvedTexture::StockFailure,
        ) => Ok(()),
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
        M2ResolvedTexture::Authored(_)
        | M2ResolvedTexture::CharacterAtlas(_)
        | M2ResolvedTexture::StockWhite
        | M2ResolvedTexture::StockFailure => handles
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

#[cfg(test)]
mod tests {
    use glam::{Mat4, Vec3};

    use super::{
        M2UnsupportedParticle, PARTICLE_IGNORE_DISTANCE_LOD, classify_particle_support,
        particle_emission_density, particle_lod_origin, stock_glue_character_local_transform,
    };

    #[test]
    fn unsupported_particle_paths_are_isolated_before_frame_advance() {
        assert_eq!(classify_particle_support(1, 0), None);
        assert_eq!(classify_particle_support(2, 0), None);
        assert_eq!(
            classify_particle_support(1, 0x0000_0800),
            Some(M2UnsupportedParticle::BehaviorFlags(0x0000_0800))
        );
        assert_eq!(
            classify_particle_support(3, 0),
            Some(M2UnsupportedParticle::EmitterType(3))
        );
    }

    #[test]
    fn glue_character_local_transform_keeps_stock_unit_scale() {
        let transform = stock_glue_character_local_transform(0.75);
        assert_eq!(transform.transform_vector3(Vec3::X).length(), 1.0);
        assert_eq!(transform.transform_point3(Vec3::ZERO), Vec3::ZERO);
        assert_ne!(transform, Mat4::IDENTITY);
    }

    #[test]
    fn particle_distance_lod_matches_stock_threshold_and_floor() {
        let camera = Vec3::ZERO;
        assert_eq!(particle_emission_density(0, Vec3::X * 50.0, camera), 1.0);
        assert_eq!(particle_emission_density(0, Vec3::X * 75.0, camera), 0.5);
        assert_eq!(particle_emission_density(0, Vec3::X * 100.0, camera), 0.25);
        assert_eq!(
            particle_emission_density(PARTICLE_IGNORE_DISTANCE_LOD, Vec3::X * 100.0, camera),
            1.0
        );
    }

    #[test]
    fn particle_lod_uses_model_origin_before_emitter_offsets() {
        let model = Mat4::from_translation(Vec3::new(12.0, 34.0, 56.0));
        let emitter = model * Mat4::from_translation(Vec3::new(1_000.0, 2_000.0, 3_000.0));

        assert_eq!(particle_lod_origin(model), Vec3::new(12.0, 34.0, 56.0));
        assert_ne!(
            particle_lod_origin(model),
            emitter.transform_point3(Vec3::ZERO)
        );
    }
}
