//! Renderer-local resources for the shared resident placed-M2 scene.

#[cfg(test)]
#[path = "../../../../tests/application/game_object_scene.rs"]
mod game_object_scene_tests;

#[cfg(test)]
#[path = "../../../../tests/application/unit_effect_models.rs"]
mod unit_effect_model_tests;

mod admission;
mod ancestry;
#[cfg(test)]
#[path = "../../../../tests/application/m2_attachment_inputs.rs"]
mod attachment_input_tests;
mod attachments;
pub(super) use admission::M2SourceAdmission;
use attachments::{
    ItemRequests, ItemSamples, MountedOwners, RiderSamples, VisualRequests, VisualSamples,
};
mod character_residency;
mod distance;
mod doodad_scene;
mod entity_lighting;
mod frame_work;
mod game_objects;
mod placements;
mod playback;
mod portrait;
mod preparation;
mod retirement;
#[cfg(test)]
#[path = "../../../../tests/application/scenery_distance.rs"]
mod scenery_distance_tests;
mod shadow;
pub(in crate::application) mod sky;
pub(in crate::application) mod sound;
mod source;
use solarity_asset::ResourceLease;
use source::{
    prepare_gpu_source, prepare_gpu_source_from_cpu, prepare_source, prepare_source_with_lights,
};
#[cfg(test)]
#[path = "../../../../tests/application/static_m2_streaming.rs"]
mod static_streaming_tests;
mod streaming;
mod topology;
pub(in crate::application) use source::{
    M2CpuSource, M2GlueCpuSourceKey, M2GluePipelineWarmup, prepare_m2_cpu_source,
};
pub(in crate::application) mod unit_effects;
mod unit_registration;
mod unit_scene;
#[cfg(test)]
#[path = "../../../../tests/application/unit_shadow_scene.rs"]
mod unit_shadow_tests;
mod units;
mod vehicle_passengers;
mod visibility;

use crate::application::entity_opacity::EntityOpacityOwner;
use crate::application::unit_animation::UnitAnimationBehavior;
use character_residency::M2UnitItemIdentity;
use playback::M2PlaybackStorage;
use preparation::geometry::advance_ribbons;
use unit_registration::UnitSceneRegistration;

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use crate::application::model_playback::{M2Playback, M2PlaybackAdvance};
use glam::Mat4;
use solarity_asset::{
    AnimationDataCatalog, AssetPath, BlpTextureSource, DecodedM2Model, M2ParticleEmitter,
};
use solarity_ecs::{WorldObjectIdentity, WorldTransform};
use solarity_rendering::{
    BlpColorSpace, BlpTextureUploadRequest, CharacterAtlasTexture, CharacterAttachmentPoint,
    CharacterGeosetPlan, CreatureGeosetPlan, M2AnimationClock, M2BonePose, M2BonePoseOverrides,
    M2BoneSamples, M2BoneTransforms, M2CameraEffectScale, M2DrawCall, M2EffectOrder,
    M2ElementAlphaState, M2EventTimeWindow, M2FingerPoseHands, M2LocalLightCount, M2MaterialPose,
    M2MaterialState, M2MaterialUniform, M2MeshHandle, M2MeshPlan, M2ModelOrientation,
    M2ParticleColorReplacement, M2ParticleMeshPlan, M2ParticleMeshPlanError,
    M2ParticlePipelineHandle, M2ParticlePose, M2ParticlePreparedDraw, M2ParticleRenderVertex,
    M2ParticleSimulation, M2ParticleTwinkleTable, M2PipelineHandle, M2PreparedDraw,
    M2RibbonMeshPlan, M2RibbonPose, M2RibbonPreparedDraw, M2RibbonRenderVertex, M2RibbonTrail,
    M2SampledTexture, M2SceneLightBank, M2ShaderPermutation, M2ShaderPlan, M2ShadowFiltering,
    M2ShadowPermutation, M2SpirvKey, M2TextureImageHandle, M2TextureSet, M2TextureSetHandle,
    M2TransparentPass, M2TransparentSortKey, VulkanRenderer, WorldCameraFrame, WorldFrustum,
    compare_m2_transparent, m2_model_distance_key, m2_section_distance_key, sample_m2_lights_into,
    triggered_m2_event_indices,
};

