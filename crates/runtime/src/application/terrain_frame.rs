//! Resident ADT composition into immutable renderer-owned resources.

use std::sync::Arc;

use glam::Vec4;
use solarity_asset::{AssetPath, BlpTextureSource, TerrainTileIndex};
use solarity_rendering::{
    BlpColorSpace, BlpTextureUploadError, M2BonePoseError, M2LocalLightCount, M2LocalLightState,
    M2MaterialPoseError, M2MeshPlanError, M2ParticleMeshPlanError, M2ParticleSimulationError,
    M2ParticleSpirvError, M2RibbonMeshPlanError, M2RibbonSpirvError, M2RibbonTrailError,
    M2SceneUniform, M2ShaderPlanError, M2SpirvError, TerrainLayerCount, TerrainLayerCountError,
    TerrainPreparedDraw, TerrainSceneUniform, TerrainTextureSet, TerrainTileMeshPlan,
    UiPreparedDraw, VulkanError, VulkanRenderer, WorldCameraError, WorldCameraFrame,
    WorldFrameReport, WorldFrameScene, WorldFrustum, WorldModelBaseMip, WorldModelMeshPlanError,
    WorldModelPlacementError, WorldModelSceneUniform, WorldModelTextureFiltering,
    WorldScreenWindow,
};
use thiserror::Error;

use crate::application::environment_coordinator::RuntimeWorldEnvironmentFrame;
use crate::application::game_object_coordinator::GameObjectFrameInput;
use crate::application::liquid::{
    LiquidGpuBatch, LiquidGpuMaterialCache, ResidentTerrainLiquidBatch, liquid_depth_images,
    liquid_environment,
};
use crate::application::player_coordinator::{
    ResidentCreatureFrameInput, ResidentPlayerFrameInput,
};
use crate::application::terrain_coordinator::m2_residency::ResidentM2Scene;
use crate::application::terrain_coordinator::world_model_residency::ResidentWorldModelScene;
use crate::random::CrtRand;

pub(in crate::application) mod m2;
mod sky;
mod streaming;
mod world_model;

use m2::M2Frame;
pub(super) use m2::{RuntimeM2Event, RuntimeMountCameraSample};
use world_model::WorldModelFrame;

