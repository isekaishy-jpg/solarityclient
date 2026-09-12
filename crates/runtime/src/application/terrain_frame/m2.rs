//! Renderer-local resources for the shared resident placed-M2 scene.

#[cfg(test)]
#[path = "../../../tests/application/game_object_scene.rs"]
mod game_object_scene_tests;

#[cfg(test)]
#[path = "../../../tests/application/unit_effect_models.rs"]
mod unit_effect_model_tests;

mod character_residency;
mod distance;
mod doodad_scene;
mod entity_lighting;
mod game_objects;
mod playback;
mod portrait;
mod retirement;
#[cfg(test)]
#[path = "../../../tests/application/scenery_distance.rs"]
mod scenery_distance_tests;
mod shadow;
pub(in crate::application) mod sky;
pub(in crate::application) mod sound;
#[cfg(test)]
#[path = "../../../tests/application/static_m2_streaming.rs"]
mod static_streaming_tests;
mod streaming;
pub(in crate::application) mod unit_effects;
mod unit_registration;
mod unit_scene;
#[cfg(test)]
#[path = "../../../tests/application/unit_shadow_scene.rs"]
mod unit_shadow_tests;
mod vehicle_passengers;
mod visibility;
use crate::application::entity_opacity::EntityOpacityOwner;
use crate::application::unit_animation::UnitAnimationBehavior;
use character_residency::{
    M2PreparedCharacter, M2UnitItemIdentity, UnitEquipmentGpuInput, UnitMountGpuInput,
    prepare_character_gpu, prepare_mount_gpu, prepare_unit_equipment_gpu,
};
use playback::M2PlaybackStorage;
use unit_registration::UnitSceneRegistration;

use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock};

use crate::application::frame_profile::RuntimeFrameProfile;
use crate::application::model_playback::{M2Playback, M2PlaybackAdvance};
use glam::Mat4;
use solarity_asset::{
    AnimationDataCatalog, AssetPath, BlpTextureSource, DecodedM2Model, M2ParticleEmitter,
};
use solarity_ecs::{WorldObjectIdentity, WorldTransform};
use solarity_rendering::{
    BlpColorSpace, BlpTextureUploadRequest, CharacterAtlasTexture, CharacterAttachmentPoint,
    CharacterGeosetPlan, CreatureGeosetPlan, M2AnimationClock, M2BonePose, M2BonePoseOverrides,
    M2CameraEffectScale, M2DrawCall, M2EffectOrder, M2ElementAlphaState, M2EventTimeWindow,
    M2FingerPoseHands, M2LocalLightCount, M2MaterialPose, M2MaterialState, M2MaterialUniform,
    M2MeshHandle, M2MeshPlan, M2ModelOrientation, M2ParticleColorReplacement, M2ParticleMeshPlan,
    M2ParticleMeshPlanError, M2ParticlePipelineHandle, M2ParticlePose, M2ParticlePreparedDraw,
    M2ParticleRenderVertex, M2ParticleSimulation, M2ParticleSpirvCompiler, M2ParticleSpirvProgram,
    M2ParticleTwinkleTable, M2PipelineHandle, M2PreparedDraw, M2RibbonControlPoint,
    M2RibbonMeshPlan, M2RibbonPipelineHandle, M2RibbonPose, M2RibbonPreparedDraw,
    M2RibbonRenderVertex, M2RibbonSpirvCompiler, M2RibbonSpirvProgram, M2RibbonTrail,
    M2SampledTexture, M2SceneLightBank, M2ShaderPermutation, M2ShaderPlan, M2ShadowFiltering,
    M2ShadowPermutation, M2SpirvCompiler, M2SpirvKey, M2SpirvProgram, M2TextureImageHandle,
    M2TextureSet, M2TextureSetHandle, M2TransparentPass, M2TransparentSortKey, VulkanRenderer,
    WorldCameraFrame, WorldFrustum, compare_m2_transparent, m2_model_distance_key,
    m2_section_distance_key, sample_m2_lights_into, triggered_m2_event_indices,
};

use crate::application::game_object_coordinator::{
    GameObjectFrameInput, GameObjectWorldModelState,
};
use crate::application::player_coordinator::{
    MountModelKey, ResidentCreatureFrameInput, ResidentCreatureGeosets, ResidentCreatureTexture,
    ResidentGlueCharacterFrameInput, ResidentPlayerFrameInput, ResidentPlayerTexture,
    UnitPresentationGeneration,
};
use crate::application::terrain_coordinator::m2_residency::{
    ResidentM2Owner, ResidentM2Scene, ResidentM2Source, ResidentM2Texture,
};
use crate::random::CrtRand;

use super::RuntimeTerrainFrameError;
mod scene_lighting;
#[cfg(test)]
#[path = "../../../tests/application/scene_lighting.rs"]
mod scene_lighting_tests;

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
    /// 83CC80 counts bones with flags 0x2F8 for the static/animated shadow bank.
    animated_shadow_caster: bool,
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
    runtime_fade_pipeline: M2ParticlePipelineHandle,
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
    Ribbon { first: usize, count: usize },
}

/// Authoritative unit inputs retained before terrain tilt changes the model basis.
#[derive(Clone)]
struct UnitGroundPlacement {
    position: glam::Vec3,
    scale: f32,
    /// The unit owns normals/yaw even when this placement is its mount.
    owner: Rc<UnitAnimationBehavior>,
}