use crate::application::game_object_coordinator::{
    GameObjectFrameInput, GameObjectWorldModelState,
};
use crate::application::player_coordinator::{
    MountModelKey, ResidentCreatureGeosets, ResidentCreatureTexture,
    ResidentGlueCharacterFrameInput, ResidentPlayerFrameInput, ResidentPlayerTexture,
    UnitPresentationGeneration,
};
use crate::application::terrain_coordinator::m2_residency::{
    ResidentM2Owner, ResidentM2Scene, ResidentM2Source, ResidentM2Texture,
};
use crate::random::CrtRand;

use super::RuntimeTerrainFrameError;
#[cfg(test)]
#[path = "../../../../tests/application/placement_fixtures.rs"]
mod placement_fixtures;
mod scene_lighting;
pub(super) use scene_lighting::SceneLightInputs;
#[cfg(test)]
#[path = "../../../../tests/application/scene_lighting.rs"]
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
struct M2GpuSourceData {
    _resource_leases: Vec<solarity_rendering::GpuResourceLease>,
    model: ResourceLease<DecodedM2Model>,
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

/// One immutable generation pins templates and GPU resources for worker consumers.
type M2GpuSource = Arc<M2GpuSourceData>;

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
    template: solarity_rendering::M2DrawTemplate,
    fade_template: Option<solarity_rendering::M2DrawTemplate>,
    pipeline: M2PipelineHandle,
    texture_set: M2TextureSetHandle,
}

/// Shared renderer objects for one ordinary particle declaration.
#[derive(Clone)]
struct M2GpuParticle {
    /// Cached authored output bound; it does not change the live simulation pool.
    maximum_particles: usize,
    template: solarity_rendering::M2ParticleDrawTemplate,
    fade_template: solarity_rendering::M2ParticleDrawTemplate,
}

/// One stock ribbon pass pairing parallel material and texture entries.
#[derive(Clone)]
struct M2GpuRibbonPass {
    template: solarity_rendering::M2RibbonDrawTemplate,
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
    /// Static owners retain their worker-prepared spatial data through index remaps.
    static_spatial: Option<crate::application::m2_spatial::StaticM2Spatial>,
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
    placements: placements::M2PlacementStorage,
    static_residency: streaming::StaticM2Residency,
    prepared_static: Vec<admission::PreparedStaticM2>,
    particle_twinkle: Arc<M2ParticleTwinkleTable>,
    animation_started_at: std::time::Instant,
    /// Previous scene pass, independent of unit residency and draw admission.
    unit_scene_time_ms: f32,
    retirement: retirement::M2RetirementScene,
    bone_pose_scratch: M2BonePose,
    /// CPU samples cannot be mistaken for a complete render palette.
    bone_samples_scratch: M2BoneSamples,
    bone_demand: preparation::demand::CpuBoneDemand,
    pose_batch: preparation::poses::PoseBatch,
    spatial_batch: preparation::spatial::SpatialBatch,
    receiver_frame: preparation::receivers::ReceiverFrame,
    /// Reused only between shadow and ordinary draws of the current placement.
    material_pose_scratch: Vec<Option<M2MaterialPose>>,
    // Only the frozen serial oracle concatenates palettes; production borrows jobs.
    #[cfg(test)]
    bone_transforms: Vec<Mat4>,
    visible_draws: Vec<M2PreparedDraw>,
    shadow_draws: Vec<M2PreparedDraw>,
    shadow_admission: Vec<bool>,
    environment_shadow_draws: Vec<solarity_rendering::WorldEnvironmentM2Caster>,
    environment_shadow_admission: Vec<u8>,
    transparent_elements: Vec<M2TransparentElement>,
    placement_topology_dirty: bool,
    placement_visibility: visibility::M2PlacementVisibility,
    frame_work: frame_work::M2FrameWork,
    doodad_scene: doodad_scene::M2DoodadScene,
    /// Stock environmentDetail is clamped by its 78DC60 CVar callback.
    pub(super) environment_detail: f32,
    particle_vertices: Vec<M2ParticleRenderVertex>,
    particle_indices: Vec<u32>,
    #[cfg(test)]
    particle_sort_indices: Vec<usize>,
    particle_draws: Vec<M2ParticlePreparedDraw>,
    ribbon_vertices: Vec<M2RibbonRenderVertex>,
    ribbon_draws: Vec<M2RibbonPreparedDraw>,
    triggered_events: Vec<RuntimeM2Event>,
    mount_camera_sample: Option<RuntimeMountCameraSample>,
    scene_lighting: scene_lighting::SceneLighting,
    glue_directional_lights: Vec<solarity_rendering::M2DirectionalLight>,
    glue_point_lights: Vec<solarity_rendering::M2PointLight>,
    requested_items: ItemRequests,
    requested_visuals: VisualRequests,
    mounted_guids: MountedOwners,
    rider_transforms: RiderSamples,
    vehicle_passengers: vehicle_passengers::M2VehiclePassengers,
    item_transforms: ItemSamples,
    visual_transforms: VisualSamples,
    glue_attachment_ids: Vec<u32>,
    glue_attachment_transforms: Vec<(u32, Option<Mat4>)>,
    recoverable_errors: Vec<String>,
    pending_glue_playback_advance: Option<M2PlaybackAdvance>,
    unit_effects: unit_effects::M2UnitEffectScene,
    /// Declared last so renderer vectors drop before their retained capacity charges.
    geometry_batch: preparation::geometry::GeometryBatch,
}