/// Failure while joining a resident ADT to renderer-local GPU resources.
#[derive(Debug, Error)]
pub enum RuntimeTerrainFrameError {
    /// A current scene light or receiving model has invalid spatial data.
    #[error(transparent)]
    SceneLight(#[from] solarity_rendering::ScenePointLightError),
    /// Model interior light registration could not resolve its resident geometry.
    #[error(transparent)]
    SceneLightRegistration(#[from] super::terrain_coordinator::RuntimeMovementRegistrationError),
    /// An authored liquid scroll rate cannot form a native clock divisor.
    #[error(transparent)]
    LiquidScroll(#[from] solarity_rendering::LiquidScrollError),
    /// A selected animated surface ordinal is outside its complete resident sequence.
    #[error("liquid surface frame {index} is outside its {count}-frame sequence")]
    LiquidTextureFrame {
        /// Zero-based ordinal selected by the native animation clock.
        index: usize,
        /// Number of retained surface slots.
        count: usize,
    },
    /// A retained GameObject could not form its dedicated collision placement.
    #[error(transparent)]
    M2Collision(#[from] solarity_systems::M2CollisionError),
    /// A gameplay animation callback lost its authoritative object fields.
    #[error(transparent)]
    WorldObject(#[from] solarity_ecs::WorldStateError),
    /// An authored BLP could not decode or enter device-local storage.
    #[error(transparent)]
    TextureUpload(#[from] BlpTextureUploadError),
    /// Vulkan resource preparation or cross-resource validation failed.
    #[error(transparent)]
    Vulkan(#[from] VulkanError),
    /// An MCNK carried a diffuse-layer count outside stock's closed domain.
    #[error(transparent)]
    LayerCount(#[from] TerrainLayerCountError),
    /// The final player camera could not form a valid frustum.
    #[error(transparent)]
    Camera(#[from] WorldCameraError),
    /// A decoded WMO could not enter the combined direct-index mesh ABI.
    #[error(transparent)]
    WorldModelMesh(#[from] WorldModelMeshPlanError),
    /// An authored MODF transform could not enter presentation state.
    #[error(transparent)]
    WorldModelPlacement(#[from] WorldModelPlacementError),
    /// An M2 SKIN profile could not enter the direct-index mesh ABI.
    #[error(transparent)]
    M2Mesh(#[from] M2MeshPlanError),
    /// One M2 material batch could not select a stock BLS effect.
    #[error(transparent)]
    M2Shader(#[from] M2ShaderPlanError),
    /// A worker could not compile one ordinary M2 shader permutation.
    #[error(transparent)]
    M2Spirv(#[from] M2SpirvError),
    /// A worker could not compile one particle material permutation.
    #[error(transparent)]
    M2ParticleSpirv(#[from] M2ParticleSpirvError),
    /// A worker could not compile one ribbon material permutation.
    #[error(transparent)]
    M2RibbonSpirv(#[from] M2RibbonSpirvError),
    /// Worker-prepared shader state did not cover its immutable source plan.
    #[error("M2 model {model} is missing worker-prepared {domain} shader bytecode")]
    M2CpuProgram {
        /// Model whose CPU generation and GPU publication disagreed.
        model: AssetPath,
        /// Shader family whose exact identity was absent.
        domain: &'static str,
    },
    /// Glue attempted to publish a character model before its worker generation completed.
    #[error("Glue character M2 {model} has no worker-prepared source for {local_light_count:?}")]
    MissingGlueCpuSource {
        /// Model whose immutable CPU source is not resident.
        model: AssetPath,
        /// Exact local-light permutation required by the current ModelFFX bank.
        local_light_count: M2LocalLightCount,
    },
    /// One visible M2 could not form its bone palette.
    #[error(transparent)]
    M2BonePose(#[from] M2BonePoseError),
    /// A portrait's authored camera or fallback frame could not be sampled.
    #[error(transparent)]
    M2Camera(#[from] solarity_rendering::M2CameraFrameError),
    /// One visible M2 material could not sample its authored animation tracks.
    #[error(transparent)]
    M2MaterialPose(#[from] M2MaterialPoseError),
    /// One placed emitter could not advance its recovered ordinary path.
    #[error(transparent)]
    M2ParticleSimulation(#[from] M2ParticleSimulationError),
    /// One visible placement failed with its model and emitter identity retained.
    #[error("M2 model {model} particle {particle_index} failed at {time_ms} ms: {source}")]
    M2PlacedParticleSimulation {
        /// Authored model containing the emitter.
        model: AssetPath,
        /// Zero-based emitter index in the model.
        particle_index: usize,
        /// Placement animation sample time.
        time_ms: f32,
        /// Simulation failure from the recovered ordinary path.
        #[source]
        source: M2ParticleSimulationError,
    },
    /// Live ordinary particles could not enter the dynamic PNC0T0 mesh.
    #[error(transparent)]
    M2ParticleMesh(#[from] M2ParticleMeshPlanError),
    /// One authored ribbon could not enter bounded placement-local state.
    #[error(transparent)]
    M2RibbonTrail(#[from] M2RibbonTrailError),
    /// One live ribbon history could not enter the stock PCT0 strip ABI.
    #[error(transparent)]
    M2RibbonMesh(#[from] M2RibbonMeshPlanError),
    /// A runtime-alpha mesh lacked the pipeline prepared for stock promotion.
    #[error("M2 model {model} draw {draw_index} has no runtime-alpha fade pipeline")]
    M2RuntimeFadePipeline {
        /// Model whose animated material entered the transparent pass.
        model: AssetPath,
        /// Zero-based draw within the selected SKIN profile.
        draw_index: usize,
    },
    /// The retained MTEX sources no longer match the immutable mesh plan.
    #[error(
        "terrain MTEX source count {source_count} does not match mesh texture count {plan_count}"
    )]
    TextureTableCount {
        /// Number of retained decoded BLP sources.
        source_count: usize,
        /// Number of paths captured by mesh preparation.
        plan_count: usize,
    },
    /// One retained BLP source does not occupy its authored MTEX slot.
    #[error("terrain MTEX source {index} is {actual}, expected {expected}")]
    TextureTablePath {
        /// Authored MTEX table index.
        index: usize,
        /// Path captured by mesh preparation.
        expected: AssetPath,
        /// Path attached to the retained BLP source.
        actual: AssetPath,
    },
    /// An MCNK layer references a texture absent from the retained MTEX table.
    #[error(
        "terrain chunk {chunk_index} references MTEX index {texture_index}, but only {texture_count} textures are resident"
    )]
    TextureIndex {
        /// Row-major MCNK index in the ADT.
        chunk_index: usize,
        /// Invalid authored MTEX index.
        texture_index: u32,
        /// Number of resident MTEX textures.
        texture_count: usize,
    },
    /// Terrain reported a new resident tile without its immutable mesh plan.
    #[error("resident terrain tile [{tile_x}, {tile_y}] has no mesh plan")]
    MissingMeshPlan {
        /// Resident ADT X coordinate.
        tile_x: u8,
        /// Resident ADT Y coordinate.
        tile_y: u8,
    },
    /// Terrain reported a new resident tile without its retained MTEX sources.
    #[error("resident terrain tile [{tile_x}, {tile_y}] has no MTEX sources")]
    MissingTextureSources {
        /// Resident ADT X coordinate.
        tile_x: u8,
        /// Resident ADT Y coordinate.
        tile_y: u8,
    },
    /// Terrain reported a new tile without its admitted WMO presentation scene.
    #[error("resident terrain tile [{tile_x}, {tile_y}] has no WMO presentation scene")]
    MissingWorldModelScene {
        /// Resident ADT X coordinate.
        tile_x: u8,
        /// Resident ADT Y coordinate.
        tile_y: u8,
    },
    /// Terrain reported a new tile without its admitted M2 presentation scene.
    #[error("resident terrain tile [{tile_x}, {tile_y}] has no M2 presentation scene")]
    MissingM2Scene {
        /// Resident ADT X coordinate.
        tile_x: u8,
        /// Resident ADT Y coordinate.
        tile_y: u8,
    },
    /// Terrain residency and the retained GPU generation became inconsistent.
    #[error("current terrain tile [{tile_x}, {tile_y}] has no matching GPU generation")]
    MissingGpuGeneration {
        /// Resident ADT X coordinate.
        tile_x: u8,
        /// Resident ADT Y coordinate.
        tile_y: u8,
    },
    /// A retained GPU generation was paired with another resident tile plan.
    #[error("terrain GPU tile [{frame_x}, {frame_y}] does not match plan [{plan_x}, {plan_y}]")]
    TileMismatch {
        /// Retained GPU generation X coordinate.
        frame_x: u8,
        /// Retained GPU generation Y coordinate.
        frame_y: u8,
        /// Submitted CPU plan X coordinate.
        plan_x: u8,
        /// Submitted CPU plan Y coordinate.
        plan_y: u8,
    },
    /// A tiled and global-WMO renderer generation were paired together.
    #[error("terrain GPU generation and resident scene kind do not match")]
    SceneKindMismatch,
    /// One retained MODF references no resident source-table slot.
    #[error(
        "resident WMO placement references source {source_index}, but only {source_count} sources exist"
    )]
    WorldModelSourceIndex {
        /// Invalid source-table slot.
        source_index: usize,
        /// Number of retained root-WMO generations.
        source_count: usize,
    },
    /// One placed MDDF/MODD instance references no resident M2 source slot.
    #[error(
        "resident M2 placement references source {source_index}, but only {source_count} sources exist"
    )]
    M2SourceIndex {
        /// Invalid M2 source-table slot.
        source_index: usize,
        /// Number of resident M2 generations.
        source_count: usize,
    },
    /// Resident M2 texture sources no longer parallel the model declarations.
    #[error("M2 {model} retains {source_count} texture sources for {model_count} declarations")]
    M2TextureTableCount {
        /// Model whose immutable texture table became inconsistent.
        model: AssetPath,
        /// Number of resident texture sources.
        source_count: usize,
        /// Number of decoded model texture declarations.
        model_count: usize,
    },
    /// A validated M2 draw references no corresponding resident texture source.
    #[error("M2 {model} draw references absent resident texture {texture_index}")]
    M2TextureIndex {
        /// Model containing the selected material draw.
        model: AssetPath,
        /// Texture-declaration index selected through the SKIN combo table.
        texture_index: u16,
    },
    /// An internal M2 texture slot cannot fit the decoded 16-bit domain.
    #[error("M2 {model} texture slot {texture_index} exceeds the 16-bit format domain")]
    M2TextureIndexCapacity {
        /// Model containing the oversized internal slot.
        model: AssetPath,
        /// Unaddressable zero-based texture slot.
        texture_index: usize,
    },
    /// A selected player draw/effect requires a replacement not yet supplied.
    #[error("M2 {model} texture {texture_index} requires unresolved replacement {kind:?}")]
    M2UnresolvedTexture {
        /// Model containing the selected texture declaration.
        model: AssetPath,
        /// Zero-based texture declaration slot.
        texture_index: usize,
        /// Stock replacement category lacking an owner-provided source.
        kind: solarity_asset::M2TextureKind,
    },
    /// A selected material escaped stock's one-or-two-stage shader domain.
    #[error("M2 {model} draw {draw_index} resolved {stage_count} texture stages")]
    M2TextureStageCount {
        /// Model containing the selected material batch.
        model: AssetPath,
        /// Zero-based draw slot in the selected SKIN profile.
        draw_index: usize,
        /// Resolved stage count outside the closed shader domain.
        stage_count: usize,
    },
    /// Authoritative unit placement data cannot form a finite model matrix.
    #[error("unit M2 transform is invalid")]
    InvalidUnitM2Transform,
    /// A Glue model widget supplied a non-positive or non-finite local scale.
    #[error("Glue M2 model scale is invalid")]
    InvalidGlueM2Scale,
    /// A Glue model widget supplied a non-finite local rotation.
    #[error("Glue M2 model rotation is invalid")]
    InvalidGlueM2Rotation,
    /// Glue effective alpha is outside the normalized finite interval.
    #[error("Glue M2 opacity {opacity} is outside the normalized finite interval")]
    InvalidGlueM2Opacity {
        /// Invalid effective Model frame alpha.
        opacity: f32,
    },
    /// The retained Glue model generation has no matching model placement.
    #[error("Glue M2 model placement is unavailable")]
    MissingGlueM2Placement,
    /// The active Glue background lacks its authored character anchor.
    #[error("Glue M2 {model} has no character attachment point {attachment_id}")]
    MissingGlueM2Attachment {
        /// Glue background model lacking the selected attachment.
        model: AssetPath,
        /// Exact build-12340 parent attachment identifier.
        attachment_id: u32,
    },
    /// A Glue character was evaluated before its background attachment pose.
    #[error("Glue character attachment point {attachment_id} has no current pose")]
    MissingGlueM2AttachmentPose {
        /// Exact build-12340 parent attachment identifier.
        attachment_id: u32,
    },
    /// A current player update has no matching placement in the GPU generation.
    #[error("local player {guid:#018X} has no M2 placement in the current frame")]
    MissingPlayerM2Placement {
        /// Controlled player GUID absent from the M2 frame.
        guid: u64,
    },
    /// A remote-player update has no matching placement in the GPU generation.
    #[error("remote player {guid:#018X} has no M2 placement in the current frame")]
    MissingRemotePlayerM2Placement {
        /// Remote player GUID absent from the M2 frame.
        guid: u64,
    },
    /// A mounted local-player update has no matching mount placement.
    #[error("local player {guid:#018X} has no mount M2 placement in the current frame")]
    MissingPlayerMountM2Placement {
        /// Controlled player GUID whose mount is absent.
        guid: u64,
    },
    /// A mounted remote-player update has no matching mount placement.
    #[error("remote player {guid:#018X} has no mount M2 placement in the current frame")]
    MissingRemotePlayerMountM2Placement {
        /// Remote player GUID whose mount is absent.
        guid: u64,
    },
    /// A current creature update has no matching placement in the GPU generation.
    #[error("visible creature {guid:#018X} has no M2 placement in the current frame")]
    MissingCreatureM2Placement {
        /// Creature GUID absent from the M2 frame.
        guid: u64,
    },
    /// Equipped gear selected an attachment absent from the player body M2.
    #[error("local player M2 {model} has no attachment point {attachment_id}")]
    MissingPlayerM2Attachment {
        /// Player body model lacking the selected semantic attachment.
        model: AssetPath,
        /// Exact build-12340 attachment identifier.
        attachment_id: u32,
    },
    /// A child placement was evaluated before its body attachment pose.
    #[error("local player {guid:#018X} attachment point {attachment_id} has no current pose")]
    MissingPlayerM2AttachmentPose {
        /// Controlled player GUID.
        guid: u64,
        /// Exact build-12340 attachment identifier.
        attachment_id: u32,
    },
    /// An active mount lacks its authored mount-main attachment.
    #[error("mount M2 {model} has no attachment point {attachment_id}")]
    MissingMountM2Attachment {
        /// Mount model lacking the selected semantic attachment.
        model: AssetPath,
        /// Exact build-12340 mount-main attachment identifier.
        attachment_id: u32,
    },
    /// A rider was evaluated before its active mount attachment pose.
    #[error("mounted player {guid:#018X} attachment point {attachment_id} has no current pose")]
    MissingMountM2AttachmentPose {
        /// Mounted player GUID.
        guid: u64,
        /// Exact build-12340 mount-main attachment identifier.
        attachment_id: u32,
    },
    /// Camera presentation exists without the corresponding resident M2 input.
    #[error("local player camera has no resident M2 frame input")]
    MissingPlayerM2FrameInput,
    /// A selected M2 animation no longer resolves through its validated aliases.
    #[error("M2 {model} selected absent animation sequence {sequence}")]
    M2SequenceIndex {
        /// Model whose immutable animation catalog became inconsistent.
        model: AssetPath,
        /// Selected sequence-table index.
        sequence: usize,
    },
    /// A previously admitted M2 animation can no longer select a variation.
    #[error("M2 {model} cannot select animation ID {animation_id}")]
    M2AnimationSelection {
        /// Model whose authored variation chain became unavailable.
        model: AssetPath,
        /// AnimationData identifier being advanced.
        animation_id: u16,
    },
    /// Placement-local ribbon state no longer parallels the shared model.
    #[error("M2 model {model} retains {trail_count} ribbon trails for {emitter_count} emitters")]
    M2RibbonTrailCount {
        /// Model whose immutable emitter table became inconsistent.
        model: AssetPath,
        /// Number of mutable trails owned by the placement.
        trail_count: usize,
        /// Number of shared decoded ribbon declarations.
        emitter_count: usize,
    },
    /// A decoded event no longer resolves through its validated model bone.
    #[error("M2 model {model} event {event_index} references missing bone {bone_index}")]
    M2EventBoneIndex {
        /// Model whose event-to-bone relationship became inconsistent.
        model: AssetPath,
        /// Zero-based event declaration index.
        event_index: usize,
        /// Missing model-bone index.
        bone_index: u32,
    },
    /// A validated ribbon unexpectedly references an absent bone transform.
    #[error("M2 model {model} ribbon {ribbon_index} references absent bone {bone_index}")]
    M2RibbonBoneIndex {
        /// Model containing the emitter.
        model: AssetPath,
        /// Zero-based ribbon declaration slot.
        ribbon_index: usize,
        /// Missing zero-based bone transform.
        bone_index: u32,
    },
    /// Placement-local particle state no longer parallels the shared model.
    #[error(
        "M2 model {model} retains {simulation_count} particle simulations for {emitter_count} emitters"
    )]
    M2ParticleSimulationCount {
        /// Model whose immutable emitter table became inconsistent.
        model: AssetPath,
        /// Number of mutable simulations owned by the placement.
        simulation_count: usize,
        /// Number of shared decoded particle declarations.
        emitter_count: usize,
    },
    /// Shared particle GPU resources no longer parallel the decoded table.
    #[error(
        "M2 model {model} retains {resource_count} particle resources for {emitter_count} emitters"
    )]
    M2ParticleResourceCount {
        /// Model whose resource generation became inconsistent.
        model: AssetPath,
        /// Number of renderer resource pairs.
        resource_count: usize,
        /// Number of shared decoded particle declarations.
        emitter_count: usize,
    },
    /// Ordinary particle rendering requires one authored texture declaration.
    #[error(
        "M2 model {model} particle {particle_index} exposes {texture_count} textures to the ordinary path"
    )]
    M2ParticleTextureCount {
        /// Model containing the emitter.
        model: AssetPath,
        /// Zero-based particle declaration slot.
        particle_index: usize,
        /// Number of populated packed texture slots.
        texture_count: usize,
    },
    /// The ordinary simulation does not substitute another generator shape.
    #[error(
        "M2 model {model} particle {particle_index} uses unsupported emitter type {emitter_type}"
    )]
    M2ParticleEmitterType {
        /// Model containing the emitter.
        model: AssetPath,
        /// Zero-based particle declaration slot.
        particle_index: usize,
        /// Authored generator selector.
        emitter_type: u8,
    },
}

/// One retained ADT's immutable upload resources and culling plan.
struct TerrainGpuTile {
    plan: Arc<TerrainTileMeshPlan>,
    draws: Vec<TerrainPreparedDraw>,
    liquids: Vec<LiquidGpuBatch>,
}

/// Resident world composition whose placement state survives tile changes.
pub(super) struct TerrainFrame {
    tile: Option<TerrainTileIndex>,
    map_id: Option<u32>,
    tiles: Vec<TerrainGpuTile>,
    visible_draws: Vec<TerrainPreparedDraw>,
    liquid_materials: LiquidGpuMaterialCache,
    liquid_filtering: WorldModelTextureFiltering,
    liquid_draws: Vec<solarity_rendering::LiquidPreparedDraw>,
    sky: sky::WorldSky,
    m2: M2Frame,
    world_models: WorldModelFrame,
}

impl TerrainFrame {
    pub(super) fn emit_unit_effect(
        &mut self,
        request: m2::unit_effects::UnitEffectRequest,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.m2
            .emit_unit_effect(request, self.m2_animation_time_ms(), random)
    }
    /// Captures the published player appearance without changing its live pose.
    pub(super) fn render_player_portrait(
        &self,
        renderer: &mut VulkanRenderer,
        player: &ResidentPlayerFrameInput<'_>,
        mask: solarity_rendering::BlpTextureHandle,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        self.m2.render_player_portrait(renderer, player, mask)
    }

    /// Uploads and validates every resource referenced by one admitted ADT.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare(
        renderer: &mut VulkanRenderer,
        map_id: u32,
        plan: &Arc<TerrainTileMeshPlan>,
        sources: &[Arc<BlpTextureSource>],
        liquid_batches: &[ResidentTerrainLiquidBatch],
        m2_scene: &ResidentM2Scene,
        world_models: &ResidentWorldModelScene,
        world_model_filtering: WorldModelTextureFiltering,
        world_model_base_mip: WorldModelBaseMip,
        random: &mut CrtRand,
        particle_twinkle: std::sync::Arc<solarity_rendering::M2ParticleTwinkleTable>,
        player: Option<ResidentPlayerFrameInput<'_>>,
        creatures: &[ResidentCreatureFrameInput<'_>],
        remote_players: &[ResidentPlayerFrameInput<'_>],
        game_objects: GameObjectFrameInput<'_>,
    ) -> Result<Self, RuntimeTerrainFrameError> {
        let draws = prepare_tile_draws(renderer, plan, sources)?;
        let mut liquid_materials = LiquidGpuMaterialCache::default();
        let liquids = liquid_materials.prepare_terrain(renderer, liquid_batches)?;

        let (m2, world_models) = prepare_scene_models(
            renderer,
            m2_scene,
            world_models,
            world_model_filtering,
            world_model_base_mip,
            random,
            particle_twinkle,
            player,
            creatures,
            remote_players,
            game_objects,
        )?;
        Ok(Self {
            tile: Some(plan.tile()),
            map_id: Some(map_id),
            tiles: vec![TerrainGpuTile {
                plan: Arc::clone(plan),
                draws,
                liquids,
            }],
            visible_draws: Vec::with_capacity(plan.chunks().len()),
            liquid_materials,
            liquid_filtering: world_model_filtering,
            sky: sky::WorldSky::new(),
            liquid_draws: Vec::new(),
            m2,
            world_models,
        })
    }

    /// Uploads one global-WMO scene without fabricating an ADT generation.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_global_world_model(
        renderer: &mut VulkanRenderer,
        m2_scene: &ResidentM2Scene,
        world_models: &ResidentWorldModelScene,
        world_model_filtering: WorldModelTextureFiltering,
        world_model_base_mip: WorldModelBaseMip,
        random: &mut CrtRand,
        particle_twinkle: std::sync::Arc<solarity_rendering::M2ParticleTwinkleTable>,
        player: Option<ResidentPlayerFrameInput<'_>>,
        creatures: &[ResidentCreatureFrameInput<'_>],
        remote_players: &[ResidentPlayerFrameInput<'_>],
        game_objects: GameObjectFrameInput<'_>,
    ) -> Result<Self, RuntimeTerrainFrameError> {
        let (m2, world_models) = prepare_scene_models(
            renderer,
            m2_scene,
            world_models,
            world_model_filtering,
            world_model_base_mip,
            random,
            particle_twinkle,
            player,
            creatures,
            remote_players,
            game_objects,
        )?;
        Ok(Self {
            tile: None,
            map_id: None,
            tiles: Vec::new(),
            visible_draws: Vec::new(),
            liquid_materials: LiquidGpuMaterialCache::default(),
            liquid_filtering: world_model_filtering,
            sky: sky::WorldSky::new(),
            liquid_draws: Vec::new(),
            m2,
            world_models,
        })
    }

    /// Culls, lights, records, and presents one resident terrain frame.
    ///
    /// The visible packet buffer is retained across frames. An empty result is
    /// still presented as a cleared world attachment; looking away from the
    /// resident ADT is valid camera state, not a rendering failure.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn present(
        &mut self,
        renderer: &mut VulkanRenderer,
        plan: Option<TerrainTileIndex>,
        environment: RuntimeWorldEnvironmentFrame,
        terrain: &mut super::terrain_coordinator::RuntimeTerrainCoordinator,
        camera: WorldCameraFrame,
        liquid_time_ms: u32,
        camera_submerged: bool,
        specular_enabled: bool,
        ripples: Option<solarity_rendering::WaterRippleFrame<'_>>,
        underwater_particles: Option<solarity_rendering::UnderwaterParticleFrame<'_>>,
        celestial_resources: super::sky_resources::RuntimeCelestialResources,
        sky_resources: &mut super::sky_resources::RuntimeSkyResources,
        unit_effect_sources: Option<Arc<m2::unit_effects::M2UnitEffectSources>>,
        unit_effect_callback: Option<&mut m2::unit_effects::UnitEffectEventCallback<'_>>,
        random: &mut CrtRand,
        player: ResidentPlayerFrameInput<'_>,
        creatures: &[ResidentCreatureFrameInput<'_>],
        remote_players: &[ResidentPlayerFrameInput<'_>],
        game_objects: GameObjectFrameInput<'_>,
        ui_extent: [f32; 2],
        ui_draws: &[UiPreparedDraw],
        ui_overlay_draws: &[UiPreparedDraw],
    ) -> Result<WorldFrameReport, RuntimeTerrainFrameError> {
        let mut profile =
            crate::application::frame_profile::RuntimeFrameProfile::new("World scene preparation");
        match (self.tile, plan) {
            (Some(frame), Some(plan)) if frame != plan => {
                return Err(RuntimeTerrainFrameError::TileMismatch {
                    frame_x: frame.x(),
                    frame_y: frame.y(),
                    plan_x: plan.x(),
                    plan_y: plan.y(),
                });
            }
            (Some(_), None) | (None, Some(_)) => {
                return Err(RuntimeTerrainFrameError::SceneKindMismatch);
            }
            (Some(_), Some(_)) | (None, None) => {}
        }
        let local_animation_time_ms = self.m2.animation_time_ms();
        self.m2
            .update_game_object_states(game_objects, local_animation_time_ms, random)?;
        self.world_models.update_game_object_states(game_objects)?;
        profile.mark("object states");
        let frustum = WorldFrustum::new(camera, WorldScreenWindow::FULL)?;
        self.visible_draws.clear();
        for tile in &self.tiles {
            for (chunk, draw) in tile.plan.chunks().iter().zip(&tile.draws) {
                if chunk.is_visible(frustum)? {
                    self.visible_draws.push(*draw);
                }
            }
        }
        profile.mark("terrain culling");
        let light = environment.light();
        self.sky.update(
            environment,
            camera,
            liquid_time_ms,
            celestial_resources.colors,
        );
        let fog = environment.fog();
        let (fog_start, fog_end) = fog.range();
        let fog_parameters = Vec4::new(fog_start, fog_end, 0.0, fog.exponent());
        let terrain_scene = TerrainSceneUniform::new(
            camera.projection(),
            camera.view(),
            light.ambient_color(),
            light.diffuse_color(),
            environment.light_direction(),
        )
        .with_fog(camera.view(), fog_parameters, fog.color());
        let world_model_scene = WorldModelSceneUniform::new(
            camera.projection(),
            camera.view(),
            camera.camera().position(),
            light.ambient_color(),
            light.diffuse_color(),
            environment.light_direction(),
            fog_parameters,
        );
        let m2_scene = M2SceneUniform::new(
            camera.projection(),
            camera.view(),
            camera.camera().position(),
            glam::Vec3::ZERO,
            glam::Vec3::ZERO,
            environment.light_direction(),
            fog_parameters,
            fog.color(),
            [M2LocalLightState::disabled(); 4],
        )
        .with_specular_enabled(specular_enabled);
        self.m2
            .update_player_state(player, local_animation_time_ms, random)?;
        self.m2
            .update_creature_states(creatures, local_animation_time_ms, random)?;
        self.m2
            .update_remote_player_states(remote_players, local_animation_time_ms, random)?;
        profile.mark("unit states");
        if let Some(sources) = unit_effect_sources {
            self.m2.set_unit_effect_sources(sources);
        }
        let m2 = self.m2.prepare_visible_draws_with_unit_effects(
            renderer,
            frustum,
            camera,
            if camera_submerged {
                solarity_rendering::M2TransparentPass::One
            } else {
                solarity_rendering::M2TransparentPass::Two
            },
            fog.color(),
            local_animation_time_ms,
            solarity_rendering::M2CameraEffectScale::EXTERNAL_CAMERA,
            random,
            Some(game_objects),
            unit_effect_callback,
            Some((
                m2_scene,
                solarity_rendering::M2DirectionalLight::new(
                    -environment.light_direction(),
                    light.ambient_color(),
                    light.diffuse_color(),
                ),
            )),
            Some((
                terrain,
                solarity_systems::WorldEntityLightEnvironment::new(
                    light.ambient_color(),
                    light.diffuse_color(),
                    -environment.light_direction(),
                    solarity_asset::exterior_light_ray_at(environment.day_fraction()),
                ),
            )),
        )?;
        profile.mark("M2 packets");
        let (liquid_lighting, liquid_fog) = liquid_environment(environment, camera);
        self.liquid_draws.clear();
        for batch in self.tiles.iter().flat_map(|tile| &tile.liquids) {
            if let Some(draw) = batch.prepare_draw(
                renderer,
                frustum,
                camera,
                liquid_lighting,
                liquid_fog,
                liquid_time_ms,
                specular_enabled,
                Some((m2.scene_points, m2.scene_directionals)),
            )? {
                self.liquid_draws.push(draw);
            }
        }
        self.world_models.prepare_liquid_draws(
            renderer,
            frustum,
            camera,
            liquid_lighting,
            liquid_fog,
            liquid_time_ms,
            specular_enabled,
            Some((m2.scene_points, m2.scene_directionals)),
            &mut self.liquid_draws,
        )?;
        profile.mark("liquid packets");
        let world_model_draws = self.world_models.prepare_visible_draws(
            renderer,
            frustum,
            environment.world_model_emissive(),
            fog.color(),
        )?;
        profile.mark("WMO packets");

        let (default_sky, sky_models) = sky_resources.prepare_models(
            renderer,
            camera,
            liquid_time_ms,
            environment,
            m2.bone_transforms.len(),
            random,
        )?;
        let depths = (!self.liquid_draws.is_empty()).then(|| liquid_depth_images(light));
        let mut scene = WorldFrameScene::new(terrain_scene, world_model_scene, m2_scene)
            .with_m2_instance_scenes(m2.instance_scenes)
            .with_sky_models(sky_models)
            .with_particle_capacity(m2.particle_vertex_capacity, m2.particle_index_capacity);
        if default_sky {
            scene = scene
                .with_celestials(
                    self.sky
                        .celestial_frame(camera, celestial_resources.textures),
                )
                .with_sky(self.sky.gradient_frame(camera))
                .with_clouds(self.sky.cloud_frame(camera));
        }
        if let Some([river, ocean, world_model]) = &depths {
            scene = scene.with_liquids(
                solarity_rendering::LiquidFrame::new(
                    &self.liquid_draws,
                    river,
                    ocean,
                    world_model,
                    m2.water_scene_order,
                )
                .with_texture_filtering(self.liquid_filtering),
            );
        }
        if let Some(ripples) = ripples {
            scene = scene.with_ripples(ripples.with_water_scene_order(m2.water_scene_order));
        }
        if let Some(underwater_particles) = underwater_particles {
            scene = scene.with_underwater_particles(underwater_particles);
        }
        let report = renderer.present_world_frame_with_ui_layers(
            scene,
            m2.bone_transforms,
            &self.visible_draws,
            world_model_draws,
            m2.draws,
            m2.particle_vertices,
            m2.particle_indices,
            m2.particle_draws,
            m2.ribbon_vertices,
            m2.ribbon_draws,
            WorldScreenWindow::FULL,
            ui_extent,
            ui_draws,
            ui_overlay_draws,
        )?;
        profile.mark("Vulkan presentation");
        Ok(report)
    }

    /// Rebuilds only the player-owned source after appearance customization.
    pub(super) fn replace_player(
        &mut self,
        renderer: &mut VulkanRenderer,
        player: Option<ResidentPlayerFrameInput<'_>>,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.m2.replace_player(renderer, player, random)
    }

    /// Rebuilds only visible creature sources after range/appearance changes.
    pub(super) fn replace_creatures(
        &mut self,
        renderer: &mut VulkanRenderer,
        creatures: &[ResidentCreatureFrameInput<'_>],
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.m2.replace_creatures(renderer, creatures, random)
    }

    /// Rebuilds visible remote characters after range or appearance changes.
    pub(super) fn replace_remote_players(
        &mut self,
        renderer: &mut VulkanRenderer,
        players: &[ResidentPlayerFrameInput<'_>],
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.m2.replace_remote_players(renderer, players, random)
    }

    /// Reconciles shared GameObject resources and independent object lifetimes.
    pub(super) fn synchronize_game_objects(
        &mut self,
        renderer: &mut VulkanRenderer,
        game_objects: GameObjectFrameInput<'_>,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.m2
            .synchronize_game_objects(renderer, game_objects, random)?;
        self.world_models
            .synchronize_game_objects(renderer, game_objects)
    }

    /// Transfers callbacks generated while advancing the current M2 frame.
    pub(super) fn drain_m2_events(&mut self) -> Vec<RuntimeM2Event> {
        self.m2.drain_triggered_events()
    }

    /// Transfers nonfatal M2 presentation diagnostics accumulated this frame.
    pub(super) fn drain_recoverable_errors(&mut self) -> Vec<String> {
        self.m2.drain_recoverable_errors()
    }

    /// Takes camera markers sampled from the latest controlled mount pose.
    pub(super) fn take_mount_camera_sample(&mut self) -> Option<RuntimeMountCameraSample> {
        self.m2.take_mount_camera_sample()
    }

    /// Returns the clock used to sample the latest resident M2 pose.
    pub(super) fn m2_animation_time_ms(&self) -> f32 {
        self.m2.animation_time_ms()
    }

    /// Returns the ADT whose renderer resources this generation represents.
    pub(super) const fn tile(&self) -> Option<TerrainTileIndex> {
        self.tile
    }

    /// Returns the complete row-major packet count retained for camera culling.
    pub(super) fn draw_count(&self) -> usize {
        self.tiles.iter().map(|tile| tile.draws.len()).sum()
    }

    /// Returns the number of independently transformed resident WMO owners.
    pub(super) fn world_model_placement_count(&self) -> usize {
        self.world_models.placement_count()
    }

    /// Returns the number of selected shared M2 GPU generations.
    pub(super) fn m2_mesh_count(&self) -> usize {
        self.m2.mesh_count()
    }

    /// Returns the number of retained MDDF and MODD instances.
    pub(super) fn m2_placement_count(&self) -> usize {
        self.m2.placement_count()
    }
}

/// Prepares scene-wide M2 and WMO state shared by tiled and global maps.
#[allow(clippy::too_many_arguments)]
fn prepare_scene_models(
    renderer: &mut VulkanRenderer,
    m2_scene: &ResidentM2Scene,
    world_models: &ResidentWorldModelScene,
    world_model_filtering: WorldModelTextureFiltering,
    world_model_base_mip: WorldModelBaseMip,
    random: &mut CrtRand,
    particle_twinkle: std::sync::Arc<solarity_rendering::M2ParticleTwinkleTable>,
    player: Option<ResidentPlayerFrameInput<'_>>,
    creatures: &[ResidentCreatureFrameInput<'_>],
    remote_players: &[ResidentPlayerFrameInput<'_>],
    game_objects: GameObjectFrameInput<'_>,
) -> Result<(M2Frame, WorldModelFrame), RuntimeTerrainFrameError> {
    let mut m2 = M2Frame::prepare(
        renderer,
        m2_scene,
        Arc::clone(game_objects.animations()),
        random,
        particle_twinkle,
    )?;
    m2.replace_player(renderer, player, random)?;
    m2.replace_creatures(renderer, creatures, random)?;
    m2.replace_remote_players(renderer, remote_players, random)?;
    m2.synchronize_game_objects(renderer, game_objects, random)?;
    let mut world_models = WorldModelFrame::prepare(
        renderer,
        world_models,
        world_model_filtering,
        world_model_base_mip,
    )?;
    world_models.synchronize_game_objects(renderer, game_objects)?;
    Ok((m2, world_models))
}

/// Proves that mesh and source tables describe one exact MTEX generation.
fn validate_texture_table(
    plan: &TerrainTileMeshPlan,
    sources: &[Arc<BlpTextureSource>],
) -> Result<(), RuntimeTerrainFrameError> {
    if sources.len() != plan.textures().len() {
        return Err(RuntimeTerrainFrameError::TextureTableCount {
            source_count: sources.len(),
            plan_count: plan.textures().len(),
        });
    }
    for (index, (expected, source)) in plan.textures().iter().zip(sources).enumerate() {
        if expected != source.path() {
            return Err(RuntimeTerrainFrameError::TextureTablePath {
                index,
                expected: expected.clone(),
                actual: source.path().clone(),
            });
        }
    }
    Ok(())
}

/// Uploads one admitted ADT without replacing any world placement state.
fn prepare_tile_draws(
    renderer: &mut VulkanRenderer,
    plan: &TerrainTileMeshPlan,
    sources: &[Arc<BlpTextureSource>],
) -> Result<Vec<TerrainPreparedDraw>, RuntimeTerrainFrameError> {
    validate_texture_table(plan, sources)?;

    // The shared tile allocations are immutable. Renderer registries use
    // the plan identity to suppress duplicate staging work within a
    // generation, while MTEX images deduplicate by selected asset identity.
    let mesh = renderer.upload_terrain_mesh(plan)?;
    let material = renderer.upload_terrain_material(plan)?;
    let texture_uploads = sources
        .iter()
        .map(|source| {
            solarity_rendering::BlpTextureUploadRequest::new(source, BlpColorSpace::Linear)
        })
        .collect::<Vec<_>>();
    let textures = renderer.upload_blp_textures(&texture_uploads)?;

    let mut requests = Vec::with_capacity(plan.chunks().len());
    let mut layer_counts = Vec::with_capacity(plan.chunks().len());
    for (chunk_index, chunk) in plan.chunks().iter().enumerate() {
        let layer_count = TerrainLayerCount::try_from(chunk.layers().len())?;
        let layers = chunk
            .layers()
            .iter()
            .map(|layer| {
                let texture_index = usize::try_from(layer.texture_index()).map_err(|_source| {
                    RuntimeTerrainFrameError::TextureIndex {
                        chunk_index,
                        texture_index: layer.texture_index(),
                        texture_count: textures.len(),
                    }
                })?;
                textures
                    .get(texture_index)
                    .copied()
                    .ok_or(RuntimeTerrainFrameError::TextureIndex {
                        chunk_index,
                        texture_index: layer.texture_index(),
                        texture_count: textures.len(),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        requests.push(TerrainTextureSet::new(material, &layers)?);
        layer_counts.push(layer_count);
    }

    // Descriptor allocation is exact-demand and internally shares
    // identical atlas/layer tuples. Pipeline preparation similarly reduces
    // the 256 chunks to the one-through-four variants actually authored.
    let texture_sets = renderer.prepare_terrain_texture_sets(&requests)?;
    let mut pipelines = [None; 4];
    for layer_count in &layer_counts {
        let slot = usize::from(layer_count.get() - 1);
        if pipelines[slot].is_none() {
            pipelines[slot] = Some(renderer.prepare_terrain_pipeline(*layer_count)?);
        }
    }

    let mut draws = Vec::with_capacity(plan.chunks().len());
    for chunk_index in 0..plan.chunks().len() {
        let slot = usize::from(layer_counts[chunk_index].get() - 1);
        let Some(pipeline) = pipelines[slot] else {
            // The immediately preceding loop prepared this exact closed
            // layer-count slot, so absence would indicate local corruption.
            return Err(RuntimeTerrainFrameError::Vulkan(
                VulkanError::TerrainDrawPipelineMismatch,
            ));
        };
        draws.push(renderer.prepare_terrain_draw(
            mesh,
            pipeline,
            texture_sets[chunk_index],
            &requests[chunk_index],
            plan,
            chunk_index,
        )?);
    }

    Ok(draws)
}
