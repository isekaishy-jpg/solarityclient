//! Retained stock M2 scene inserted beneath pre-world Glue presentation.

mod backdrop_loader;
mod script_models;

use backdrop_loader::GlueBackdropLoader;
use script_models::GlueScriptModelInstance;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use glam::{Vec3, Vec4};
use solarity_asset::{
    AnimationDataCatalog, ArchiveCatalog, AssetError, AssetPath, AssetStore, BlpTextureCache,
    M2HardcodedTextureSource, M2LightKind, M2ModelCache, M2TextureKind, WorldLightSampleError,
};
use solarity_cpu::{CpuError, CpuExecutor, CpuTask};
use solarity_rendering::{
    M2CameraFrameError, M2DirectionalLight, M2LightOverride, M2LocalLightCount, M2LocalLightState,
    M2ModelOrientation, M2ParticleTwinkleTable, M2SceneUniform, M2Sunlight, M2UiCameraViewport,
    TerrainSceneUniform, VulkanError, VulkanRenderer, WorldCameraFrame, WorldFrameGlow,
    WorldFrameScene, WorldFrustum, WorldModelSceneUniform, WorldScreenWindow,
    glue_character_sunlight, merge_wotlk_directional_lights, sample_m2_ui_camera_frame,
};
use solarity_ui::{GlueManager, UiModelLight, UiModelLightSets, UiModelPresentation, UiScreenRect};
use thiserror::Error;

use crate::application::login_ui::RuntimeUiFrame;
use crate::application::player_coordinator::ResidentGlueCharacterFrameInput;
use crate::application::terrain_frame::RuntimeTerrainFrameError;
use crate::application::terrain_frame::m2::{
    GlueM2Texture, M2CpuSource, M2Frame, M2GlueCpuSourceKey, M2GluePipelineWarmup, RuntimeM2Event,
    prepare_m2_cpu_source,
};
use crate::random::CrtRand;

const STOCK_GLUE_AMBIENT: Vec3 = Vec3::splat(0.35);
const STOCK_GLUE_DIFFUSE: Vec3 = Vec3::splat(0.65);
const STOCK_GLUE_LIGHT_DIRECTION: Vec3 = Vec3::new(-0.35, 0.45, 0.82);
const STOCK_LOGIN_FOG_COLOR: Vec3 = Vec3::new(0.25, 0.06, 0.015);
const STOCK_LOGIN_FOG_RANGE: Vec4 = Vec4::new(0.0, 1200.0, 0.0, 1.0);