/// Borrowed dynamic streams assembled for one unified world submission.
pub(in crate::application) struct M2VisibleFrame<'frame> {
    pub(in crate::application) trace: solarity_profiling::TraceContext,
    pub(in crate::application) instance_scenes: &'frame [solarity_rendering::M2SceneUniform],
    /// Native liquid queue one belongs between the two transparent model passes.
    pub(in crate::application) water_scene_order: u32,
    pub(in crate::application) bone_transforms: &'frame dyn solarity_rendering::M2BonePaletteSource,
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
    #[cfg(test)]
    pub(super) fn log_diagnostic_workload(&self) {
        let (frame_candidates, static_distance_tests) = self.frame_work.diagnostic_counts();
        tracing::info!(
            frame_candidates,
            static_distance_tests,
            placements = self.placements.len(),
            dynamic_placements = self.placement_visibility.dynamic_indices().len(),
            sources = self.sources.iter().flatten().count(),
            visible_draws = self.visible_draws.len(),
            primary_shadow_draws = self.shadow_draws.len(),
            environment_shadow_draws = self.environment_shadow_draws.len(),
            bones = self.geometry_batch.bone_count(),
            particle_vertices = self.particle_vertices.len(),
            "live diagnostic retained model workload"
        );
    }

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
            placements: placements.into(),
            static_residency: streaming::StaticM2Residency::new(scene),
            prepared_static: Vec::new(),
            particle_twinkle,
            animation_started_at: std::time::Instant::now(),
            unit_scene_time_ms: 0.0,
            retirement: Default::default(),
            bone_pose_scratch: M2BonePose::default(),
            geometry_batch: Default::default(),
            bone_samples_scratch: M2BoneSamples::default(),
            bone_demand: preparation::demand::CpuBoneDemand::default(),
            pose_batch: preparation::poses::PoseBatch::default(),
            spatial_batch: preparation::spatial::SpatialBatch::default(),
            receiver_frame: preparation::receivers::ReceiverFrame::default(),
            material_pose_scratch: Vec::new(),
            #[cfg(test)]
            bone_transforms: Vec::new(),
            visible_draws: Vec::new(),
            shadow_draws: Vec::new(),
            shadow_admission: Vec::new(),
            environment_shadow_draws: Vec::new(),
            environment_shadow_admission: Vec::new(),
            transparent_elements: Vec::new(),
            placement_topology_dirty: true,
            placement_visibility: visibility::M2PlacementVisibility::default(),
            frame_work: frame_work::M2FrameWork::default(),
            doodad_scene: doodad_scene::M2DoodadScene::default(),
            environment_detail: 1.0,
            particle_vertices: Vec::new(),
            particle_indices: Vec::new(),
            #[cfg(test)]
            particle_sort_indices: Vec::new(),
            particle_draws: Vec::new(),
            ribbon_vertices: Vec::new(),
            ribbon_draws: Vec::new(),
            triggered_events: Vec::new(),
            mount_camera_sample: None,
            scene_lighting: scene_lighting::SceneLighting::default(),
            glue_directional_lights: Vec::new(),
            glue_point_lights: Vec::new(),
            requested_items: ItemRequests::default(),
            requested_visuals: VisualRequests::default(),
            mounted_guids: MountedOwners::default(),
            rider_transforms: RiderSamples::default(),
            vehicle_passengers: vehicle_passengers::M2VehiclePassengers::default(),
            item_transforms: ItemSamples::default(),
            visual_transforms: VisualSamples::default(),
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
        model: ResourceLease<DecodedM2Model>,
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
        let model = ResourceLease::clone(&source.model);
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
            prepared_static: Vec::new(),
            placements: vec![M2GpuPlacement {
                static_spatial: None,
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
            }]
            .into(),
            particle_twinkle,
            animation_started_at,
            unit_scene_time_ms: 0.0,
            retirement: Default::default(),
            bone_pose_scratch: M2BonePose::default(),
            geometry_batch: Default::default(),
            bone_samples_scratch: M2BoneSamples::default(),
            bone_demand: preparation::demand::CpuBoneDemand::default(),
            pose_batch: preparation::poses::PoseBatch::default(),
            spatial_batch: preparation::spatial::SpatialBatch::default(),
            receiver_frame: preparation::receivers::ReceiverFrame::default(),
            material_pose_scratch: Vec::new(),
            #[cfg(test)]
            bone_transforms: Vec::new(),
            visible_draws: Vec::new(),
            shadow_draws: Vec::new(),
            shadow_admission: Vec::new(),
            environment_shadow_draws: Vec::new(),
            environment_shadow_admission: Vec::new(),
            transparent_elements: Vec::new(),
            placement_topology_dirty: true,
            placement_visibility: visibility::M2PlacementVisibility::default(),
            frame_work: frame_work::M2FrameWork::default(),
            doodad_scene: doodad_scene::M2DoodadScene::default(),
            environment_detail: 1.0,
            particle_vertices: Vec::new(),
            particle_indices: Vec::new(),
            #[cfg(test)]
            particle_sort_indices: Vec::new(),
            particle_draws: Vec::new(),
            ribbon_vertices: Vec::new(),
            ribbon_draws: Vec::new(),
            triggered_events: Vec::new(),
            mount_camera_sample: None,
            scene_lighting: scene_lighting::SceneLighting::default(),
            glue_directional_lights: Vec::new(),
            glue_point_lights: Vec::new(),
            requested_items: ItemRequests::default(),
            requested_visuals: VisualRequests::default(),
            mounted_guids: MountedOwners::default(),
            rider_transforms: RiderSamples::default(),
            vehicle_passengers: vehicle_passengers::M2VehiclePassengers::default(),
            item_transforms: ItemSamples::default(),
            visual_transforms: VisualSamples::default(),
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
                static_spatial: None,
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
            self.placements.as_mut_slice(),
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
        cpu: &solarity_cpu::CpuExecutor,
        wait: &mut crate::application::frame_pipeline::FrameWait<'_>,
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
            cpu,
            wait,
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
    requested_items: &ItemRequests,
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
    for (_guid, point) in requested_items.for_owner(guid) {
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
    model: &ResourceLease<DecodedM2Model>,
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

/// Reproduces the direct `$CMA`/`$CFM` lookup performed on the active mount M2.
fn sample_mount_camera(
    model: &DecodedM2Model,
    model_transform: Mat4,
    bone_pose: &dyn M2BoneTransforms,
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
                .bone_transform(bone_index as usize)
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
    bone_pose: &dyn M2BoneTransforms,
    window: M2EventTimeWindow,
) -> Result<(), RuntimeTerrainFrameError> {
    for event_index in triggered_m2_event_indices(model.animations(), window) {
        let event = &model.animations().events()[event_index];
        let bone = match event.bone_index() {
            Some(bone_index) => bone_pose
                .bone_transform(bone_index as usize)
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
                    .bone_transform(usize::from(attachment.bone_index()))
                    .ok_or(solarity_rendering::M2BonePoseError::AttachmentBoneIndex {
                        requested: attachment.bone_index(),
                        available: bone_pose.bone_count(),
                    })?;
                (model_transform * bone).transform_point3(attachment.position())
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
        static_spatial: None,
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

#[cfg(test)]
#[path = "../../../../tests/application/m2_scene.rs"]
mod tests;