/// Exact per-instance state required by later animation and material assembly.
struct M2GpuPlacement {
    sound_lifetime: std::cell::OnceCell<Rc<sound::M2SoundKind>>,
    light_lifetime: std::cell::OnceCell<Rc<()>>,
    placement_valid: bool,
    /// CMapObj +0x0c fog bit survives exterior submissions without a bank write.
    scene_indoor_fog: bool,
    world_model_state: Option<Rc<GameObjectWorldModelState>>,
    source_index: usize,
    /// Placement-local transform retained across animated parent resolution.
    local_transform: Mat4,
    /// Raw unit position/scale avoid recovering placement inputs from a matrix.
    ground_placement: Option<UnitGroundPlacement>,
    /// GetModel scene bounds remain independent of terrain and rider posing.
    scene_registration: Option<UnitSceneRegistration>,
    /// 73D5D0's inverse mount display scale, applied after the rider attachment.
    rider_scale: f32,
    transform: Mat4,
    /// Local reflection paired with the source's Vulkan front-face state.
    orientation: M2ModelOrientation,
    /// Stock parent attachment used by Glue character preview models.
    glue_parent_attachment: Option<u32>,
    owner: M2GpuPlacementOwner,
    flags: u16,
    color: [u8; 4],
    entity_lighting: entity_lighting::EntityLighting,
    opacity: f32,
    entity_opacity: Option<Rc<EntityOpacityOwner>>,
    retirement: Option<Box<retirement::RetiredM2Placement>>,
    particle_colors: Option<M2ParticleColorReplacement>,
    playback: Option<M2PlaybackStorage>,
    /// The scene callback pass can advance a mount before its draw visit.
    passenger_playback_advance: Option<M2PlaybackAdvance>,
    unit_animation: Option<Rc<UnitAnimationBehavior>>,
    unit_presentation: Option<UnitPresentationGeneration>,
    /// An unchanged mount survives character atlas and equipment rebuilds.
    mount_key: Option<MountModelKey>,
    item_identity: Option<M2UnitItemIdentity>,
    particles: Vec<M2ParticlePlacement>,
    ribbons: Vec<M2RibbonTrail>,
    /// `CM2Model +0x8c` belongs to this model lifetime, including unsampled intervals.
    last_effect_time_ms: u32,
    unit_effect: Option<unit_effects::UnitEffectPlacement>,
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
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum M2GpuPlacementOwner {
    /// Independent scene lifetime after the gameplay object leaves the world.
    Retired(retirement::RetiredModelKey),
    /// One CEffect instance; its unit and attachment lifetime are retained separately.
    UnitEffect { serial: u64 },
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
    /// Independent mount below a non-player unit rider.
    CreatureMount { guid: u64 },
    /// The controlled player's current movement-parent GameObject.
    GameObject {
        guid: u64,
        identity: WorldObjectIdentity,
        display_id: u32,
    },
    /// One default-set MODD attached to a replicated WMO lifetime.
    GameObjectWorldModelDoodad {
        identity: WorldObjectIdentity,
        display_id: u32,
        doodad_index: usize,
    },
    /// One equipment M2 driven by an animated player or NPC attachment point.
    UnitItem {
        guid: u64,
        point: CharacterAttachmentPoint,
    },
    /// One enchant/display effect driven by its equipped item M2.
    UnitItemVisual {
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
            for material in std::iter::once(material)
                .chain((!material.blend_enabled()).then(|| material.with_runtime_alpha_fade()))
            {
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
                            Ok(mut cache) => {
                                cache.particles.entry(material).or_insert(program).clone()
                            }
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

/// One generic authored M2 callback resolved into world space.
#[derive(Clone, Debug)]
pub(in crate::application) struct RuntimeM2Event {
    identifier: [u8; 4],
    data: u32,
    position: glam::Vec3,
    owner_guid: Option<u64>,
    sound_owner: Option<sound::M2SoundOwner>,
    sound_kind: Option<sound::M2SoundKind>,
    effect_kit_sound: bool,
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
    pub(in crate::application) const fn new(
        identifier: [u8; 4],
        data: u32,
        position: glam::Vec3,
        owner_guid: Option<u64>,
    ) -> Self {
        Self {
            identifier,
            data,
            position,
            owner_guid,
            sound_owner: None,
            sound_kind: None,
            effect_kit_sound: false,
        }
    }

    pub(in crate::application) const fn identifier(&self) -> [u8; 4] {
        self.identifier
    }

    pub(in crate::application) const fn data(&self) -> u32 {
        self.data
    }

    pub(in crate::application) const fn position(&self) -> glam::Vec3 {
        self.position
    }

    pub(in crate::application) const fn owner_guid(&self) -> Option<u64> {
        self.owner_guid
    }

    pub(in crate::application) fn sound_owner(&self) -> Option<&sound::M2SoundOwner> {
        self.sound_owner.as_ref()
    }

    pub(in crate::application) fn with_sound_owner(mut self, owner: sound::M2SoundOwner) -> Self {
        self.sound_kind = Some(owner.kind());
        self.sound_owner = Some(owner);
        self
    }

    pub(in crate::application) const fn sound_kind(&self) -> Option<sound::M2SoundKind> {
        self.sound_kind
    }

    pub(in crate::application) fn with_effect_kit_sound(mut self) -> Self {
        self.effect_kit_sound = true;
        self
    }
    pub(in crate::application) const fn is_effect_kit_sound(&self) -> bool {
        self.effect_kit_sound
    }
}

/// All resident M2 geometry and transforms owned by one terrain generation.
pub(in crate::application) struct M2Frame {
    animations: Arc<AnimationDataCatalog>,
    sources: Vec<Option<M2GpuSource>>,
    placements: Vec<M2GpuPlacement>,
    static_residency: streaming::StaticM2Residency,
    particle_twinkle: Arc<M2ParticleTwinkleTable>,
    animation_started_at: std::time::Instant,
    /// Previous scene pass, independent of unit residency and draw admission.
    unit_scene_time_ms: f32,
    retirement: retirement::M2RetirementScene,
    bone_pose_scratch: M2BonePose,
    bone_transforms: Vec<Mat4>,
    visible_draws: Vec<M2PreparedDraw>,
    shadow_draws: Vec<M2PreparedDraw>,
    shadow_admission: Vec<bool>,
    environment_shadow_draws: Vec<solarity_rendering::WorldEnvironmentM2Caster>,
    environment_shadow_admission: Vec<u8>,
    transparent_elements: Vec<M2TransparentElement>,
    placement_topology_dirty: bool,
    placement_visibility: visibility::M2PlacementVisibility,
    doodad_scene: doodad_scene::M2DoodadScene,
    /// Stock environmentDetail is clamped by its 78DC60 CVar callback.
    pub(super) environment_detail: f32,
    particle_vertices: Vec<M2ParticleRenderVertex>,
    particle_indices: Vec<u32>,
    particle_sort_indices: Vec<usize>,
    particle_draws: Vec<M2ParticlePreparedDraw>,
    ribbon_vertices: Vec<M2RibbonRenderVertex>,
    ribbon_draws: Vec<M2RibbonPreparedDraw>,
    triggered_events: Vec<RuntimeM2Event>,
    mount_camera_sample: Option<RuntimeMountCameraSample>,
    scene_lighting: scene_lighting::SceneLighting,
    glue_directional_lights: Vec<solarity_rendering::M2DirectionalLight>,
    glue_point_lights: Vec<solarity_rendering::M2PointLight>,
    requested_items: Vec<(u64, CharacterAttachmentPoint)>,
    requested_visuals: Vec<(u64, CharacterAttachmentPoint, u32)>,
    mounted_guids: Vec<u64>,
    rider_transforms: Vec<(u64, Option<Mat4>)>,
    vehicle_passengers: vehicle_passengers::M2VehiclePassengers,
    item_transforms: Vec<(u64, CharacterAttachmentPoint, Option<Mat4>)>,
    visual_transforms: Vec<(u64, CharacterAttachmentPoint, u32, Option<Mat4>)>,
    glue_attachment_ids: Vec<u32>,
    glue_attachment_transforms: Vec<(u32, Option<Mat4>)>,
    recoverable_errors: Vec<String>,
    pending_glue_playback_advance: Option<M2PlaybackAdvance>,
    unit_effects: unit_effects::M2UnitEffectScene,
}

/// Borrowed dynamic streams assembled for one unified world submission.
pub(in crate::application) struct M2VisibleFrame<'frame> {
    pub(in crate::application) instance_scenes: &'frame [solarity_rendering::M2SceneUniform],
    pub(in crate::application) scene_points: &'frame solarity_rendering::ScenePointLights,
    pub(in crate::application) scene_directionals:
        &'frame [solarity_rendering::M2DirectionalLight],
    /// Native liquid queue one belongs between the two transparent model passes.
    pub(in crate::application) water_scene_order: u32,
    pub(in crate::application) bone_transforms: &'frame [Mat4],
    pub(in crate::application) draws: &'frame [M2PreparedDraw],
    pub(in crate::application) shadow_draws: &'frame [M2PreparedDraw],
    pub(in crate::application) environment_shadow_draws:
        &'frame [solarity_rendering::WorldEnvironmentM2Caster],
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
        animations: Arc<AnimationDataCatalog>,
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
                &animations,
                0.0,
                random,
            )?);
        }
        Ok(Self {
            animations,
            sources,
            placements,
            static_residency: streaming::StaticM2Residency::new(scene),
            particle_twinkle,
            animation_started_at: std::time::Instant::now(),
            unit_scene_time_ms: 0.0,
            retirement: Default::default(),
            bone_pose_scratch: M2BonePose::default(),
            bone_transforms: Vec::new(),
            visible_draws: Vec::new(),
            shadow_draws: Vec::new(),
            shadow_admission: Vec::new(),
            environment_shadow_draws: Vec::new(),
            environment_shadow_admission: Vec::new(),
            transparent_elements: Vec::new(),
            placement_topology_dirty: true,
            placement_visibility: visibility::M2PlacementVisibility::default(),
            doodad_scene: doodad_scene::M2DoodadScene::default(),
            environment_detail: 1.0,
            particle_vertices: Vec::new(),
            particle_indices: Vec::new(),
            particle_sort_indices: Vec::new(),
            particle_draws: Vec::new(),
            ribbon_vertices: Vec::new(),
            ribbon_draws: Vec::new(),
            triggered_events: Vec::new(),
            mount_camera_sample: None,
            scene_lighting: scene_lighting::SceneLighting::default(),
            glue_directional_lights: Vec::new(),
            glue_point_lights: Vec::new(),
            requested_items: Vec::new(),
            requested_visuals: Vec::new(),
            mounted_guids: Vec::new(),
            rider_transforms: Vec::new(),
            vehicle_passengers: vehicle_passengers::M2VehiclePassengers::default(),
            item_transforms: Vec::new(),
            visual_transforms: Vec::new(),
            glue_attachment_ids: Vec::new(),
            glue_attachment_transforms: Vec::new(),
            recoverable_errors: Vec::new(),
            pending_glue_playback_advance: None,
            unit_effects: unit_effects::M2UnitEffectScene::default(),
        })
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
        animations: Arc<AnimationDataCatalog>,
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
            animations,
            sources: vec![Some(source)],
            static_residency: streaming::StaticM2Residency::default(),
            placements: vec![M2GpuPlacement {
                sound_lifetime: Default::default(),
                light_lifetime: Default::default(),
                entity_lighting: Default::default(),
                placement_valid: true,
                scene_indoor_fog: false,
                world_model_state: None,
                source_index: 0,
                local_transform: transform,
                ground_placement: None,
                scene_registration: None,
                rider_scale: 1.0,
                transform,
                orientation: M2ModelOrientation::Authored,
                glue_parent_attachment: None,
                owner: M2GpuPlacementOwner::GlueModel { object_index },
                flags: 0,
                color: [u8::MAX; 4],
                opacity: 1.0,
                entity_opacity: None,
                retirement: None,
                particle_colors: None,
                playback: Some(M2PlaybackStorage::Local(playback)),
                passenger_playback_advance: None,
                unit_animation: None,
                unit_presentation: None,
                mount_key: None,
                item_identity: None,
                particles,
                ribbons,
                // This widget model was created at its local clock origin.
                last_effect_time_ms: 0,
                unit_effect: None,
            }],
            particle_twinkle,
            animation_started_at,
            unit_scene_time_ms: 0.0,
            retirement: Default::default(),
            bone_pose_scratch: M2BonePose::default(),
            bone_transforms: Vec::new(),
            visible_draws: Vec::new(),
            shadow_draws: Vec::new(),
            shadow_admission: Vec::new(),
            environment_shadow_draws: Vec::new(),
            environment_shadow_admission: Vec::new(),
            transparent_elements: Vec::new(),
            placement_topology_dirty: true,
            placement_visibility: visibility::M2PlacementVisibility::default(),
            doodad_scene: doodad_scene::M2DoodadScene::default(),
            environment_detail: 1.0,
            particle_vertices: Vec::new(),
            particle_indices: Vec::new(),
            particle_sort_indices: Vec::new(),
            particle_draws: Vec::new(),
            ribbon_vertices: Vec::new(),
            ribbon_draws: Vec::new(),
            triggered_events: Vec::new(),
            mount_camera_sample: None,
            scene_lighting: scene_lighting::SceneLighting::default(),
            glue_directional_lights: Vec::new(),
            glue_point_lights: Vec::new(),
            requested_items: Vec::new(),
            requested_visuals: Vec::new(),
            mounted_guids: Vec::new(),
            rider_transforms: Vec::new(),
            vehicle_passengers: vehicle_passengers::M2VehiclePassengers::default(),
            item_transforms: Vec::new(),
            visual_transforms: Vec::new(),
            glue_attachment_ids: Vec::new(),
            glue_attachment_transforms: Vec::new(),
            recoverable_errors: Vec::new(),
            pending_glue_playback_advance: None,
            unit_effects: unit_effects::M2UnitEffectScene::default(),
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
            self.animation_time_ms(),
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
            let orientation = M2ModelOrientation::Authored;
            let source = prepare_glue_character_gpu_source(
                renderer,
                attachment.model(),
                &resolved,
                None,
                character_light_count,
                cpu_sources,
                orientation,
            )?;
            let mut placement = default_gpu_placement(
                self.animation_time_ms(),
                transform,
                M2GpuPlacementOwner::UnitItem {
                    guid: 0,
                    point: attachment.point(),
                },
                attachment.model(),
                &self.animations,
                attachment.particle_colors().cloned(),
                random,
            )?;
            placement.orientation = orientation;
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
                let placement = default_gpu_placement(
                    self.animation_time_ms(),
                    transform,
                    M2GpuPlacementOwner::UnitItemVisual {
                        guid: 0,
                        item_point: attachment.point(),
                        effect_point: effect.point(),
                    },
                    effect.model(),
                    &self.animations,
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
                self.animation_time_ms(),
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
                    && placement
                        .playback
                        .as_ref()
                        .map(M2PlaybackStorage::borrow)
                        .is_some_and(|playback| {
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
                sound_lifetime: Default::default(),
                light_lifetime: Default::default(),
                entity_lighting: Default::default(),
                placement_valid: true,
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
                    | M2GpuPlacementOwner::UnitItem { guid: 0, .. }
                    | M2GpuPlacementOwner::UnitItemVisual { guid: 0, .. }
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
        self.retire_removed_models();
        let Some(input) = input else {
            self.remove_player();
            return Ok(());
        };
        let mut prepared = [prepare_character_gpu(
            self,
            renderer,
            &input,
            M2GpuPlacementOwner::PlayerBody { guid: input.guid() },
            self.animation_time_ms(),
            random,
        )?];
        self.retain_character_instances(&mut prepared);
        self.remove_player();
        for character in prepared {
            self.publish_character(character);
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
        self.retire_removed_models();
        let mut prepared = Vec::with_capacity(inputs.len());
        let mut retained = Vec::with_capacity(inputs.len());
        for input in inputs {
            if self.placements.iter().any(|placement| {
                placement.owner == (M2GpuPlacementOwner::CreatureBody { guid: input.guid() })
                    && placement
                        .unit_presentation
                        .as_ref()
                        .is_some_and(|generation| generation.matches(input.generation()))
            }) {
                retained.push(input.guid());
                continue;
            }
            let mut character = M2PreparedCharacter::default();
            let mount = input.mount();
            if let Some(mount) = mount {
                prepare_mount_gpu(
                    self,
                    renderer,
                    &mut character,
                    UnitMountGpuInput {
                        mount,
                        body_owner: M2GpuPlacementOwner::CreatureBody { guid: input.guid() },
                        world_transform: input.world_transform(),
                        animation: input.unit_animation(),
                    },
                    self.animation_time_ms(),
                    random,
                )?;
            }
            let resolved = input
                .textures()
                .iter()
                .map(|texture| match texture {
                    ResidentCreatureTexture::Authored(source) => {
                        M2ResolvedTexture::Authored(source.as_ref())
                    }
                    ResidentCreatureTexture::StockWhite => M2ResolvedTexture::StockWhite,
                    ResidentCreatureTexture::StockFailure => M2ResolvedTexture::StockFailure,
                })
                .collect::<Vec<_>>();
            let source = prepare_gpu_source(
                renderer,
                input.model(),
                &resolved,
                input.geosets().map(M2GeosetSelection::from),
                M2LocalLightCount::Four,
                M2ModelOrientation::Authored,
            )?;
            let transform = unit_placement_transform(
                input.world_transform(),
                mount.map_or(input.object_scale(), |mount| mount.object_scale()),
            )?;
            let mut placement = if let Some(animation) = input.unit_animation() {
                animation.synchronize(self.animation_time_ms() as u32, random)?;
                let mut placement = m2_gpu_placement(
                    0,
                    transform,
                    M2GpuPlacementOwner::CreatureBody { guid: input.guid() },
                    input.model(),
                    Some(M2PlaybackStorage::Shared(animation.playback())),
                    input.particle_colors().cloned(),
                    self.animation_time_ms() as u32,
                )?;
                placement.unit_animation = Some(Rc::clone(animation));
                placement
            } else {
                unit_gpu_placement(
                    self.animation_time_ms(),
                    transform,
                    M2GpuPlacementOwner::CreatureBody { guid: input.guid() },
                    input.model(),
                    input.animation().animation_id(),
                    input.particle_colors().cloned(),
                    random,
                )?
            };
            placement.scene_registration = Some(UnitSceneRegistration::new(
                mount.map_or(input.model().as_ref(), |mount| mount.model().as_ref()),
                transform,
            )?);
            placement.unit_presentation = Some(input.generation().clone());
            placement.rider_scale = mount.map_or(1.0, |mount| mount.rider_scale());
            placement.ground_placement =
                input
                    .unit_animation()
                    .filter(|_| mount.is_none())
                    .map(|animation| UnitGroundPlacement {
                        position: input.world_transform().position(),
                        scale: input.object_scale(),
                        owner: Rc::clone(animation),
                    });
            character.push(source, placement);
            prepare_unit_equipment_gpu(
                self,
                renderer,
                &mut character,
                UnitEquipmentGpuInput {
                    guid: input.guid(),
                    model: input.model(),
                    attachments: input.attachments(),
                    animation: input.unit_animation(),
                    body_owner: M2GpuPlacementOwner::CreatureBody { guid: input.guid() },
                    world_transform: transform,
                    atlas: None,
                },
                |slot| {
                    input
                        .armor_display_id(slot)
                        .map(|display_id| M2UnitItemIdentity::NpcArmor { slot, display_id })
                        .or_else(|| {
                            input
                                .virtual_item_entry(slot)
                                .map(|entry_id| M2UnitItemIdentity::NpcVirtual { slot, entry_id })
                        })
                },
                self.animation_time_ms(),
                random,
            )?;
            prepared.push(character);
        }

        self.retain_character_instances(&mut prepared);
        self.remove_creatures(&retained);
        for character in prepared {
            self.publish_character(character);
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
        self.retire_removed_models();
        let mut prepared = Vec::with_capacity(inputs.len());
        let mut retained = Vec::with_capacity(inputs.len());
        for input in inputs {
            if self.placements.iter().any(|placement| {
                placement.owner == (M2GpuPlacementOwner::RemotePlayerBody { guid: input.guid() })
                    && placement
                        .unit_presentation
                        .as_ref()
                        .is_some_and(|generation| generation.matches(input.generation()))
            }) {
                retained.push(input.guid());
                continue;
            }
            prepared.push(prepare_character_gpu(
                self,
                renderer,
                input,
                M2GpuPlacementOwner::RemotePlayerBody { guid: input.guid() },
                self.animation_time_ms(),
                random,
            )?);
        }
        self.retain_character_instances(&mut prepared);
        self.remove_remote_players(&retained);
        for character in prepared {
            self.publish_character(character);
        }
        Ok(())
    }

    /// A material rebuild changes GPU resources, not the living model's
    /// emitter histories. Transfer them only after every replacement has
    /// prepared successfully, and only within the same unit/model lifetime.
    fn retain_unit_effects<'a>(
        &mut self,
        prepared: impl IntoIterator<Item = &'a mut M2GpuPlacement>,
    ) {
        for replacement in prepared {
            let Some(animation) = &replacement.unit_animation else {
                continue;
            };
            let Some(previous) = self.placements.iter_mut().find(|previous| {
                previous.owner == replacement.owner
                    && previous
                        .unit_animation
                        .as_ref()
                        .is_some_and(|previous| Rc::ptr_eq(previous, animation))
            }) else {
                continue;
            };
            std::mem::swap(&mut previous.particles, &mut replacement.particles);
            std::mem::swap(&mut previous.ribbons, &mut replacement.ribbons);
            replacement.last_effect_time_ms = previous.last_effect_time_ms;
        }
    }

    /// Finds the first dynamic owner without scanning static scenery on settled
    /// frames. Publication may have changed placement order since the previous
    /// draw preparation, so dirty metadata must use the current records.
    fn dynamic_placement_index(&self, owner: M2GpuPlacementOwner) -> Option<usize> {
        debug_assert!(!matches!(owner, M2GpuPlacementOwner::Static(_)));
        if self.placement_topology_dirty {
            self.placements
                .iter()
                .position(|placement| placement.owner == owner)
        } else {
            self.placement_visibility.dynamic_owner_index(owner)
        }
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
            let placement_index = self
                .dynamic_placement_index(M2GpuPlacementOwner::PlayerMount { guid: input.guid() })
                .ok_or(RuntimeTerrainFrameError::MissingPlayerMountM2Placement {
                    guid: input.guid(),
                })?;
            let placement = &mut self.placements[placement_index];
            placement.transform = transform;
            placement.local_transform = transform;
            placement.ground_placement =
                input.unit_animation().map(|animation| UnitGroundPlacement {
                    position: input.world_transform().position(),
                    scale: mount.object_scale(),
                    owner: Rc::clone(animation),
                });
            let Some(source) = self.sources[placement.source_index].as_ref() else {
                return Ok(());
            };
            if let Some(animation) = input.unit_animation() {
                animation.synchronize(animation_time_ms as u32, random)?;
            } else if let Some(mut playback) = placement
                .playback
                .as_mut()
                .map(M2PlaybackStorage::borrow_mut)
            {
                playback.select_mount_animation(
                    &source.model,
                    mount.animation().animation_id(),
                    None,
                    (1., 0),
                    animation_time_ms,
                    solarity_rendering::M2SequenceStartPhase::BeforeSceneUpdate,
                    random,
                )?;
            }
            transform
        } else {
            unit_placement_transform(input.world_transform(), input.object_scale())?
        };
        let placement_index = self
            .dynamic_placement_index(M2GpuPlacementOwner::PlayerBody { guid: input.guid() })
            .ok_or(RuntimeTerrainFrameError::MissingPlayerM2Placement { guid: input.guid() })?;
        let placement = &mut self.placements[placement_index];
        placement.transform = transform;
        placement.local_transform = transform;
        placement.rider_scale = input.mount().map_or(1.0, |mount| mount.rider_scale());
        placement.scene_registration = Some(UnitSceneRegistration::new(
            input
                .mount()
                .map_or(input.model().as_ref(), |mount| mount.model().as_ref()),
            transform,
        )?);
        let Some(source) = self.sources[placement.source_index].as_ref() else {
            return Ok(());
        };
        if let Some(animation) = input.unit_animation() {
            placement.ground_placement = input.mount().is_none().then_some(UnitGroundPlacement {
                position: input.world_transform().position(),
                scale: input.object_scale(),
                owner: Rc::clone(animation),
            });
            animation.synchronize(animation_time_ms as u32, random)?;
            placement.unit_animation = Some(Rc::clone(animation));
            placement.playback = Some(M2PlaybackStorage::Shared(animation.playback()));
            return Ok(());
        }
        if let Some(mut playback) = placement
            .playback
            .as_mut()
            .map(M2PlaybackStorage::borrow_mut)
        {
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
            let transform = if let Some(mount) = input.mount() {
                let transform =
                    unit_placement_transform(input.world_transform(), mount.object_scale())?;
                let placement_index = self
                    .dynamic_placement_index(M2GpuPlacementOwner::CreatureMount {
                        guid: input.guid(),
                    })
                    .ok_or(RuntimeTerrainFrameError::MissingCreatureMountM2Placement {
                        guid: input.guid(),
                    })?;
                let placement = &mut self.placements[placement_index];
                placement.transform = transform;
                placement.local_transform = transform;
                placement.ground_placement =
                    input.unit_animation().map(|animation| UnitGroundPlacement {
                        position: input.world_transform().position(),
                        scale: mount.object_scale(),
                        owner: Rc::clone(animation),
                    });
                let Some(source) = self.sources[placement.source_index].as_ref() else {
                    continue;
                };
                if let Some(animation) = input.unit_animation() {
                    animation.synchronize(animation_time_ms as u32, random)?;
                } else if let Some(mut playback) = placement
                    .playback
                    .as_mut()
                    .map(M2PlaybackStorage::borrow_mut)
                {
                    playback.select_mount_animation(
                        &source.model,
                        mount.animation().animation_id(),
                        None,
                        (1., 0),
                        animation_time_ms,
                        solarity_rendering::M2SequenceStartPhase::BeforeSceneUpdate,
                        random,
                    )?;
                }
                transform
            } else {
                unit_placement_transform(input.world_transform(), input.object_scale())?
            };
            let placement_index = self
                .dynamic_placement_index(M2GpuPlacementOwner::CreatureBody { guid: input.guid() })
                .ok_or(RuntimeTerrainFrameError::MissingCreatureM2Placement {
                    guid: input.guid(),
                })?;
            let placement = &mut self.placements[placement_index];
            placement.transform = transform;
            placement.local_transform = transform;
            placement.rider_scale = input.mount().map_or(1.0, |mount| mount.rider_scale());
            placement.scene_registration = Some(UnitSceneRegistration::new(
                input
                    .mount()
                    .map_or(input.model().as_ref(), |mount| mount.model().as_ref()),
                transform,
            )?);
            let Some(source) = self.sources[placement.source_index].as_ref() else {
                continue;
            };
            if let Some(animation) = input.unit_animation() {
                placement.ground_placement =
                    input.mount().is_none().then_some(UnitGroundPlacement {
                        position: input.world_transform().position(),
                        scale: input.object_scale(),
                        owner: Rc::clone(animation),
                    });
                animation.synchronize(animation_time_ms as u32, random)?;
                placement.unit_animation = Some(Rc::clone(animation));
                placement.playback = Some(M2PlaybackStorage::Shared(animation.playback()));
                continue;
            }
            if let Some(mut playback) = placement
                .playback
                .as_mut()
                .map(M2PlaybackStorage::borrow_mut)
            {
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
                let placement_index = self
                    .dynamic_placement_index(M2GpuPlacementOwner::RemotePlayerMount {
                        guid: input.guid(),
                    })
                    .ok_or(
                        RuntimeTerrainFrameError::MissingRemotePlayerMountM2Placement {
                            guid: input.guid(),
                        },
                    )?;
                let placement = &mut self.placements[placement_index];
                placement.transform = transform;
                placement.local_transform = transform;
                placement.ground_placement =
                    input.unit_animation().map(|animation| UnitGroundPlacement {
                        position: input.world_transform().position(),
                        scale: mount.object_scale(),
                        owner: Rc::clone(animation),
                    });
                let Some(source) = self.sources[placement.source_index].as_ref() else {
                    continue;
                };
                if let Some(animation) = input.unit_animation() {
                    animation.synchronize(animation_time_ms as u32, random)?;
                } else if let Some(mut playback) = placement
                    .playback
                    .as_mut()
                    .map(M2PlaybackStorage::borrow_mut)
                {
                    playback.select_mount_animation(
                        &source.model,
                        mount.animation().animation_id(),
                        None,
                        (1., 0),
                        animation_time_ms,
                        solarity_rendering::M2SequenceStartPhase::BeforeSceneUpdate,
                        random,
                    )?;
                }
                transform
            } else {
                unit_placement_transform(input.world_transform(), input.object_scale())?
            };
            let placement_index = self
                .dynamic_placement_index(M2GpuPlacementOwner::RemotePlayerBody {
                    guid: input.guid(),
                })
                .ok_or(RuntimeTerrainFrameError::MissingRemotePlayerM2Placement {
                    guid: input.guid(),
                })?;
            let placement = &mut self.placements[placement_index];
            placement.transform = transform;
            placement.local_transform = transform;
            placement.rider_scale = input.mount().map_or(1.0, |mount| mount.rider_scale());
            placement.scene_registration = Some(UnitSceneRegistration::new(
                input
                    .mount()
                    .map_or(input.model().as_ref(), |mount| mount.model().as_ref()),
                transform,
            )?);
            let Some(source) = self.sources[placement.source_index].as_ref() else {
                continue;
            };
            if let Some(animation) = input.unit_animation() {
                placement.ground_placement =
                    input.mount().is_none().then_some(UnitGroundPlacement {
                        position: input.world_transform().position(),
                        scale: input.object_scale(),
                        owner: Rc::clone(animation),
                    });
                animation.synchronize(animation_time_ms as u32, random)?;
                placement.unit_animation = Some(Rc::clone(animation));
                placement.playback = Some(M2PlaybackStorage::Shared(animation.playback()));
                continue;
            }
            if let Some(mut playback) = placement
                .playback
                .as_mut()
                .map(M2PlaybackStorage::borrow_mut)
            {
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
                M2GpuPlacementOwner::Retired(_) => false,
                M2GpuPlacementOwner::PlayerBody { .. }
                | M2GpuPlacementOwner::PlayerMount { .. }
                | M2GpuPlacementOwner::GluePet => true,
                M2GpuPlacementOwner::UnitItem { guid, .. }
                | M2GpuPlacementOwner::UnitItemVisual { guid, .. } => local_guid == Some(guid),
                M2GpuPlacementOwner::UnitEffect { .. }
                | M2GpuPlacementOwner::Static(_)
                | M2GpuPlacementOwner::GameObjectWorldModelDoodad { .. }
                | M2GpuPlacementOwner::GlueModel { .. }
                | M2GpuPlacementOwner::RemotePlayerBody { .. }
                | M2GpuPlacementOwner::RemotePlayerMount { .. }
                | M2GpuPlacementOwner::CreatureBody { .. }
                | M2GpuPlacementOwner::CreatureMount { .. }
                | M2GpuPlacementOwner::GameObject { .. } => false,
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
    fn remove_creatures(&mut self, retained: &[u64]) {
        self.placement_topology_dirty = true;
        let removed_guids = self
            .placements
            .iter()
            .filter_map(|placement| match placement.owner {
                M2GpuPlacementOwner::CreatureBody { guid } if !retained.contains(&guid) => {
                    Some(guid)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut creature_sources = Vec::new();
        self.placements.retain(|placement| {
            let owned = match placement.owner {
                M2GpuPlacementOwner::CreatureBody { guid }
                | M2GpuPlacementOwner::CreatureMount { guid } => !retained.contains(&guid),
                M2GpuPlacementOwner::UnitItem { guid, .. }
                | M2GpuPlacementOwner::UnitItemVisual { guid, .. } => removed_guids.contains(&guid),
                _ => false,
            };
            if owned {
                creature_sources.push(placement.source_index);
            }
            !owned
        });
        for source_index in creature_sources {
            if let Some(source) = self.sources.get_mut(source_index) {
                *source = None;
            }
        }
    }

    /// Drops remote character bodies and every child placement they own.
    fn remove_remote_players(&mut self, retained: &[u64]) {
        self.placement_topology_dirty = true;
        let remote_guids = self
            .placements
            .iter()
            .filter_map(|placement| match placement.owner {
                M2GpuPlacementOwner::RemotePlayerBody { guid } if !retained.contains(&guid) => {
                    Some(guid)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut remote_sources = Vec::new();
        self.placements.retain(|placement| {
            let owned = match placement.owner {
                M2GpuPlacementOwner::Retired(_) => false,
                M2GpuPlacementOwner::RemotePlayerBody { guid }
                | M2GpuPlacementOwner::RemotePlayerMount { guid } => !retained.contains(&guid),
                M2GpuPlacementOwner::UnitItem { guid, .. }
                | M2GpuPlacementOwner::UnitItemVisual { guid, .. } => remote_guids.contains(&guid),
                M2GpuPlacementOwner::UnitEffect { .. }
                | M2GpuPlacementOwner::Static(_)
                | M2GpuPlacementOwner::GameObjectWorldModelDoodad { .. }
                | M2GpuPlacementOwner::GlueModel { .. }
                | M2GpuPlacementOwner::GluePet
                | M2GpuPlacementOwner::PlayerBody { .. }
                | M2GpuPlacementOwner::PlayerMount { .. }
                | M2GpuPlacementOwner::CreatureBody { .. }
                | M2GpuPlacementOwner::CreatureMount { .. }
                | M2GpuPlacementOwner::GameObject { .. } => false,
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
    pub(in crate::application) fn advance_unbound_passengers(
        &mut self,
        scene: &crate::application::unit_animation::UnitAnimationScene,
        now: f32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.vehicle_passengers.advance_unbound(
            scene,
            &mut self.placements,
            &self.sources,
            &self.requested_items,
            now,
            random,
        )
    }

    pub(in crate::application) fn animation_time_ms(&self) -> f32 {
        self.animation_started_at.elapsed().as_secs_f32() * 1_000.0
    }

    /// Transfers playback back to its widget owner when this GPU frame retires.
    pub(in crate::application) fn take_glue_playback(&mut self) -> Option<M2Playback> {
        self.placements
            .iter_mut()
            .find(|placement| matches!(placement.owner, M2GpuPlacementOwner::GlueModel { .. }))
            .and_then(|placement| placement.playback.take())
            .and_then(M2PlaybackStorage::into_local)
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
        let mut playback = placement
            .playback
            .as_mut()
            .map(M2PlaybackStorage::borrow_mut)
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
        let mut playback = placement
            .playback
            .as_mut()
            .map(M2PlaybackStorage::borrow_mut)
            .ok_or(RuntimeTerrainFrameError::MissingGlueM2Placement)?;
        let advance = playback.clock(&source.model, animation_time_ms, random)?;
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
        first_transparent_pass: M2TransparentPass,
        fog_color: glam::Vec3,
        animation_time_ms: f32,
        effect_scale: M2CameraEffectScale,
        random: &mut CrtRand,
        game_objects: Option<GameObjectFrameInput<'_>>,
    ) -> Result<M2VisibleFrame<'_>, RuntimeTerrainFrameError> {
        self.prepare_visible_draws_with_unit_effects(
            renderer,
            frustum,
            camera,
            first_transparent_pass,
            fog_color,
            animation_time_ms,
            effect_scale,
            random,
            game_objects,
            None,
            None,
            None,
            None,
            None,
        )
    }

    pub(in crate::application) fn set_unit_effect_sources(
        &mut self,
        sources: Arc<unit_effects::M2UnitEffectSources>,
    ) {
        self.unit_effects.sources = Some(sources);
    }

    pub(in crate::application) fn emit_unit_effect(
        &mut self,
        request: unit_effects::UnitEffectRequest,
        now: f32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.unit_effects
            .emit(request, &self.animations, now, random)
    }

    /// Unit callbacks construct CEffect models before this frame's effect pass.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_visible_draws_with_unit_effects(
        &mut self,
        renderer: &VulkanRenderer,
        frustum: WorldFrustum,
        camera: WorldCameraFrame,
        first_transparent_pass: M2TransparentPass,
        fog_color: glam::Vec3,
        animation_time_ms: f32,
        effect_scale: M2CameraEffectScale,
        random: &mut CrtRand,
        game_objects: Option<GameObjectFrameInput<'_>>,
        unit_effect_callback: Option<&mut unit_effects::UnitEffectEventCallback<'_>>,
        world_lighting: Option<(
            solarity_rendering::M2SceneUniform,
            solarity_rendering::M2DirectionalLight,
        )>,
        mut spatial_lighting: Option<(
            &mut crate::application::terrain_coordinator::RuntimeTerrainCoordinator,
            solarity_systems::WorldEntityLightEnvironment,
            glam::Vec3,
            &solarity_asset::LiquidTypeCatalog,
        )>,
        shadow_projection: Option<solarity_rendering::WorldShadowProjection>,
        scenery_shadows: Option<super::shadow::SceneryShadowQueries<'_>>,
    ) -> Result<M2VisibleFrame<'_>, RuntimeTerrainFrameError> {
        let mut frame_profile = RuntimeFrameProfile::new("M2 frame preparation");
        let frame_seconds = ((animation_time_ms - self.unit_scene_time_ms) * 0.001).max(0.0);
        self.unit_scene_time_ms = animation_time_ms;
        if let Some((terrain, ..)) = spatial_lighting.as_mut() {
            terrain.prepare_world_scene(camera)?;
        }
        if let Some(game_objects) = game_objects {
            game_objects.advance_scene(animation_time_ms, random)?;
        }
        self.advance_retired_models(animation_time_ms as u32, game_objects);
        self.bone_transforms.clear();
        self.visible_draws.clear();
        self.shadow_draws.clear();
        self.shadow_admission.clear();
        self.environment_shadow_draws.clear();
        self.environment_shadow_admission.clear();
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
        self.scene_lighting.clear();
        let mut particle_vertex_capacity = 0_usize;
        let mut particle_index_capacity = 0_usize;
        frame_profile.mark("scene setup");
        self.unit_effects
            .publish_loaded(&self.animations, animation_time_ms, random)?;
        self.unit_effects.begin_frame();
        // Current topology excludes every ordinary model before the first
        // effect. Publication can invalidate that boundary before this pass.
        let first_effect = if self.placement_topology_dirty {
            0
        } else {
            self.placement_visibility.effect_start()
        };
        if self
            .unit_effects
            .retire_drained(&mut self.placements, first_effect)
        {
            self.placement_topology_dirty = true;
            self.compact_sources();
        }
        if self.placement_topology_dirty {
            let mut profile = RuntimeFrameProfile::new("M2 placement topology");
            // Changes to body residency can append a parent after retained
            // CEffects. Keep every effect behind its current parent pose.
            self.placements
                .sort_by_key(|placement| placement.unit_effect.is_some());
            profile.mark("effect ordering");
            self.requested_items.clear();
            self.requested_items
                .extend(
                    self.placements
                        .iter()
                        .filter_map(|placement| match placement.owner {
                            M2GpuPlacementOwner::Retired(_) => None,
                            M2GpuPlacementOwner::UnitItem { guid, point } => Some((guid, point)),
                            M2GpuPlacementOwner::UnitEffect { .. }
                            | M2GpuPlacementOwner::Static(_)
                            | M2GpuPlacementOwner::GameObjectWorldModelDoodad { .. }
                            | M2GpuPlacementOwner::GlueModel { .. }
                            | M2GpuPlacementOwner::GluePet
                            | M2GpuPlacementOwner::PlayerBody { .. }
                            | M2GpuPlacementOwner::PlayerMount { .. }
                            | M2GpuPlacementOwner::RemotePlayerBody { .. }
                            | M2GpuPlacementOwner::RemotePlayerMount { .. }
                            | M2GpuPlacementOwner::CreatureBody { .. }
                            | M2GpuPlacementOwner::CreatureMount { .. }
                            | M2GpuPlacementOwner::GameObject { .. }
                            | M2GpuPlacementOwner::UnitItemVisual { .. } => None,
                        }),
                );
            self.requested_visuals.clear();
            self.requested_visuals
                .extend(
                    self.placements
                        .iter()
                        .filter_map(|placement| match placement.owner {
                            M2GpuPlacementOwner::Retired(_) => None,
                            M2GpuPlacementOwner::UnitItemVisual {
                                guid,
                                item_point,
                                effect_point,
                            } => Some((guid, item_point, effect_point)),
                            M2GpuPlacementOwner::UnitEffect { .. }
                            | M2GpuPlacementOwner::Static(_)
                            | M2GpuPlacementOwner::GameObjectWorldModelDoodad { .. }
                            | M2GpuPlacementOwner::GlueModel { .. }
                            | M2GpuPlacementOwner::GluePet
                            | M2GpuPlacementOwner::PlayerBody { .. }
                            | M2GpuPlacementOwner::PlayerMount { .. }
                            | M2GpuPlacementOwner::RemotePlayerBody { .. }
                            | M2GpuPlacementOwner::RemotePlayerMount { .. }
                            | M2GpuPlacementOwner::CreatureBody { .. }
                            | M2GpuPlacementOwner::CreatureMount { .. }
                            | M2GpuPlacementOwner::GameObject { .. }
                            | M2GpuPlacementOwner::UnitItem { .. } => None,
                        }),
                );
            self.mounted_guids.clear();
            self.mounted_guids
                .extend(
                    self.placements
                        .iter()
                        .filter_map(|placement| match placement.owner {
                            M2GpuPlacementOwner::PlayerMount { guid }
                            | M2GpuPlacementOwner::RemotePlayerMount { guid }
                            | M2GpuPlacementOwner::CreatureMount { guid } => Some(guid),
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
            profile.mark("attachment membership");
            self.placement_visibility
                .rebuild(&self.placements, &self.sources);
            self.vehicle_passengers.invalidate();
            profile.mark("visibility rebuild");
            self.placement_topology_dirty = false;
        }
        frame_profile.mark("residency and topology");
        self.vehicle_passengers.prepare_timing(
            &mut self.placements,
            &self.sources,
            &self.placement_visibility,
            &self.requested_items,
            camera.view(),
            animation_time_ms,
            random,
        )?;
        // Prepare unit selection and yaw before placement. Authored events and
        // completion follow below in scene traversal order, before camera culling.
        for &index in self.placement_visibility.dynamic_indices() {
            let placement = &mut self.placements[index];
            if let Some(animation) = &placement.unit_animation {
                if let Some(registration) = placement.scene_registration
                    && let Some((terrain, ..)) = spatial_lighting.as_mut()
                    && terrain.unit_scene_admits(registration.position, registration.bounds)?
                {
                    animation.admit_scene_collision();
                }
                animation.prepare_scene(animation_time_ms, random)?;
                placement.transform =
                    placement.local_transform * animation.body_pose().placement_rotation;
            }
        }
        // Ground placement follows unit animation/yaw for every model, including
        // mounts inserted before their riders. It does not advance a second
        // unit callback or substitute the rider's model/timer for the mount.
        for &index in self.placement_visibility.dynamic_indices() {
            let placement = &mut self.placements[index];
            let Some(ground) = &placement.ground_placement else {
                continue;
            };
            if ground.owner.passenger_input().is_some() {
                continue;
            }
            if !ground.owner.uses_ground_placement() {
                continue;
            }
            if placement.unit_animation.is_some() {
                placement.transform = ground.owner.ground_transform(
                    ground.position,
                    ground.scale,
                    animation_time_ms,
                    frame_seconds,
                )?;
            } else if let Some(source) = &self.sources[placement.source_index]
                && let Some(playback) = &placement.playback
            {
                placement.transform = ground.owner.ground_model_transform(
                    ground.position,
                    ground.scale,
                    animation_time_ms,
                    frame_seconds,
                    &source.model,
                    &playback.borrow(),
                )?;
            }
        }
        self.vehicle_passengers.prepare(
            &mut self.placements,
            &self.sources,
            &self.placement_visibility,
            &self.requested_items,
            camera.view(),
            animation_time_ms,
            random,
        )?;
        self.placement_visibility
            .set_vehicle_parents(self.vehicle_passengers.parents());
        self.advance_unit_callbacks(camera, animation_time_ms, random, unit_effect_callback)?;
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
        frame_profile.mark("dynamic models");
        if let Some((terrain, ..)) = spatial_lighting.as_ref() {
            self.doodad_scene.prepare(
                terrain,
                &self.placement_visibility,
                &mut self.placements,
                &self.sources,
                camera.camera().position(),
                self.environment_detail,
            )?;
        }
        frame_profile.mark("WMO doodad admission");
        let effect_start = self.placement_visibility.effect_start();
        let mut next_placement = 0;
        loop {
            if next_placement == effect_start
                && self
                    .unit_effects
                    .publish(&mut self.placements, &mut self.sources)
            {
                self.placement_topology_dirty = true;
                self.placement_visibility
                    .rebuild(&self.placements, &self.sources);
                self.placement_visibility
                    .set_vehicle_parents(self.vehicle_passengers.parents());
            }
            if next_placement == self.placements.len() {
                break;
            }
            let placement_index = next_placement;
            next_placement += 1;
            self.shadow_admission.push(false);
            self.environment_shadow_admission.push(0);
            // Moving-parent transforms have already been resolved. Forward
            // attachments query that same root; earlier roots reuse admission.
            let environment_maps = if let Some(queries) = scenery_shadows {
                let root = self
                    .placement_visibility
                    .light_root(placement_index)
                    .unwrap_or(placement_index);
                if root < placement_index {
                    self.environment_shadow_admission[root]
                } else if !self.vehicle_passengers.hidden(root)
                    && let Some(source) = &self.sources[self.placements[root].source_index]
                {
                    shadow::environment_maps(
                        queries,
                        source,
                        &self.placements[root],
                        camera.camera().position(),
                        self.environment_detail,
                    )?
                } else {
                    0
                }
            } else {
                0
            };
            let publishes_lights =
                world_lighting.is_some() && self.placement_visibility.has_lights(placement_index);
            let bounds = self.placement_visibility.bounds()[placement_index];
            let doodad_scene_active = spatial_lighting.is_some()
                && self
                    .placement_visibility
                    .is_world_model_doodad(placement_index);
            let doodad_fog = self.doodad_scene.fog_bank(placement_index);
            let doodad_visible = !doodad_scene_active || doodad_fog.is_some();
            if !doodad_visible && !publishes_lights && environment_maps == 0 {
                continue;
            }
            let mut placement_fog_color = if doodad_scene_active && doodad_fog == Some(false) {
                spatial_lighting
                    .as_ref()
                    .map_or(fog_color, |(_, _, ordinary, _)| *ordinary)
            } else {
                fog_color
            };
            let scenery_opacity = if doodad_scene_active {
                self.doodad_scene.opacity(placement_index)
            } else {
                self.placement_visibility.opacity(
                    placement_index,
                    camera.camera().position(),
                    self.environment_detail,
                )
            };
            if scenery_opacity == 0.0 && !publishes_lights && environment_maps == 0 {
                continue;
            }
            if let Some((center, radius)) = bounds
                && !publishes_lights
                && !doodad_scene_active
                && environment_maps == 0
                && !frustum.contains_sphere(center, radius)?
            {
                continue;
            }
            // Forward vehicle parents already have their final model matrices
            // from the ancestry pass. Admission needs no second animation tick.
            let forward_shadow = if let Some(projection) = shadow_projection
                && self
                    .placement_visibility
                    .light_parent(placement_index)
                    .is_some_and(|parent| parent >= placement_index)
            {
                if let Some(root) = self.placement_visibility.light_root(placement_index) {
                    let placement = &self.placements[root];
                    if root < placement_index {
                        self.shadow_admission[root]
                    } else if placement.placement_valid
                        && !placement
                            .entity_opacity
                            .as_ref()
                            .is_some_and(|owner| owner.hidden())
                        && let Some(source) = &self.sources[placement.source_index]
                    {
                        shadow::admits_root(
                            projection,
                            source,
                            placement,
                            scenery_shadows.map(|queries| queries.admission),
                        )?
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };
            // 82F0F0 retains the containing model's +88 distance for ordinary
            // attachments; mesh, particle and ribbon queues consume that lane.
            let inherited_model_distance = self
                .placement_visibility
                .light_parent(placement_index)
                .and_then(|_| self.placement_visibility.light_root(placement_index))
                .map(|root| m2_model_distance_key(camera.view() * self.placements[root].transform));
            let (root_liquid, owner_fog) =
                if let Some((terrain, _, _, liquid_types)) = spatial_lighting.as_mut() {
                    let root = self
                        .placement_visibility
                        .light_root(placement_index)
                        .unwrap_or(placement_index);
                    let root = &mut self.placements[root];
                    if root.placement_valid
                        && !root
                            .entity_opacity
                            .as_ref()
                            .is_some_and(|owner| owner.hidden())
                        && let Some(source) = &self.sources[root.source_index]
                    {
                        root.entity_lighting.scene_state(
                            root.retirement
                                .as_ref()
                                .map_or(root.owner, |retired| retired.original_owner),
                            &source.model,
                            root.local_transform,
                            root.scene_registration,
                            terrain,
                            liquid_types,
                            camera.view(),
                        )?
                    } else {
                        (solarity_rendering::M2LiquidState::Above, None)
                    }
                } else {
                    (solarity_rendering::M2LiquidState::Above, None)
                };
            if owner_fog == Some(false) {
                placement_fog_color = spatial_lighting
                    .as_ref()
                    .map_or(fog_color, |(_, _, ordinary, _)| *ordinary);
            }
            let placement = &mut self.placements[placement_index];
            if self.vehicle_passengers.hidden(placement_index) {
                if let M2GpuPlacementOwner::PlayerMount { guid }
                | M2GpuPlacementOwner::RemotePlayerMount { guid }
                | M2GpuPlacementOwner::CreatureMount { guid }
                | M2GpuPlacementOwner::PlayerBody { guid }
                | M2GpuPlacementOwner::RemotePlayerBody { guid }
                | M2GpuPlacementOwner::CreatureBody { guid } = placement.owner
                {
                    self.rider_transforms.push((guid, None));
                }
                continue;
            }
            let shadow_opacity = placement.opacity
                * placement
                    .retirement
                    .as_ref()
                    .map_or(1.0, |owner| owner.opacity())
                * placement
                    .entity_opacity
                    .as_ref()
                    .map_or(1.0, |owner| owner.opacity());
            let placement_opacity = shadow_opacity * scenery_opacity;
            self.unit_effects.prepare_attachment(placement);
            if !self.retirement.prepare_attachment(placement) {
                continue;
            }
            if let Some(effect) = &mut placement.unit_effect
                && let Some(event) = effect.take_ready_sound(placement.transform.w_axis.truncate())
            {
                self.triggered_events.push(event);
            }
            if !placement.placement_valid {
                continue;
            }
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
            | M2GpuPlacementOwner::RemotePlayerBody { guid }
            | M2GpuPlacementOwner::CreatureBody { guid } = placement.owner
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
                placement.transform =
                    transform * Mat4::from_scale(glam::Vec3::splat(placement.rider_scale));
            }
            if let M2GpuPlacementOwner::UnitItem { guid, point } = placement.owner {
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
            if let M2GpuPlacementOwner::UnitItemVisual {
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
            // ADT/WMO placements were culled from compact immutable bounds
            // before touching instance state. Replicated WMO doodads can move
            // with their parent and require their current transform here.
            // CEffects can leave live particles outside their authored model
            // bounds. Keep their update and particle packets in the scene;
            // 821BEE advances registered roots and 828A00 their children.
            let static_visibility_resolved = matches!(
                owner,
                M2GpuPlacementOwner::UnitEffect { .. }
                    | M2GpuPlacementOwner::Static(_)
                    | M2GpuPlacementOwner::GameObjectWorldModelDoodad { .. }
            );
            if matches!(
                owner,
                M2GpuPlacementOwner::GameObjectWorldModelDoodad { .. }
            ) && !publishes_lights
                && !doodad_scene_active
                && environment_maps == 0
            {
                let (center, radius) =
                    placement_bounding_sphere(&source.model, placement.transform);
                if !frustum.contains_sphere(center, radius)? {
                    continue;
                }
            }
            let Some(mut playback) = placement
                .playback
                .as_mut()
                .map(M2PlaybackStorage::borrow_mut)
            else {
                continue;
            };
            let scene_sample = if let M2GpuPlacementOwner::GameObject { identity, .. } = owner
                && let Some(game_objects) = game_objects
                && let Some(instance) = game_objects.get(identity)
            {
                instance
                    .behavior()
                    .and_then(|behavior| behavior.take_scene_sample())
                    .or_else(|| {
                        instance
                            .transport_model()
                            .and_then(|model| model.take_scene_sample())
                    })
                    .map(|sample| (sample.advance, sample.event_window))
            } else {
                placement
                    .unit_animation
                    .as_ref()
                    .and_then(|animation| animation.take_scene_sample())
                    .map(|sample| (sample.advance, sample.event_window))
            };
            let (advance, prepared_event_window) = if let Some((advance, event_window)) =
                scene_sample
            {
                (advance, Some(event_window))
            } else {
                let advance = if let Some(advance) = placement.passenger_playback_advance.take() {
                    advance
                } else if let Some(effect) = &mut placement.unit_effect {
                    effect.advance(&mut playback, &source.model, animation_time_ms, random)?
                } else if matches!(owner, M2GpuPlacementOwner::GlueModel { .. }) {
                    self.pending_glue_playback_advance.take().map_or_else(
                        || playback.clock(&source.model, animation_time_ms, random),
                        Ok,
                    )?
                } else {
                    playback.clock(&source.model, animation_time_ms, random)?
                };
                (advance, None)
            };
            let body_pose = placement
                .unit_animation
                .as_ref()
                .map(|animation| animation.body_pose())
                .or_else(|| {
                    placement
                        .retirement
                        .as_ref()?
                        .unit_pose
                        .map(|pose| pose.body)
                });
            let bone_transforms = body_pose
                .as_ref()
                .map_or(&[][..], |pose| pose.bone_transforms());
            for expired in advance.expired_variations {
                self.bone_pose_scratch.recompose_with_overrides(
                    source.model.animations(),
                    expired.clock,
                    camera.view() * placement.transform,
                    M2BonePoseOverrides {
                        model_oriented_billboard_bones: &source.model_oriented_billboard_bones,
                        bone_transforms,
                        bone_sequences: &expired.bone_sequences,
                        ..Default::default()
                    },
                )?;
                let first_event = self.triggered_events.len();
                append_triggered_events(
                    &mut self.triggered_events,
                    &source.model,
                    placement.owner,
                    placement.transform,
                    &placement.sound_lifetime,
                    &self.bone_pose_scratch,
                    expired.event_window,
                )?;
                if let Some(effect) = &placement.unit_effect {
                    effect.bind_sound_events(&mut self.triggered_events[first_event..]);
                }
            }
            let clock = advance.clock;
            let bone_sequences =
                playback.bone_sequence_clocks(&source.model, clock, animation_time_ms as u32);
            let finger_pose_hands = placement
                .retirement
                .as_ref()
                .and_then(|retired| retired.finger_hands)
                .or_else(|| held_item_finger_pose(&self.requested_items, owner));
            let finger_pose = finger_pose_hands.and_then(|hands| {
                source
                    .model
                    .animations()
                    .sequence_for_variation(15, 0)
                    .map(|sequence| {
                        (
                            M2AnimationClock::new_with_global_tick(
                                sequence,
                                0.0,
                                playback.global_tick(animation_time_ms as u32),
                            ),
                            hands,
                        )
                    })
            });
            let event_window =
                prepared_event_window.unwrap_or_else(|| playback.event_window(animation_time_ms));
            drop(playback);
            let model_view = camera.view() * placement.transform;
            let model_bounds = source.model.bounds();
            let particle_liquid = root_liquid.classify_model(
                (model_bounds.minimum() + model_bounds.maximum()) * 0.5,
                model_bounds.sphere_radius(),
                model_view,
                true,
                false,
            );
            let model_liquid = particle_liquid.with_clipping_support(
                renderer.m2_liquid_clipping_enabled(),
                first_transparent_pass == M2TransparentPass::One,
            );
            let instance_identity = std::ptr::from_ref(&*placement).addr();
            let instance_distance =
                inherited_model_distance.unwrap_or_else(|| m2_model_distance_key(model_view));
            self.bone_pose_scratch.recompose_with_overrides(
                source.model.animations(),
                clock,
                model_view,
                M2BonePoseOverrides {
                    model_oriented_billboard_bones: &source.model_oriented_billboard_bones,
                    finger_pose,
                    bone_transforms,
                    bone_sequences: &bone_sequences,
                },
            )?;
            let bone_pose = &self.bone_pose_scratch;
            self.retirement
                .publish_attachments(placement, &source.model, bone_pose, clock)?;
            let first_event = self.triggered_events.len();
            append_triggered_events(
                &mut self.triggered_events,
                &source.model,
                placement.owner,
                placement.transform,
                &placement.sound_lifetime,
                bone_pose,
                event_window,
            )?;
            if let Some(effect) = &placement.unit_effect {
                effect.bind_sound_events(&mut self.triggered_events[first_event..]);
            }
            if let Some(animation) = &placement.unit_animation {
                self.unit_effects.update_anchor(
                    animation,
                    &source.model,
                    bone_pose,
                    placement.transform,
                )?;
            }
            if publishes_lights {
                solarity_rendering::sample_m2_scene_lights_into(
                    source.model.animations(),
                    bone_pose,
                    clock,
                    placement.transform,
                    &mut self.scene_lighting.sample_directional,
                    &mut self.scene_lighting.sample_points,
                )?;
                self.scene_lighting.publish(
                    placement.light_lifetime.get_or_init(|| Rc::new(())),
                    placement_mesh_color(placement.owner, placement.color).w * placement_opacity,
                )?;
            }
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
            | M2GpuPlacementOwner::RemotePlayerMount { guid }
            | M2GpuPlacementOwner::CreatureMount { guid } = placement.owner
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
            | M2GpuPlacementOwner::RemotePlayerBody { guid }
            | M2GpuPlacementOwner::CreatureBody { guid } = placement.owner
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
            if let M2GpuPlacementOwner::UnitItem { guid, point } = placement.owner {
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
            // 4F8D10 updates unit state/placement before clearing model activity.
            // Preserve attachment samples, but hidden player hierarchies publish
            // no model lights, shadow packets, visible effects or mesh packets.
            if placement
                .entity_opacity
                .as_ref()
                .is_some_and(|owner| owner.hidden())
            {
                continue;
            }
            let scene_index = if world_lighting.is_some() {
                // 831AF0 queries matrix F4's translation at +124 with radius
                // zero. Authored mesh bounds do not move the lighting center.
                let center = placement.transform.w_axis.truncate();
                let parent = self.placement_visibility.light_parent(placement_index);
                let callback = if parent.is_none()
                    && let Some((terrain, environment, _, _)) = spatial_lighting.as_mut()
                {
                    placement.entity_lighting.sample(
                        placement
                            .retirement
                            .as_ref()
                            .map_or(placement.owner, |retired| retired.original_owner),
                        &source.model,
                        placement.transform,
                        placement.color,
                        animation_time_ms,
                        terrain,
                        *environment,
                    )?
                } else {
                    None
                };
                Some(self.scene_lighting.receiver_with_light(
                    placement_index,
                    parent,
                    center,
                    callback,
                    (doodad_scene_active || owner_fog.is_some()).then_some(placement_fog_color),
                )?)
            } else {
                None
            };
            // Native shadow traversal uses the light volume independently of
            // camera visibility, and attached models inherit root admission.
            let shadow_admitted = if let Some(projection) = shadow_projection {
                if let Some(parent) = self.placement_visibility.light_parent(placement_index) {
                    if parent < placement_index {
                        self.shadow_admission[parent]
                    } else {
                        forward_shadow
                    }
                } else {
                    shadow::admits_root(
                        projection,
                        source,
                        placement,
                        scenery_shadows.map(|queries| queries.admission),
                    )?
                }
            } else {
                false
            };
            self.shadow_admission[placement_index] = shadow_admitted;
            self.environment_shadow_admission[placement_index] = environment_maps;
            let shadow_bone_offset = u32::try_from(self.bone_transforms.len())
                .map_err(|_source| solarity_rendering::VulkanError::M2BoneTransformRange)?;
            let first_shadow_draw = self.shadow_draws.len();
            let first_environment_draw = self.environment_shadow_draws.len();
            if shadow_admitted || environment_maps != 0 {
                shadow::append_packets(
                    renderer,
                    source,
                    placement,
                    clock,
                    model_view,
                    shadow_opacity,
                    shadow_bone_offset,
                    |draw| {
                        if shadow_admitted {
                            self.shadow_draws.push(draw);
                        }
                        if environment_maps != 0 {
                            self.environment_shadow_draws.push(
                                solarity_rendering::WorldEnvironmentM2Caster {
                                    draw,
                                    maps: environment_maps,
                                },
                            );
                        }
                    },
                )?;
            }
            let has_shadow_bones = self.shadow_draws.len() != first_shadow_draw
                || self.environment_shadow_draws.len() != first_environment_draw;
            if has_shadow_bones {
                self.bone_transforms
                    .extend_from_slice(bone_pose.transforms());
            }
            if environment_maps != 0 && scenery_opacity == 0. {
                continue;
            }
            if doodad_scene_active {
                if !doodad_visible || scenery_opacity == 0.0 {
                    continue;
                }
            } else if !static_visibility_resolved
                || environment_maps != 0
                || (publishes_lights && placement.unit_effect.is_none())
            {
                let (center, radius) =
                    placement_bounding_sphere(&source.model, placement.transform);
                if !frustum.contains_sphere(center, radius)? {
                    continue;
                }
            }

            // 0x00828A00 advances a model from its own previous effect update.
            // The scene clock and subtraction wrap as unsigned milliseconds.
            let effect_time_ms = animation_time_ms as u32;
            let effect_delta_seconds =
                effect_time_ms.wrapping_sub(placement.last_effect_time_ms) as f32 * 0.001;
            placement.last_effect_time_ms = effect_time_ms;

            let bone_offset = shadow_bone_offset;
            let light_bank = placement_light_bank(placement.owner);
            let effect_retiring = placement
                .unit_effect
                .as_ref()
                .is_some_and(|effect| effect.retiring());
            let mut instance_color = placement_mesh_color(placement.owner, placement.color);
            if let Some(animation) = &placement.unit_animation {
                instance_color *= placement_color(animation.model_color().to_le_bytes());
            } else if let Some(pose) = placement
                .retirement
                .as_ref()
                .and_then(|retired| retired.unit_pose)
            {
                instance_color *= placement_color(pose.color.to_le_bytes());
            }
            instance_color.w *= placement_opacity;
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
                if effect_retiring {
                    simulation.set_emission_enabled(false);
                }
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
                        instance_color.w,
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
                let opaque =
                    !M2MaterialState::from_particle(emitter.blending_type(), emitter.flags())
                        .blend_enabled()
                        && M2ElementAlphaState::classify(instance_color.w)
                            == M2ElementAlphaState::Authored;
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
            advance_ribbons(
                &source.model,
                placement,
                bone_pose,
                clock,
                effect_delta_seconds,
                effect_scale,
                instance_color.w,
            )?;
            if !has_shadow_bones {
                self.bone_transforms
                    .extend_from_slice(bone_pose.transforms());
            }
            if let Some(mesh) = source.mesh
                && !effect_retiring
            {
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
                        placement_fog_color.extend(1.0),
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
                        .with_light_bank(light_bank)
                        .with_scene_index(scene_index);
                    if draw.transparent_sort_unit()
                        || alpha_state == M2ElementAlphaState::Translucent
                    {
                        let section_distance = section_distance_key(draw, bone_pose, model_view)?;
                        let primary_distance = if self
                            .placement_visibility
                            .model_distance_sort(placement_index)
                        {
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
                            self.visible_draws.push(
                                prepared.with_liquid_clip_plane(model_liquid.clip_plane(pass)),
                            );
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
            for (ribbon_index, ((emitter, trail), passes)) in source
                .model
                .animations()
                .ribbons()
                .iter()
                .zip(&placement.ribbons)
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
                let vertex_count =
                    M2RibbonMeshPlan::append(emitter, trail, &mut self.ribbon_vertices)?;
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
        }
        frame_profile.mark("instance traversal");
        self.transparent_elements.sort_unstable_by(|left, right| {
            (left.pass != first_transparent_pass)
                .cmp(&(right.pass != first_transparent_pass))
                .then_with(|| compare_m2_transparent(&left.key, &right.key))
        });
        let first_transparent_order = scene_element_count(
            self.visible_draws.len(),
            self.particle_draws.len(),
            self.ribbon_draws.len(),
        )?;
        let water_scene_order = first_transparent_order
            .checked_add(
                self.transparent_elements
                    .iter()
                    .take_while(|element| element.pass == first_transparent_pass)
                    .count(),
            )
            .and_then(|index| u32::try_from(index).ok())
            .ok_or(solarity_rendering::VulkanError::M2DrawIndexRange)?;
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
                M2TransparentDrawIndex::Ribbon { first, count } => {
                    let draws = self
                        .ribbon_draws
                        .get_mut(first..first + count)
                        .ok_or(solarity_rendering::VulkanError::M2RibbonDrawVertexRange)?;
                    for draw in draws {
                        *draw = draw.with_scene_order(scene_order);
                    }
                }
            }
        }
        self.visible_draws.sort_by_key(|draw| draw.scene_order());
        self.particle_draws.sort_by_key(|draw| draw.scene_order());
        self.ribbon_draws.sort_by_key(|draw| draw.scene_order());
        frame_profile.mark("transparent order");
        if let Some((base, exterior)) = world_lighting {
            self.scene_lighting.finish(base, exterior)?;
        }
        frame_profile.mark("scene lights");
        Ok(M2VisibleFrame {
            instance_scenes: &self.scene_lighting.scenes,
            scene_points: &self.scene_lighting.points,
            scene_directionals: self.scene_lighting.directionals(),
            water_scene_order,
            bone_transforms: &self.bone_transforms,
            draws: &self.visible_draws,
            shadow_draws: &self.shadow_draws,
            environment_shadow_draws: &self.environment_shadow_draws,
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

/// Reserves opaque scene positions before either liquid-dependent queue.
fn scene_element_count(
    meshes: usize,
    particles: usize,
    ribbons: usize,
) -> Result<usize, solarity_rendering::VulkanError> {
    meshes
        .checked_add(particles)
        .and_then(|count| count.checked_add(ribbons))
        .ok_or(solarity_rendering::VulkanError::M2DrawIndexRange)
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
        | M2GpuPlacementOwner::RemotePlayerBody { guid }
        | M2GpuPlacementOwner::CreatureBody { guid } => guid,
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
/// Resolves the parent-first placement relations already used for transforms.
fn placement_parent_index(
    placements: &[M2GpuPlacement],
    placement_index: usize,
    placement: &M2GpuPlacement,
) -> Option<usize> {
    let preceding = &placements[..placement_index];
    if let Some(retired) = &placement.retirement {
        return retired.parent().and_then(|parent| {
            preceding
                .iter()
                .rposition(|candidate| candidate.owner == parent)
        });
    }
    if placement.glue_parent_attachment.is_some() {
        return preceding.iter().rposition(|candidate| {
            matches!(candidate.owner, M2GpuPlacementOwner::GlueModel { .. })
        });
    }
    match placement.owner {
        M2GpuPlacementOwner::Retired(_) => None,
        M2GpuPlacementOwner::UnitEffect { .. } => preceding.iter().rposition(|candidate| {
            candidate.unit_animation.as_ref().is_some_and(|owner| {
                placement
                    .unit_effect
                    .as_ref()
                    .is_some_and(|effect| effect.attached_to(owner))
            })
        }),
        M2GpuPlacementOwner::PlayerBody { guid } => preceding
            .iter()
            .rposition(|candidate| candidate.owner == M2GpuPlacementOwner::PlayerMount { guid }),
        M2GpuPlacementOwner::RemotePlayerBody { guid } => preceding.iter().rposition(|candidate| {
            candidate.owner == M2GpuPlacementOwner::RemotePlayerMount { guid }
        }),
        M2GpuPlacementOwner::CreatureBody { guid } => preceding
            .iter()
            .rposition(|candidate| candidate.owner == M2GpuPlacementOwner::CreatureMount { guid }),
        M2GpuPlacementOwner::UnitItem { guid, .. } => preceding.iter().rposition(|candidate| {
            candidate.owner == M2GpuPlacementOwner::PlayerBody { guid }
                || candidate.owner == M2GpuPlacementOwner::RemotePlayerBody { guid }
                || candidate.owner == M2GpuPlacementOwner::CreatureBody { guid }
        }),
        M2GpuPlacementOwner::UnitItemVisual {
            guid, item_point, ..
        } => preceding.iter().rposition(|candidate| {
            candidate.owner
                == M2GpuPlacementOwner::UnitItem {
                    guid,
                    point: item_point,
                }
        }),
        M2GpuPlacementOwner::Static(_)
        | M2GpuPlacementOwner::GameObjectWorldModelDoodad { .. }
        | M2GpuPlacementOwner::GlueModel { .. }
        | M2GpuPlacementOwner::GluePet
        | M2GpuPlacementOwner::PlayerMount { .. }
        | M2GpuPlacementOwner::RemotePlayerMount { .. }
        | M2GpuPlacementOwner::CreatureMount { .. }
        | M2GpuPlacementOwner::GameObject { .. } => None,
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
    sound_lifetime: &std::cell::OnceCell<Rc<sound::M2SoundKind>>,
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
        let mut callback = RuntimeM2Event::new(
            event.identifier(),
            event.data(),
            (model_transform * bone).transform_point3(event.position()),
            placement_owner_guid(owner),
        );
        callback.sound_kind = sound::M2SoundKind::for_placement(owner);
        if event.identifier() == *b"$CSD"
            && callback.sound_kind.is_none()
            && callback.owner_guid.is_some()
        {
            // 746D60 selects attachment 17 through 8273D0 / 831330. The
            // position query uses the bone and authored point without the
            // attachment enable track; its fallback is origin + world Z*2.
            callback.position = if let Some(attachment) = model.attachment(17) {
                let bone = bone_pose
                    .transforms()
                    .get(usize::from(attachment.bone_index()))
                    .ok_or(solarity_rendering::M2BonePoseError::AttachmentBoneIndex {
                        requested: attachment.bone_index(),
                        available: bone_pose.transforms().len(),
                    })?;
                (model_transform * *bone).transform_point3(attachment.position())
            } else {
                model_transform.transform_point3(glam::Vec3::ZERO) + glam::Vec3::Z * 2.0
            };
        }
        if matches!(&event.identifier(), b"$DSL" | b"$DSE")
            && let Some(kind) = callback.sound_kind
        {
            let lifetime = sound_lifetime.get_or_init(|| Rc::new(kind));
            callback = callback.with_sound_owner(sound::M2SoundOwner::new(lifetime));
        }
        destination.push(callback);
    }
    Ok(())
}

const fn placement_owner_guid(owner: M2GpuPlacementOwner) -> Option<u64> {
    match owner {
        M2GpuPlacementOwner::Retired(_) => None,
        M2GpuPlacementOwner::UnitEffect { .. }
        | M2GpuPlacementOwner::Static(_)
        | M2GpuPlacementOwner::GameObjectWorldModelDoodad { .. }
        | M2GpuPlacementOwner::GlueModel { .. }
        | M2GpuPlacementOwner::GluePet => None,
        M2GpuPlacementOwner::PlayerBody { guid }
        | M2GpuPlacementOwner::PlayerMount { guid }
        | M2GpuPlacementOwner::RemotePlayerBody { guid }
        | M2GpuPlacementOwner::RemotePlayerMount { guid }
        | M2GpuPlacementOwner::CreatureBody { guid }
        | M2GpuPlacementOwner::CreatureMount { guid }
        | M2GpuPlacementOwner::GameObject { guid, .. }
        | M2GpuPlacementOwner::UnitItem { guid, .. }
        | M2GpuPlacementOwner::UnitItemVisual { guid, .. } => Some(guid),
    }
}

/// Maps resident ownership to the three independent banks populated by Glue Lua.
const fn placement_light_bank(owner: M2GpuPlacementOwner) -> M2SceneLightBank {
    match owner {
        M2GpuPlacementOwner::Retired(_) => M2SceneLightBank::Environment,
        M2GpuPlacementOwner::PlayerBody { guid: 0 }
        | M2GpuPlacementOwner::UnitItem { guid: 0, .. }
        | M2GpuPlacementOwner::UnitItemVisual { guid: 0, .. } => M2SceneLightBank::Character,
        M2GpuPlacementOwner::GluePet => M2SceneLightBank::Pet,
        M2GpuPlacementOwner::UnitEffect { .. }
        | M2GpuPlacementOwner::Static(_)
        | M2GpuPlacementOwner::GameObjectWorldModelDoodad { .. }
        | M2GpuPlacementOwner::GlueModel { .. }
        | M2GpuPlacementOwner::PlayerBody { .. }
        | M2GpuPlacementOwner::PlayerMount { .. }
        | M2GpuPlacementOwner::RemotePlayerBody { .. }
        | M2GpuPlacementOwner::RemotePlayerMount { .. }
        | M2GpuPlacementOwner::CreatureBody { .. }
        | M2GpuPlacementOwner::CreatureMount { .. }
        | M2GpuPlacementOwner::GameObject { .. }
        | M2GpuPlacementOwner::UnitItem { .. }
        | M2GpuPlacementOwner::UnitItemVisual { .. } => M2SceneLightBank::Environment,
    }
}

/// Equipment and enchantment constructors (4EAA70/4EA8F0) use the model's
/// ordinary load-completion default, without a separate primary request.
fn default_gpu_placement(
    scene_time_ms: f32,
    transform: Mat4,
    owner: M2GpuPlacementOwner,
    model: &DecodedM2Model,
    animations: &AnimationDataCatalog,
    particle_colors: Option<M2ParticleColorReplacement>,
    random: &mut CrtRand,
) -> Result<M2GpuPlacement, RuntimeTerrainFrameError> {
    let playback = M2Playback::default_sequence(model, animations, scene_time_ms as u32, random)?;
    m2_gpu_placement(
        0,
        transform,
        owner,
        model,
        Some(M2PlaybackStorage::Local(playback)),
        particle_colors,
        scene_time_ms as u32,
    )
}

/// Creates placement-local animation and effect histories for one living M2.
fn unit_gpu_placement(
    scene_time_ms: f32,
    transform: Mat4,
    owner: M2GpuPlacementOwner,
    model: &DecodedM2Model,
    animation_id: u16,
    particle_colors: Option<M2ParticleColorReplacement>,
    random: &mut CrtRand,
) -> Result<M2GpuPlacement, RuntimeTerrainFrameError> {
    let playback = M2Playback::new(model, animation_id, scene_time_ms as u32, random)?;
    m2_gpu_placement(
        0,
        transform,
        owner,
        model,
        playback.map(M2PlaybackStorage::Local),
        particle_colors,
        scene_time_ms as u32,
    )
}

/// Constructs the effect owner around playback selected by its behavior.
fn m2_gpu_placement(
    source_index: usize,
    transform: Mat4,
    owner: M2GpuPlacementOwner,
    model: &DecodedM2Model,
    playback: Option<M2PlaybackStorage>,
    particle_colors: Option<M2ParticleColorReplacement>,
    scene_time_ms: u32,
) -> Result<M2GpuPlacement, RuntimeTerrainFrameError> {
    let particles = stock_particle_simulations(model);
    let ribbons = model
        .animations()
        .ribbons()
        .iter()
        .map(M2RibbonTrail::new)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(M2GpuPlacement {
        sound_lifetime: Default::default(),
        light_lifetime: Default::default(),
        entity_lighting: Default::default(),
        placement_valid: true,
        scene_indoor_fog: false,
        world_model_state: None,
        source_index,
        local_transform: transform,
        ground_placement: None,
        scene_registration: None,
        rider_scale: 1.0,
        transform,
        orientation: M2ModelOrientation::Authored,
        glue_parent_attachment: None,
        owner,
        flags: 0,
        color: [255; 4],
        opacity: 1.0,
        entity_opacity: None,
        retirement: None,
        particle_colors,
        playback,
        passenger_playback_advance: None,
        unit_animation: None,
        unit_presentation: None,
        mount_key: None,
        item_identity: None,
        particles,
        ribbons,
        last_effect_time_ms: scene_time_ms,
        unit_effect: None,
    })
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
    instance_alpha: f32,
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
        let pose = M2RibbonPose::sample(model.animations(), emitter, clock)?
            .with_instance_alpha(instance_alpha);
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

/// Converts MODD's BGRA bytes to shader RGBA; MDDF already stores white.
fn placement_mesh_color(owner: M2GpuPlacementOwner, color: [u8; 4]) -> glam::Vec4 {
    if entity_lighting::is_doodad(owner) {
        glam::Vec4::ONE
    } else {
        placement_color(color)
    }
}

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

/// Publishes a source only when every selected draw has concrete BLP stages.
fn prepare_source(
    renderer: &mut VulkanRenderer,
    source: &ResidentM2Source,
) -> Result<Option<M2GpuSource>, RuntimeTerrainFrameError> {
    prepare_source_with_lights(renderer, source, M2LocalLightCount::Four)
}

fn prepare_source_with_lights(
    renderer: &mut VulkanRenderer,
    source: &ResidentM2Source,
    local_light_count: M2LocalLightCount,
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
        local_light_count,
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
    let mut profile =
        crate::application::frame_profile::RuntimeFrameProfile::new("M2 source publication");
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
    profile.mark("validation and BLP uploads");
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

    profile.mark("special texture uploads");
    let mesh = if plan.has_drawable_geometry() {
        Some(renderer.upload_m2_mesh(&plan)?)
    } else {
        None
    };
    profile.mark("mesh upload");
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
            let sampler = renderer.prepare_m2_file_sampler(&model.textures()[texture_index])?;
            stages.push(sampled_texture(texture, sampler));
        }
        texture_requests.push(texture_set(model, draw_index, &stages)?);
    }
    profile.mark("mesh pipelines and samplers");
    let texture_sets = renderer.prepare_m2_texture_sets(&texture_requests)?;
    profile.mark("mesh descriptors");
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
        let sampler = renderer.prepare_m2_file_sampler(&model.textures()[texture_slot])?;
        let material = M2MaterialState::from_particle(emitter.blending_type(), emitter.flags());
        let mut prepare_pipeline =
            |material| -> Result<M2ParticlePipelineHandle, RuntimeTerrainFrameError> {
                match cpu_source {
                    Some(cpu_source) => {
                        let program =
                            cpu_source.particle_programs.get(&material).ok_or_else(|| {
                                RuntimeTerrainFrameError::M2CpuProgram {
                                    model: model.path().clone(),
                                    domain: "particle",
                                }
                            })?;
                        renderer
                            .prepare_precompiled_m2_particle_pipeline(program)
                            .map_err(Into::into)
                    }
                    None => renderer
                        .prepare_m2_particle_pipeline(material)
                        .map_err(Into::into),
                }
            };
        let pipeline = prepare_pipeline(material)?;
        let runtime_fade_pipeline = if material.blend_enabled() {
            pipeline
        } else {
            prepare_pipeline(material.with_runtime_alpha_fade())?
        };
        particle_pipelines.push((pipeline, runtime_fade_pipeline));
        particle_texture_requests.push(M2TextureSet::One(sampled_texture(texture, sampler)));
    }
    let particle_texture_sets = renderer.prepare_m2_texture_sets(&particle_texture_requests)?;
    let particles = particle_pipelines
        .into_iter()
        .zip(particle_texture_sets)
        .map(
            |((pipeline, runtime_fade_pipeline), texture_set)| M2GpuParticle {
                pipeline,
                runtime_fade_pipeline,
                texture_set,
            },
        )
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
            let sampler = renderer.prepare_m2_file_sampler(&model.textures()[texture_slot])?;
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
    profile.mark("effects");
    Ok(M2GpuSource {
        model: Arc::clone(model),
        plan,
        model_oriented_billboard_bones,
        animated_shadow_caster: model
            .animations()
            .bones()
            .iter()
            .any(|bone| bone.flags() & 0x2f8 != 0),
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