/// A visible Glue model cannot enter the retained M2 compositor callback.
#[derive(Debug, Error)]
pub enum RuntimeGlueModelError {
    /// A required Glue model-light palette could not be sampled.
    #[error(transparent)]
    Light(#[from] WorldLightSampleError),
    /// The exact backdrop request failed on its worker and is not retried.
    #[error("Glue backdrop {path} failed to load: {source}")]
    BackdropLoad {
        path: AssetPath,
        source: Arc<RuntimeGlueModelError>,
    },
    /// An earlier worker panic poisoned the private archive owner.
    #[error("Glue backdrop archive worker state is unavailable")]
    BackdropWorkerUnavailable,
    /// Archive-backed M2, SKIN, or BLP data failed validation.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// Immutable model resources or live effects failed preparation.
    #[error(transparent)]
    Frame(#[from] RuntimeTerrainFrameError),
    /// The unified model/UI submission failed renderer validation.
    #[error(transparent)]
    Vulkan(#[from] VulkanError),
    /// The bounded CPU preparation task could not be submitted or joined.
    #[error(transparent)]
    Cpu(#[from] CpuError),
    /// The selected authored camera could not form a projection.
    #[error(transparent)]
    Camera(#[from] M2CameraFrameError),
    /// The current compositor supports one stock environment callback.
    #[error("Glue presentation exposes {count} simultaneous visible models")]
    VisibleModelCount { count: usize },
    /// An environment M2 used a replacement category without a stock owner.
    #[error("Glue model {model} texture {texture_index} requires replacement {kind:?}")]
    ReplaceableTexture {
        model: AssetPath,
        texture_index: usize,
        kind: M2TextureKind,
    },
    /// A hardcoded M2 texture declaration omitted its source path.
    #[error("Glue model {model} texture {texture_index} has no hardcoded path")]
    MissingTexturePath {
        model: AssetPath,
        texture_index: usize,
    },
    /// The live Model frame does not define a finite viewport inside its UI canvas.
    #[error("Glue model {object_index} has invalid viewport bounds")]
    InvalidViewport {
        /// Live UI arena identity of the model widget.
        object_index: usize,
    },
    /// The live stock display-gamma CVar could not form a finite renderer input.
    #[error("Glue gamma CVar has invalid value {value:?}")]
    InvalidGamma { value: Option<String> },
    /// A completed worker generation disappeared before publication.
    #[error("Glue model CPU preparation lost its pending generation")]
    PendingState,
}

#[derive(Clone, Debug, PartialEq)]
struct GlueModelKey {
    object_index: usize,
    path: AssetPath,
    instance_generation: u32,
}

impl GlueModelKey {
    fn from_presentation(model: &UiModelPresentation) -> Self {
        Self {
            object_index: model.object_index(),
            path: model.path().clone(),
            instance_generation: model.instance_generation(),
        }
    }
}

struct ActiveGlueModel {
    key: GlueModelKey,
    environment: GlueModelEnvironment,
    local_light_count: M2LocalLightCount,
    model: Arc<solarity_asset::DecodedM2Model>,
    frame: M2Frame,
}

/// Glue route that owns the optional character embedded in the backdrop.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GlueCharacterScreen {
    Selection,
    Creation,
}

/// Publication state for one complete Glue scene generation.
///
/// A pending result leaves the previously active backdrop and character
/// untouched. The composition root can therefore retain its matching UI frame
/// until every replacement resource is ready to cross the swapchain boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RuntimeGlueModelPoll {
    Pending,
    Ready,
}

/// Exact immutable backdrop variant selected by its external-light contract.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct GlueModelGenerationKey {
    path: AssetPath,
    external_directional_light: bool,
}

/// One scene generation whose CPU plan and shaders are still being prepared.
struct PendingGlueModel {
    generation: GlueModelGenerationKey,
    local_light_count: M2LocalLightCount,
    model: Arc<solarity_asset::DecodedM2Model>,
    textures: Vec<GlueM2Texture>,
    submitted_at: std::time::Instant,
    task: CpuTask<Result<PreparedGlueCpuSource, RuntimeTerrainFrameError>>,
}

/// Archive-decoded backdrop state waiting for its immutable render plan.
struct LoadedGlueBackdrop {
    generation: GlueModelGenerationKey,
    model: Arc<solarity_asset::DecodedM2Model>,
    textures: Vec<GlueM2Texture>,
}

/// One completed worker result and its execution time, excluding queue delay.
struct PreparedGlueCpuSource {
    source: M2CpuSource,
    elapsed: std::time::Duration,
}

/// One character, equipment, effect, or pet source prepared independently.
struct PendingGlueCharacterSource {
    key: M2GlueCpuSourceKey,
    submitted_at: std::time::Instant,
    task: CpuTask<Result<PreparedGlueCpuSource, RuntimeTerrainFrameError>>,
}

/// One exact model/orientation whose driver pipelines are admitted gradually.
struct PendingGlueCharacterPipelineWarmup {
    key: (M2GlueCpuSourceKey, M2ModelOrientation),
    warmup: M2GluePipelineWarmup,
}

/// Collects each distinct immutable model/light permutation in one preview.
fn glue_character_cpu_models(
    input: &ResidentGlueCharacterFrameInput<'_>,
    character_light_count: M2LocalLightCount,
    pet_light_count: M2LocalLightCount,
) -> Vec<(
    M2GlueCpuSourceKey,
    Arc<solarity_asset::DecodedM2Model>,
    M2ModelOrientation,
)> {
    let mut seen = HashSet::new();
    let mut models = Vec::new();
    let mut push = |model: &Arc<solarity_asset::DecodedM2Model>, light_count, orientation| {
        let key = M2GlueCpuSourceKey::new(model.path().clone(), light_count);
        if seen.insert((key.clone(), orientation)) {
            models.push((key, Arc::clone(model), orientation));
        }
    };
    push(
        input.model(),
        character_light_count,
        M2ModelOrientation::Authored,
    );
    for attachment in input.attachments() {
        let orientation = if attachment.is_model_mirrored() {
            M2ModelOrientation::Mirrored
        } else {
            M2ModelOrientation::Authored
        };
        push(attachment.model(), character_light_count, orientation);
        for effect in attachment.visual_effects() {
            push(effect.model(), character_light_count, orientation);
        }
    }
    if let Some(pet) = input.pet() {
        push(pet.model(), pet_light_count, M2ModelOrientation::Authored);
    }
    models
}

/// Measures the worker-owned immutable preparation stage without main-thread wait time.
fn prepare_glue_cpu_task(
    model: &Arc<solarity_asset::DecodedM2Model>,
    local_light_count: M2LocalLightCount,
) -> Result<PreparedGlueCpuSource, RuntimeTerrainFrameError> {
    let started = std::time::Instant::now();
    let source = prepare_m2_cpu_source(model, local_light_count)?;
    Ok(PreparedGlueCpuSource {
        source,
        elapsed: started.elapsed(),
    })
}

/// Loads one environment M2 and resolves its complete hardcoded texture set.
fn load_glue_model_generation(
    models: &mut M2ModelCache,
    textures: &mut BlpTextureCache,
    store: &mut AssetStore,
    path: &AssetPath,
) -> Result<(Arc<solarity_asset::DecodedM2Model>, Vec<GlueM2Texture>), RuntimeGlueModelError> {
    let asset_started = std::time::Instant::now();
    let model = models.load(store, path)?;
    let mut texture_sources = Vec::with_capacity(model.textures().len());
    for (texture_index, texture) in model.textures().iter().enumerate() {
        if texture.kind() != M2TextureKind::Hardcoded {
            return Err(RuntimeGlueModelError::ReplaceableTexture {
                model: model.path().clone(),
                texture_index,
                kind: texture.kind(),
            });
        }
        match texture.hardcoded_source().ok_or_else(|| {
            RuntimeGlueModelError::MissingTexturePath {
                model: model.path().clone(),
                texture_index,
            }
        })? {
            M2HardcodedTextureSource::Archive(path) => match textures.load(store, path) {
                Ok(texture) => texture_sources.push(GlueM2Texture::Authored(texture)),
                Err(source) => {
                    tracing::warn!(
                        model = %model.path(),
                        texture = %path,
                        error = %source,
                        "Glue M2 texture request failed; using stock green texture"
                    );
                    texture_sources.push(GlueM2Texture::StockFailure);
                }
            },
            M2HardcodedTextureSource::StockWhite => {
                texture_sources.push(GlueM2Texture::StockWhite);
            }
            M2HardcodedTextureSource::StockFailure => {
                tracing::warn!(
                    model = %model.path(),
                    "Glue M2 contains a non-archive texture name; using stock green texture"
                );
                texture_sources.push(GlueM2Texture::StockFailure);
            }
        }
    }
    let authored_directional_lights = model
        .animations()
        .lights()
        .iter()
        .filter(|light| light.kind() == M2LightKind::Directional)
        .count();
    let authored_point_lights = model
        .animations()
        .lights()
        .iter()
        .filter(|light| light.kind() == M2LightKind::Point)
        .count();
    tracing::info!(
        model = %model.path(),
        texture_count = texture_sources.len(),
        authored_directional_lights,
        authored_point_lights,
        asset_ms = asset_started.elapsed().as_secs_f64() * 1_000.0,
        "loaded Glue model archive generation"
    );
    Ok((model, texture_sources))
}

/// Immutable Vulkan resources retained across Glue backdrop switches.
struct PreparedGlueModel {
    local_light_count: M2LocalLightCount,
    model: Arc<solarity_asset::DecodedM2Model>,
    source: crate::application::terrain_frame::m2::M2GlueGpuSource,
}

/// Selected half of each ModelFFX live/ghost light pair.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GlueModelLightVariant {
    Live,
    Ghost,
}

impl GlueModelLightVariant {
    /// Selects one authored four-light bank without merging the two states.
    const fn select(self, lights: UiModelLightSets) -> [Option<UiModelLight>; 4] {
        match self {
            Self::Live => lights.live(),
            Self::Ghost => lights.ghost(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct GlueModelEnvironment {
    /// Camera selection changes the view without replacing M2 playback.
    camera: i32,
    model_scale: f32,
    rotation_radians: f32,
    ambient: Vec3,
    diffuse: Vec3,
    light_direction: Vec3,
    fog_color: Vec3,
    fog_range: Vec4,
    background_light_override: Option<M2LightOverride>,
    character_light_override: Option<M2LightOverride>,
    pet_light_override: Option<M2LightOverride>,
    shared_point_light_count: usize,
    bounds: UiScreenRect,
    camera_viewport: M2UiCameraViewport,
    alpha: f32,
    glow: f32,
}

impl GlueModelEnvironment {
    fn from_presentation(
        model: &UiModelPresentation,
        light_variant: GlueModelLightVariant,
        ghost_sunlight: Result<M2Sunlight, WorldLightSampleError>,
    ) -> Result<Self, RuntimeGlueModelError> {
        let (fog_color, fog_range) =
            model
                .fog()
                .map_or((STOCK_LOGIN_FOG_COLOR, STOCK_LOGIN_FOG_RANGE), |fog| {
                    let [near, far] = fog.range();
                    (
                        Vec3::from_array(fog.color()),
                        Vec4::new(near, far, 0.0, 1.0),
                    )
                });
        // GlueParent.lua documents these as six paired background, character,
        // and pet banks. The selected character's enum ghost flag chooses the
        // same half of every pair before SetupSunlight merges directionals.
        let light_override = |lights| -> Result<_, RuntimeGlueModelError> {
            if let Some(sunlight) =
                model_sunlight(model_directional_lights(light_variant.select(lights)))
            {
                return Ok(Some(M2LightOverride::Directional(sunlight)));
            }
            // 0x004E3A20 requires the default palette and resets the entire
            // accumulator only for a ghost bank without authored lights.
            Ok(match light_variant {
                GlueModelLightVariant::Live => None,
                GlueModelLightVariant::Ghost => Some(M2LightOverride::All(ghost_sunlight?)),
            })
        };
        Ok(Self {
            camera: model.camera(),
            model_scale: model.model_scale(),
            rotation_radians: model.rotation_radians(),
            ambient: STOCK_GLUE_AMBIENT,
            diffuse: STOCK_GLUE_DIFFUSE,
            light_direction: STOCK_GLUE_LIGHT_DIRECTION,
            fog_color,
            fog_range,
            background_light_override: light_override(model.background_lights())?,
            character_light_override: light_override(model.character_lights())?,
            pet_light_override: light_override(model.pet_lights())?,
            shared_point_light_count: 0,
            bounds: model.bounds(),
            camera_viewport: M2UiCameraViewport::new(
                [
                    model.bounds().width() as f32,
                    model.bounds().height() as f32,
                ],
                model.ui_extent(),
                model.effective_scale(),
            ),
            alpha: model.alpha(),
            glow: model.glow(),
        })
    }

    /// Returns the shader light count after stock directional-light merging.
    fn character_light_count(self) -> M2LocalLightCount {
        self.character_light_override.map_or_else(
            || local_light_count_from_len((1 + self.shared_point_light_count).min(4)),
            |light| light.local_light_count(self.shared_point_light_count),
        )
    }

    /// Applies stock's pet fallback to the effective character light bank.
    fn pet_light_count(self) -> M2LocalLightCount {
        self.pet_light_override.map_or_else(
            || self.character_light_count(),
            |light| light.local_light_count(self.shared_point_light_count),
        )
    }
}

/// Merges one ModelFFX bank through build 12340's SetupSunlight boundary.
fn model_directional_lights(lights: [Option<UiModelLight>; 4]) -> [Option<M2DirectionalLight>; 4] {
    lights.map(|light| {
        light.map(|light| {
            M2DirectionalLight::new(
                Vec3::from_array(light.direction()),
                Vec3::from_array(light.ambient()),
                Vec3::from_array(light.diffuse()),
            )
        })
    })
}

fn model_sunlight(lights: [Option<M2DirectionalLight>; 4]) -> Option<M2Sunlight> {
    let mut source = lights.into_iter().flatten();
    let first = source.next()?;
    let mut contiguous = [first; 4];
    let mut count = 1_usize;
    for light in source {
        contiguous[count] = light;
        count += 1;
    }
    merge_wotlk_directional_lights(&contiguous[..count])
}

/// Process-long model and texture caches plus the current Glue M2 generation.
pub(crate) struct RuntimeGlueModelScene {
    backdrop_assets: GlueBackdropLoader,
    animations: Arc<AnimationDataCatalog>,
    model_instances: Vec<GlueScriptModelInstance>,
    // Keep failures deferred until the ghost branch actually requests this
    // palette, matching 0x004E3A20 without archive work on selection input.
    ghost_sunlight: Result<M2Sunlight, WorldLightSampleError>,
    active: Option<ActiveGlueModel>,
    pending: Vec<PendingGlueModel>,
    backdrop_paths: VecDeque<AssetPath>,
    loaded_backdrops: VecDeque<LoadedGlueBackdrop>,
    prepared: HashMap<GlueModelGenerationKey, PreparedGlueModel>,
    character_sources: HashMap<M2GlueCpuSourceKey, Arc<M2CpuSource>>,
    pending_character_sources: Vec<PendingGlueCharacterSource>,
    pending_character_pipeline_warmups: VecDeque<PendingGlueCharacterPipelineWarmup>,
    warmed_character_pipelines: HashSet<(M2GlueCpuSourceKey, M2ModelOrientation)>,
    character_replacement_required: bool,
    character_screen: Option<GlueCharacterScreen>,
    sound_camera: Option<WorldCameraFrame>,
    frame_profiler: Option<GlueFrameProfiler>,
}

struct GlueFrameProfiler {
    window_started: std::time::Instant,
    frame_count: u64,
    prepare_total: std::time::Duration,
    prepare_max: std::time::Duration,
    present_total: std::time::Duration,
    present_max: std::time::Duration,
}

impl GlueFrameProfiler {
    fn from_environment() -> Option<Self> {
        std::env::var_os("SOLARITY_FRAME_TIMINGS").map(|_value| Self {
            window_started: std::time::Instant::now(),
            frame_count: 0,
            prepare_total: std::time::Duration::ZERO,
            prepare_max: std::time::Duration::ZERO,
            present_total: std::time::Duration::ZERO,
            present_max: std::time::Duration::ZERO,
        })
    }

    fn record(
        &mut self,
        prepare: std::time::Duration,
        present: std::time::Duration,
        draw_count: usize,
        bone_count: usize,
        particle_vertex_count: usize,
    ) {
        self.frame_count = self.frame_count.saturating_add(1);
        self.prepare_total += prepare;
        self.prepare_max = self.prepare_max.max(prepare);
        self.present_total += present;
        self.present_max = self.present_max.max(present);
        let elapsed = self.window_started.elapsed();
        if elapsed < std::time::Duration::from_secs(2) {
            return;
        }
        let divisor = self.frame_count.max(1) as f64;
        tracing::info!(
            measured_fps = self.frame_count as f64 / elapsed.as_secs_f64(),
            frame_count = self.frame_count,
            prepare_mean_us = self.prepare_total.as_secs_f64() * 1_000_000.0 / divisor,
            prepare_max_us = self.prepare_max.as_secs_f64() * 1_000_000.0,
            present_mean_us = self.present_total.as_secs_f64() * 1_000_000.0 / divisor,
            present_max_us = self.present_max.as_secs_f64() * 1_000_000.0,
            draw_count,
            bone_count,
            particle_vertex_count,
            "profiled retained Glue model frame"
        );
        self.window_started = std::time::Instant::now();
        self.frame_count = 0;
        self.prepare_total = std::time::Duration::ZERO;
        self.prepare_max = std::time::Duration::ZERO;
        self.present_total = std::time::Duration::ZERO;
        self.present_max = std::time::Duration::ZERO;
    }
}

impl RuntimeGlueModelScene {
    #[must_use]
    pub(crate) fn new(
        catalog: ArchiveCatalog,
        ghost_sunlight: Result<M2Sunlight, WorldLightSampleError>,
        animations: Arc<AnimationDataCatalog>,
    ) -> Self {
        Self {
            backdrop_assets: GlueBackdropLoader::new(catalog),
            animations,
            model_instances: Vec::new(),
            ghost_sunlight,
            active: None,
            pending: Vec::new(),
            backdrop_paths: VecDeque::new(),
            loaded_backdrops: VecDeque::new(),
            prepared: HashMap::new(),
            character_sources: HashMap::new(),
            pending_character_sources: Vec::new(),
            pending_character_pipeline_warmups: VecDeque::new(),
            warmed_character_pipelines: HashSet::new(),
            character_replacement_required: false,
            character_screen: None,
            sound_camera: None,
            frame_profiler: GlueFrameProfiler::from_environment(),
        }
    }

    /// Queues worker-owned archive reads for the finite racial backdrops.
    ///
    /// A second archive mount gives the worker exclusive mutable handles;
    /// neither MPQ reads nor M2/BLP parsing can then block the presentation
    /// thread. Stock CharacterSelect and CharacterCreate both install the
    /// ModelFFX background-light bank, so only that exact shader contract is
    /// prepared.
    pub(crate) fn prewarm_backdrops(
        &mut self,
        paths: Vec<AssetPath>,
        cpu: &CpuExecutor,
    ) -> Result<(), RuntimeGlueModelError> {
        self.backdrop_paths.extend(paths);
        self.service_backdrop_reads(cpu)
    }

    /// Begins immutable environment-model preparation before its Glue screen
    /// becomes visible. The movie screen intentionally has no visible model,
    /// but stock has already assigned AccountLogin's source in `OnLoad`.
    pub(crate) fn prewarm(
        &mut self,
        path: AssetPath,
        background_light_count: usize,
        cpu: &CpuExecutor,
    ) -> Result<(), RuntimeGlueModelError> {
        let external_directional_light = background_light_count != 0;
        let generation = GlueModelGenerationKey {
            path: path.clone(),
            external_directional_light,
        };
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.key.path == path)
            || self.prepared.contains_key(&generation)
            || self
                .pending
                .iter()
                .any(|pending| pending.generation == generation)
        {
            return Ok(());
        }
        let loaded = self
            .backdrop_assets
            .load_blocking(&path, cpu)
            .map_err(|source| RuntimeGlueModelError::BackdropLoad { path, source })?;
        let model = Arc::clone(&loaded.model);
        let textures = loaded.textures.clone();
        let local_light_count = maximum_glue_light_count(&model, external_directional_light);
        let task_model = Arc::clone(&model);
        let task = cpu.try_submit(move || prepare_glue_cpu_task(&task_model, local_light_count))?;
        self.pending.push(PendingGlueModel {
            generation,
            local_light_count,
            model,
            textures,
            submitted_at: std::time::Instant::now(),
            task,
        });
        Ok(())
    }

    /// Completes immutable GPU preparation before the cinematic presents.
    ///
    /// Vulkan resource ownership stays on the presentation thread, while the
    /// mesh plan and shader compilation are joined from the bounded CPU pool.
    /// AccountLogin_OnShow starts playback when login actually becomes visible.
    pub(crate) fn finish_prewarm(
        &mut self,
        renderer: &mut VulkanRenderer,
        presentation: &UiModelPresentation,
    ) -> Result<(), RuntimeGlueModelError> {
        let key = GlueModelKey::from_presentation(presentation);
        let environment = GlueModelEnvironment::from_presentation(
            presentation,
            GlueModelLightVariant::Live,
            self.ghost_sunlight,
        )?;
        let generation = GlueModelGenerationKey {
            path: key.path.clone(),
            external_directional_light: environment.background_light_override.is_some(),
        };
        if !self.prepared.contains_key(&generation) {
            let pending_index = self
                .pending
                .iter()
                .position(|pending| pending.generation == generation)
                .ok_or(RuntimeGlueModelError::PendingState)?;
            self.complete_pending(
                renderer,
                pending_index,
                "prepared hidden Glue model generation",
            )?;
        }
        if let Err(source) = renderer.save_pipeline_cache() {
            tracing::warn!(
                error = %source,
                "could not checkpoint startup Vulkan pipeline cache"
            );
        }
        Ok(())
    }

    /// Advances one optional archive path after the selected scene is ready.
    fn service_backdrop_reads(&mut self, cpu: &CpuExecutor) -> Result<(), RuntimeGlueModelError> {
        let Some(path) = self.backdrop_paths.front().cloned() else {
            return Ok(());
        };
        if !cpu.can_admit_speculative()? {
            return Ok(());
        }
        let result = match self.backdrop_assets.poll(&path, cpu) {
            Ok(None) => return Ok(()),
            Ok(Some(loaded)) => Ok(loaded),
            Err(source) => Err(source),
        };
        self.backdrop_paths.pop_front();
        match result {
            Ok(loaded) => {
                let generation = GlueModelGenerationKey {
                    path,
                    external_directional_light: true,
                };
                if !self.prepared.contains_key(&generation)
                    && !self
                        .pending
                        .iter()
                        .any(|pending| pending.generation == generation)
                    && !self
                        .loaded_backdrops
                        .iter()
                        .any(|loaded| loaded.generation == generation)
                {
                    self.loaded_backdrops.push_back(LoadedGlueBackdrop {
                        generation,
                        model: Arc::clone(&loaded.model),
                        textures: loaded.textures.clone(),
                    });
                }
            }
            Err(source) => {
                if !matches!(source.as_ref(), RuntimeGlueModelError::Asset(AssetError::AssetNotFound { path: missing }) if *missing == path)
                {
                    tracing::warn!(model = %path, error = %source,
                        "optional Glue backdrop archive prewarm failed");
                }
            }
        }
        Ok(())
    }

    /// Advances backdrop prewarm without monopolizing a presentation frame.
    ///
    /// Archive decoding stays on the CPU pool. As results arrive,
    /// at most one render plan is admitted and at most one completed generation
    /// is published per frame. This bounds both CPU-pool contention and Vulkan
    /// pipeline creation while still making the subsequent Glue screen change
    /// a cache lookup.
    pub(crate) fn service_backdrop_prewarms(
        &mut self,
        renderer: &mut VulkanRenderer,
        cpu: &CpuExecutor,
    ) -> Result<bool, RuntimeGlueModelError> {
        self.service_backdrop_reads(cpu)?;
        // Demand may already have consumed an archive result queued for prewarm.
        self.loaded_backdrops.retain(|loaded| {
            !self.prepared.contains_key(&loaded.generation)
                && !self
                    .pending
                    .iter()
                    .any(|pending| pending.generation == loaded.generation)
        });
        // Racial backdrops are speculative. Do not let their FIFO fill every
        // worker lane ahead of the character representation selected by the
        // user; the configured test client deliberately has far more queue
        // capacity than workers.
        if !self.loaded_backdrops.is_empty()
            && cpu.can_admit_speculative()?
            && let Some(loaded) = self.loaded_backdrops.pop_front()
        {
            let local_light_count = maximum_glue_light_count(&loaded.model, true);
            let task_model = Arc::clone(&loaded.model);
            match cpu.try_submit(move || prepare_glue_cpu_task(&task_model, local_light_count)) {
                Ok(task) => self.pending.push(PendingGlueModel {
                    generation: loaded.generation,
                    local_light_count,
                    model: loaded.model,
                    textures: loaded.textures,
                    submitted_at: std::time::Instant::now(),
                    task,
                }),
                Err(CpuError::AtCapacity { .. }) => self.loaded_backdrops.push_front(loaded),
                Err(source) => return Err(source.into()),
            }
        }
        let completed = if let Some(index) = self
            .pending
            .iter()
            .position(|pending| pending.task.is_finished())
        {
            self.complete_pending(renderer, index, "prepared background Glue model generation")?;
            true
        } else {
            false
        };
        let complete = self.backdrop_paths.is_empty()
            && !self.backdrop_assets.has_pending()
            && self.loaded_backdrops.is_empty()
            && self.pending.is_empty();
        if complete
            && completed
            && let Err(source) = renderer.save_pipeline_cache()
        {
            tracing::warn!(
                error = %source,
                "could not checkpoint Glue backdrop Vulkan pipeline cache"
            );
        }
        Ok(complete)
    }

    /// Retires visible effects when Glue hides its Model, retaining widget playback.
    /// AccountLogin_OnHide stops its SFX; no hidden frame may emit new callbacks.
    pub(crate) fn hide(&mut self) {
        self.retire_active_script_model();
        self.sound_camera = None;
    }

    /// Rebuilds GPU state only when Glue changes the selected model generation.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn synchronize(
        &mut self,
        renderer: &mut VulkanRenderer,
        glue: &GlueManager,
        cpu: &CpuExecutor,
        random: &mut CrtRand,
        particle_twinkle: Arc<M2ParticleTwinkleTable>,
        glue_character: Option<ResidentGlueCharacterFrameInput<'_>>,
        glue_character_changed: bool,
        glue_character_expected: bool,
    ) -> Result<RuntimeGlueModelPoll, RuntimeGlueModelError> {
        self.synchronize_script_models(glue, cpu, random)?;
        let mut visible = glue.presentation().visible_models();
        let character_screen = match glue.current_screen().as_str() {
            "charselect" => Some(GlueCharacterScreen::Selection),
            "charcreate" => Some(GlueCharacterScreen::Creation),
            _ => None,
        };
        let character_screen_changed = self.character_screen != character_screen;
        if glue_character_changed || character_screen_changed {
            self.character_replacement_required = true;
        }
        let Some(presentation) = visible.next() else {
            // CharacterSelect briefly owns an empty directory before its
            // asynchronous enumeration arrives. The old scene remains a
            // complete generation until the new Model widget becomes visible.
            let explicitly_cleared = glue.presentation().has_visible_cleared_model()
                || self.active.as_ref().is_some_and(|active| {
                    glue.presentation()
                        .model_was_cleared(active.key.object_index)
                });
            if self.active.is_some()
                && !explicitly_cleared
                && matches!(glue.current_screen().as_str(), "charselect" | "charcreate")
            {
                return Ok(RuntimeGlueModelPoll::Pending);
            }
            self.retire_active_script_model();
            self.character_screen = character_screen;
            self.sound_camera = None;
            return Ok(RuntimeGlueModelPoll::Ready);
        };
        if visible.next().is_some() {
            return Err(RuntimeGlueModelError::VisibleModelCount {
                count: 2 + visible.count(),
            });
        }
        let key = GlueModelKey::from_presentation(presentation);
        let light_variant = if glue_character
            .as_ref()
            .is_some_and(ResidentGlueCharacterFrameInput::is_ghost)
        {
            GlueModelLightVariant::Ghost
        } else {
            GlueModelLightVariant::Live
        };
        let mut environment = GlueModelEnvironment::from_presentation(
            presentation,
            light_variant,
            self.ghost_sunlight,
        )?;
        let external_directional_light = environment.background_light_override.is_some();
        let generation = GlueModelGenerationKey {
            path: key.path.clone(),
            external_directional_light,
        };
        let known_model = self
            .active
            .as_ref()
            .filter(|active| active.key.path == key.path)
            .map(|active| Arc::clone(&active.model))
            .or_else(|| {
                self.prepared
                    .get(&generation)
                    .map(|prepared| Arc::clone(&prepared.model))
            })
            .or_else(|| {
                self.pending
                    .iter()
                    .find(|pending| pending.generation == generation)
                    .map(|pending| Arc::clone(&pending.model))
            });
        let mut newly_loaded_generation = None;
        let lighting_model = match known_model {
            Some(model) => model,
            None => {
                let Some(loaded) = self
                    .backdrop_assets
                    .poll(&key.path, cpu)
                    .map_err(|source| RuntimeGlueModelError::BackdropLoad {
                        path: key.path.clone(),
                        source,
                    })?
                else {
                    return Ok(RuntimeGlueModelPoll::Pending);
                };
                let model = Arc::clone(&loaded.model);
                newly_loaded_generation = Some(loaded);
                model
            }
        };
        environment.shared_point_light_count = maximum_glue_point_light_count(&lighting_model);
        let active_matches = self.active.as_ref().is_some_and(|active| {
            active.key == key
                && active.local_light_count
                    == maximum_glue_light_count(&active.model, external_directional_light)
        });
        if !active_matches {
            self.character_replacement_required |= glue_character.is_some();
        }
        let character_sources_ready = if self.character_replacement_required
            && glue_character_expected
            && glue_character.is_none()
        {
            false
        } else if self.character_replacement_required {
            self.ensure_character_cpu_sources(
                renderer,
                glue_character.as_ref(),
                environment.character_light_count(),
                environment.pet_light_count(),
                cpu,
            )?
        } else {
            true
        };
        if let Some(active) = self.active.as_mut()
            && active_matches
        {
            if self.character_replacement_required && !character_sources_ready {
                return Ok(RuntimeGlueModelPoll::Pending);
            }
            if active.environment.model_scale != environment.model_scale
                || active.environment.rotation_radians != environment.rotation_radians
            {
                active.frame.update_glue_model_transform(
                    environment.model_scale,
                    environment.rotation_radians,
                )?;
            }
            active.environment = environment;
            if self.character_replacement_required {
                let gpu_started = std::time::Instant::now();
                active.frame.replace_glue_character(
                    renderer,
                    glue_character,
                    active.environment.character_light_count(),
                    active.environment.pet_light_count(),
                    &self.character_sources,
                    random,
                )?;
                tracing::info!(
                    gpu_prepare_ms = gpu_started.elapsed().as_secs_f64() * 1_000.0,
                    "published worker-prepared Glue character generation"
                );
                self.character_replacement_required = false;
            } else if !self.character_replacement_required
                && let Some(character) = glue_character
            {
                active
                    .frame
                    .update_glue_character_transform(character.facing_radians())?;
            }
            active.frame.set_glue_opacity(active.environment.alpha)?;
            self.character_screen = character_screen;
            self.service_backdrop_reads(cpu)?;
            return Ok(RuntimeGlueModelPoll::Ready);
        }
        if self.prepared.contains_key(&generation) {
            if self.character_replacement_required && !character_sources_ready {
                return Ok(RuntimeGlueModelPoll::Pending);
            }
            self.activate_prepared(
                renderer,
                generation,
                key,
                environment,
                glue_character,
                random,
                particle_twinkle,
            )?;
            self.character_replacement_required = false;
            self.character_screen = character_screen;
            self.service_backdrop_reads(cpu)?;
            return Ok(RuntimeGlueModelPoll::Ready);
        }
        if let Some(pending_index) = self
            .pending
            .iter()
            .position(|pending| pending.generation == generation)
        {
            if !self.pending[pending_index].task.is_finished() {
                return Ok(RuntimeGlueModelPoll::Pending);
            }
            self.complete_pending(
                renderer,
                pending_index,
                "prepared resident Glue model generation",
            )?;
            if self.character_replacement_required && !character_sources_ready {
                return Ok(RuntimeGlueModelPoll::Pending);
            }
            self.activate_prepared(
                renderer,
                generation,
                key,
                environment,
                glue_character,
                random,
                particle_twinkle,
            )?;
            self.character_replacement_required = false;
            self.character_screen = character_screen;
            self.service_backdrop_reads(cpu)?;
            return Ok(RuntimeGlueModelPoll::Ready);
        }
        let loaded = match newly_loaded_generation {
            Some(loaded) => Some(loaded),
            None => self
                .backdrop_assets
                .poll(&key.path, cpu)
                .map_err(|source| RuntimeGlueModelError::BackdropLoad {
                    path: key.path.clone(),
                    source,
                })?,
        };
        let Some(loaded) = loaded else {
            return Ok(RuntimeGlueModelPoll::Pending);
        };
        let model = Arc::clone(&loaded.model);
        let environment_light_count = maximum_glue_light_count(&model, external_directional_light);
        let task_model = Arc::clone(&model);
        let task = match cpu
            .try_submit(move || prepare_glue_cpu_task(&task_model, environment_light_count))
        {
            Ok(task) => task,
            Err(CpuError::AtCapacity { .. }) => return Ok(RuntimeGlueModelPoll::Pending),
            Err(source) => return Err(source.into()),
        };
        self.pending.push(PendingGlueModel {
            generation,
            local_light_count: environment_light_count,
            model,
            textures: loaded.textures.clone(),
            submitted_at: std::time::Instant::now(),
            task,
        });
        Ok(RuntimeGlueModelPoll::Pending)
    }

    /// Reports whether every immutable CPU source for the current preview is resident.
    fn ensure_character_cpu_sources(
        &mut self,
        renderer: &mut VulkanRenderer,
        input: Option<&ResidentGlueCharacterFrameInput<'_>>,
        character_light_count: M2LocalLightCount,
        pet_light_count: M2LocalLightCount,
        cpu: &CpuExecutor,
    ) -> Result<bool, RuntimeGlueModelError> {
        self.complete_finished_character_tasks()?;
        let Some(input) = input else {
            return Ok(true);
        };
        let models = glue_character_cpu_models(input, character_light_count, pet_light_count);
        if models
            .iter()
            .any(|(key, _model, _orientation)| !self.character_sources.contains_key(key))
        {
            let mut submitted_count = 0_usize;
            for (key, model, _orientation) in &models {
                if self.character_sources.contains_key(key)
                    || self
                        .pending_character_sources
                        .iter()
                        .any(|pending| pending.key == *key)
                {
                    continue;
                }
                let local_light_count = key.local_light_count();
                let task_model = Arc::clone(model);
                let task = match cpu
                    .try_submit(move || prepare_glue_cpu_task(&task_model, local_light_count))
                {
                    Ok(task) => task,
                    // A preview can contain more equipment/effect models than
                    // the bounded executor can admit at once. Preserve the
                    // already-submitted prefix and retry the remainder during
                    // the next non-blocking synchronization pass.
                    Err(CpuError::AtCapacity { .. }) => break,
                    Err(source) => return Err(source.into()),
                };
                self.pending_character_sources
                    .push(PendingGlueCharacterSource {
                        key: key.clone(),
                        submitted_at: std::time::Instant::now(),
                        task,
                    });
                submitted_count += 1;
            }
            if submitted_count != 0 {
                tracing::info!(
                    model_count = submitted_count,
                    "submitted parallel Glue character sources to bounded CPU executor"
                );
            }
            return Ok(false);
        }

        let required = models
            .iter()
            .map(|(key, _model, orientation)| (key.clone(), *orientation))
            .collect::<HashSet<_>>();
        self.pending_character_pipeline_warmups
            .retain(|pending| required.contains(&pending.key));
        for key in &required {
            if self.warmed_character_pipelines.contains(key)
                || self
                    .pending_character_pipeline_warmups
                    .iter()
                    .any(|pending| pending.key == *key)
            {
                continue;
            }
            let source = self
                .character_sources
                .get(&key.0)
                .ok_or(RuntimeGlueModelError::PendingState)?;
            self.pending_character_pipeline_warmups
                .push_back(PendingGlueCharacterPipelineWarmup {
                    key: key.clone(),
                    warmup: M2GluePipelineWarmup::new(source, key.1),
                });
        }
        if let Some(pending) = self.pending_character_pipeline_warmups.front_mut()
            && pending.warmup.service_one(renderer)?
        {
            let complete = self
                .pending_character_pipeline_warmups
                .pop_front()
                .ok_or(RuntimeGlueModelError::PendingState)?;
            self.warmed_character_pipelines.insert(complete.key);
        }
        Ok(required
            .iter()
            .all(|key| self.warmed_character_pipelines.contains(key)))
    }

    /// Joins completed character tasks without ever waiting on the presentation thread.
    fn complete_finished_character_tasks(&mut self) -> Result<(), RuntimeGlueModelError> {
        let finished = self
            .pending_character_sources
            .iter()
            .enumerate()
            .filter_map(|(index, pending)| pending.task.is_finished().then_some(index))
            .rev()
            .collect::<Vec<_>>();
        for index in finished {
            let pending = self.pending_character_sources.swap_remove(index);
            let prepared = pending.task.join()??;
            self.character_sources
                .insert(pending.key.clone(), Arc::new(prepared.source));
            tracing::info!(
                model = %pending.key.path(),
                worker_prepare_ms = prepared.elapsed.as_secs_f64() * 1_000.0,
                residency_wait_ms = pending.submitted_at.elapsed().as_secs_f64() * 1_000.0,
                "completed Glue character CPU source"
            );
        }
        Ok(())
    }

    /// Publishes one finished worker generation into renderer-owned residency.
    fn complete_pending(
        &mut self,
        renderer: &mut VulkanRenderer,
        pending_index: usize,
        message: &'static str,
    ) -> Result<(), RuntimeGlueModelError> {
        if pending_index >= self.pending.len() {
            return Err(RuntimeGlueModelError::PendingState);
        }
        let pending = self.pending.swap_remove(pending_index);
        let cpu_source = pending.task.join()??;
        let residency_wait = pending.submitted_at.elapsed();
        let gpu_started = std::time::Instant::now();
        let source = M2Frame::prepare_glue_gpu_source(
            renderer,
            Arc::clone(&pending.model),
            &pending.textures,
            &cpu_source.source,
            pending.local_light_count,
        )?;
        let gpu_elapsed = gpu_started.elapsed();
        tracing::info!(
            model = %pending.model.path(),
            texture_count = pending.textures.len(),
            worker_prepare_ms = cpu_source.elapsed.as_secs_f64() * 1_000.0,
            residency_wait_ms = residency_wait.as_secs_f64() * 1_000.0,
            gpu_prepare_ms = gpu_elapsed.as_secs_f64() * 1_000.0,
            message
        );
        self.prepared.insert(
            pending.generation,
            PreparedGlueModel {
                local_light_count: pending.local_light_count,
                model: pending.model,
                source,
            },
        );
        Ok(())
    }

    /// Constructs fresh mutable playback from one retained immutable source.
    #[allow(clippy::too_many_arguments)]
    fn activate_prepared(
        &mut self,
        renderer: &mut VulkanRenderer,
        generation: GlueModelGenerationKey,
        key: GlueModelKey,
        environment: GlueModelEnvironment,
        glue_character: Option<ResidentGlueCharacterFrameInput<'_>>,
        random: &mut CrtRand,
        particle_twinkle: Arc<M2ParticleTwinkleTable>,
    ) -> Result<(), RuntimeGlueModelError> {
        self.retire_active_script_model();
        let (playback, animation_started_at) = self.take_script_playback(&key)?;
        let prepared = self
            .prepared
            .get(&generation)
            .ok_or(RuntimeGlueModelError::PendingState)?;
        let model = Arc::clone(&prepared.model);
        let local_light_count = prepared.local_light_count;
        let source = prepared.source.instantiate();
        let mut frame = M2Frame::activate_glue_gpu_source(
            source,
            key.object_index,
            playback,
            environment.model_scale,
            environment.rotation_radians,
            animation_started_at,
            particle_twinkle,
        )?;
        frame.replace_glue_character(
            renderer,
            glue_character,
            environment.character_light_count(),
            environment.pet_light_count(),
            &self.character_sources,
            random,
        )?;
        frame.set_glue_opacity(environment.alpha)?;
        tracing::info!(model = %model.path(), instance_generation = key.instance_generation,
            "activated resident Glue model generation");
        self.active = Some(ActiveGlueModel {
            key,
            environment,
            local_light_count,
            model,
            frame,
        });
        Ok(())
    }

    /// Presents the authored model/effects and then the loaded FrameXML pass.
    pub(crate) fn present(
        &mut self,
        renderer: &mut VulkanRenderer,
        glue: &GlueManager,
        ui: &RuntimeUiFrame,
        global_time_ms: f32,
        random: &mut CrtRand,
        overlay: &[solarity_rendering::UiPreparedDraw],
    ) -> Result<bool, RuntimeGlueModelError> {
        let Some(active) = self.active.as_mut() else {
            return Ok(false);
        };
        let ui_extent = ui.logical_extent();
        let screen_window = model_screen_window(
            active.key.object_index,
            active.environment.bounds,
            ui_extent,
        )?;
        let animation_time_ms = active.frame.animation_time_ms();
        let clock =
            active
                .frame
                .advance_glue_animation_clock(animation_time_ms, global_time_ms, random)?;
        let (camera, effect_scale) = sample_m2_ui_camera_frame(
            active.model.animations(),
            active.environment.camera,
            clock,
            active.environment.camera_viewport,
            active.frame.glue_model_transform()?,
        )?;
        self.sound_camera = Some(camera);
        let frustum =
            WorldFrustum::new(camera, WorldScreenWindow::FULL).map_err(M2CameraFrameError::from)?;
        let prepare_started = std::time::Instant::now();
        let visible = active.frame.prepare_visible_draws(
            renderer,
            frustum,
            camera,
            active.environment.fog_color,
            animation_time_ms,
            global_time_ms,
            effect_scale,
            random,
            None,
        )?;
        let sunlight = active
            .environment
            .background_light_override
            .map(M2LightOverride::sunlight)
            .or_else(|| merge_wotlk_directional_lights(visible.glue_directional_lights));
        let mut environment_local_lights = [M2LocalLightState::disabled(); 4];
        let mut environment_light_count = 0_usize;
        if let Some(sunlight) = sunlight {
            environment_local_lights[0] = sunlight.local_light_state();
            environment_light_count = 1;
        }
        for point in visible.glue_point_lights.iter().take(
            active
                .environment
                .background_light_override
                .map_or(4 - environment_light_count, |light| {
                    light.point_light_count(visible.glue_point_lights.len())
                }),
        ) {
            environment_local_lights[environment_light_count] = point.local_light_state();
            environment_light_count += 1;
        }
        let (environment_ambient, environment_diffuse) = if sunlight.is_none() {
            (active.environment.ambient, active.environment.diffuse)
        } else {
            (Vec3::ZERO, Vec3::ZERO)
        };
        let terrain = TerrainSceneUniform::new(
            camera.view_projection(),
            environment_ambient,
            environment_diffuse,
            active.environment.light_direction,
        );
        let world_model = WorldModelSceneUniform::new(
            camera.view_projection(),
            camera.camera().position(),
            environment_ambient,
            environment_diffuse,
            active.environment.light_direction,
            active.environment.fog_range,
        );
        let specular_enabled = glue.cvar_boolean("specular");
        let model = M2SceneUniform::new(
            camera.view_projection(),
            camera.view(),
            camera.camera().position(),
            environment_ambient,
            environment_diffuse,
            active.environment.light_direction,
            active.environment.fog_range,
            active.environment.fog_color,
            environment_local_lights,
        )
        .with_specular_enabled(specular_enabled);
        let character_local_lights =
            if let Some(light) = active.environment.character_light_override {
                light.local_lights(visible.glue_point_lights)
            } else {
                let mut lights = [M2LocalLightState::disabled(); 4];
                lights[0] = glue_character_sunlight(camera.camera()).local_light_state();
                append_point_lights(&mut lights, visible.glue_point_lights);
                lights
            };
        let pet_local_lights = active
            .environment
            .pet_light_override
            .map_or(character_local_lights, |light| {
                light.local_lights(visible.glue_point_lights)
            });
        let character_model = M2SceneUniform::new(
            camera.view_projection(),
            camera.view(),
            camera.camera().position(),
            Vec3::ZERO,
            Vec3::ZERO,
            active.environment.light_direction,
            active.environment.fog_range,
            active.environment.fog_color,
            character_local_lights,
        )
        .with_specular_enabled(specular_enabled);
        let pet_model = character_model.with_local_lights(pet_local_lights);
        let gamma_value = glue.cvar_value("gamma");
        let gamma = gamma_value
            .as_deref()
            .and_then(|value| value.parse::<f32>().ok())
            .filter(|value| value.is_finite())
            .ok_or(RuntimeGlueModelError::InvalidGamma { value: gamma_value })?;
        let strength = if glue.cvar_boolean("ffxGlow") {
            active.environment.glow
        } else {
            0.0
        };
        let scene = WorldFrameScene::new(terrain, world_model, model)
            .with_m2_light_banks(character_model, pet_model)
            .with_particle_capacity(
                visible.particle_vertex_capacity,
                visible.particle_index_capacity,
            );
        let prepare_elapsed = prepare_started.elapsed();
        let draw_count = visible.draws.len();
        let bone_count = visible.bone_transforms.len();
        let particle_vertex_count = visible.particle_vertices.len();
        let present_started = std::time::Instant::now();
        if strength > 0.0 || (gamma - 1.0).abs() > 0.0001 {
            renderer.present_world_frame_with_ui_layers_and_glow(
                scene,
                visible.bone_transforms,
                &[],
                &[],
                visible.draws,
                visible.particle_vertices,
                visible.particle_indices,
                visible.particle_draws,
                visible.ribbon_vertices,
                visible.ribbon_draws,
                screen_window,
                WorldFrameGlow::new(strength.clamp(0.0, 4.0), gamma.clamp(0.1, 4.0))?,
                ui.logical_extent(),
                ui.draws(),
                overlay,
            )?;
        } else {
            renderer.present_world_frame_with_ui_layers(
                scene,
                visible.bone_transforms,
                &[],
                &[],
                visible.draws,
                visible.particle_vertices,
                visible.particle_indices,
                visible.particle_draws,
                visible.ribbon_vertices,
                visible.ribbon_draws,
                screen_window,
                ui.logical_extent(),
                ui.draws(),
                overlay,
            )?;
        }
        if let Some(profiler) = self.frame_profiler.as_mut() {
            profiler.record(
                prepare_elapsed,
                present_started.elapsed(),
                draw_count,
                bone_count,
                particle_vertex_count,
            );
        }
        Ok(true)
    }

    /// Transfers visible Glue-model audio callbacks with their sampled listener.
    pub(crate) fn drain_sound_events(&mut self) -> Option<(WorldCameraFrame, Vec<RuntimeM2Event>)> {
        let camera = self.sound_camera?;
        let events = self.active.as_mut()?.frame.drain_triggered_events();
        (!events.is_empty()).then_some((camera, events))
    }
}

/// Converts a bottom-left UI rectangle to Vulkan's normalized model window.
fn model_screen_window(
    object_index: usize,
    bounds: UiScreenRect,
    ui_extent: [f32; 2],
) -> Result<WorldScreenWindow, RuntimeGlueModelError> {
    let ui_width = f64::from(ui_extent[0]);
    let ui_height = f64::from(ui_extent[1]);
    let values = [
        bounds.left(),
        bounds.bottom(),
        bounds.right(),
        bounds.top(),
        ui_width,
        ui_height,
    ];
    if values.into_iter().any(|value| !value.is_finite())
        || ui_width <= 0.0
        || ui_height <= 0.0
        || bounds.width() <= 0.0
        || bounds.height() <= 0.0
        || bounds.left() < 0.0
        || bounds.bottom() < 0.0
        || bounds.right() > ui_width
        || bounds.top() > ui_height
    {
        return Err(RuntimeGlueModelError::InvalidViewport { object_index });
    }
    Ok(WorldScreenWindow::new(
        (2.0 * bounds.left() / ui_width - 1.0) as f32,
        (2.0 * bounds.bottom() / ui_height - 1.0) as f32,
        (2.0 * bounds.right() / ui_width - 1.0) as f32,
        (2.0 * bounds.top() / ui_height - 1.0) as f32,
    ))
}

fn active_local_light_count(lights: [M2LocalLightState; 4]) -> usize {
    lights
        .iter()
        .filter(|light| **light != M2LocalLightState::disabled())
        .count()
}

fn append_point_lights(
    lights: &mut [M2LocalLightState; 4],
    points: &[solarity_rendering::M2PointLight],
) {
    let light_count = active_local_light_count(*lights);
    for (offset, point) in points.iter().take(4 - light_count).enumerate() {
        lights[light_count + offset] = point.local_light_state();
    }
}

fn maximum_glue_point_light_count(model: &solarity_asset::DecodedM2Model) -> usize {
    model
        .animations()
        .lights()
        .iter()
        .filter(|light| light.kind() == M2LightKind::Point)
        .count()
        .min(3)
}

/// Reserves stock's slot-zero directional aggregate and at most three point
/// slots for every light that may become visible over the model's lifetime.
fn maximum_glue_light_count(
    model: &solarity_asset::DecodedM2Model,
    external_directional_light: bool,
) -> M2LocalLightCount {
    let authored = model.animations().lights();
    let has_directional = external_directional_light
        || authored
            .iter()
            .any(|light| light.kind() == M2LightKind::Directional);
    let point_count = maximum_glue_point_light_count(model);
    local_light_count_from_len(usize::from(has_directional) + point_count)
}

fn local_light_count_from_len(count: usize) -> M2LocalLightCount {
    match count {
        0 => M2LocalLightCount::Zero,
        1 => M2LocalLightCount::One,
        2 => M2LocalLightCount::Two,
        3 => M2LocalLightCount::Three,
        4 => M2LocalLightCount::Four,
        _ => unreachable!("fixed ModelFFX light array exceeded four slots"),
    }
}
