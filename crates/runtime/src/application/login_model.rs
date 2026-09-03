//! Retained stock M2 scene inserted beneath pre-world Glue presentation.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::{Vec3, Vec4};
use solarity_asset::{
    AssetError, AssetPath, AssetStoreHandle, BlpTextureCache, M2HardcodedTextureSource,
    M2LightKind, M2ModelCache, M2TextureKind,
};
use solarity_cpu::{CpuError, CpuExecutor, CpuTask};
use solarity_rendering::{
    M2CameraFrameError, M2DirectionalLight, M2LocalLightCount, M2LocalLightState,
    M2ParticleTwinkleTable, M2SceneUniform, TerrainSceneUniform, VulkanError, VulkanRenderer,
    WorldFrameGlow, WorldFrameScene, WorldFrustum, WorldModelSceneUniform, WorldScreenWindow,
    glue_character_sunlight, merge_wotlk_directional_lights, sample_m2_camera_frame,
};
use solarity_ui::{GlueManager, UiModelLight, UiModelLightSets, UiModelPresentation, UiScreenRect};
use thiserror::Error;

use crate::application::login_ui::RuntimeUiFrame;
use crate::application::player_coordinator::ResidentGlueCharacterFrameInput;
use crate::application::terrain_frame::RuntimeTerrainFrameError;
use crate::application::terrain_frame::m2::{
    GlueM2Texture, M2Frame, M2GlueCpuSource, M2GlueCpuSourceKey, prepare_glue_cpu_source,
};
use crate::random::CrtRand;

const STOCK_GLUE_AMBIENT: Vec3 = Vec3::splat(0.35);
const STOCK_GLUE_DIFFUSE: Vec3 = Vec3::splat(0.65);
const STOCK_GLUE_LIGHT_DIRECTION: Vec3 = Vec3::new(-0.35, 0.45, 0.82);
const STOCK_LOGIN_FOG_COLOR: Vec3 = Vec3::new(0.25, 0.06, 0.015);
const STOCK_LOGIN_FOG_RANGE: Vec4 = Vec4::new(0.0, 1200.0, 0.0, 1.0);
const STOCK_M2_CAMERA_ASPECT_RATIO: f32 = 4.0 / 3.0;

/// A visible Glue model cannot enter the retained M2 compositor callback.
#[derive(Debug, Error)]
pub enum RuntimeGlueModelError {
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
    /// Lua selected a negative camera slot.
    #[error("Glue model {object_index} selected negative camera {camera}")]
    NegativeCamera { object_index: usize, camera: i32 },
    /// The Glue sequence ABI is wider than the model animation identifier.
    #[error("Glue model {object_index} selected animation {sequence} outside the M2 domain")]
    AnimationCapacity { object_index: usize, sequence: u32 },
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
    camera: i32,
    sequence: u32,
    sequence_time_sequence: u32,
    sequence_time_ms: i32,
    model_scale: f32,
}

impl GlueModelKey {
    fn from_presentation(model: &UiModelPresentation) -> Self {
        Self {
            object_index: model.object_index(),
            path: model.path().clone(),
            camera: model.camera(),
            sequence: model.sequence(),
            sequence_time_sequence: model.sequence_time_sequence(),
            sequence_time_ms: model.sequence_time_ms(),
            model_scale: model.model_scale(),
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

/// One completed worker result and its execution time, excluding queue delay.
struct PreparedGlueCpuSource {
    source: M2GlueCpuSource,
    elapsed: std::time::Duration,
}

/// One character, equipment, effect, or pet source prepared independently.
struct PendingGlueCharacterSource {
    key: M2GlueCpuSourceKey,
    submitted_at: std::time::Instant,
    task: CpuTask<Result<PreparedGlueCpuSource, RuntimeTerrainFrameError>>,
}

/// Collects each distinct immutable model/light permutation in one preview.
fn glue_character_cpu_models(
    input: &ResidentGlueCharacterFrameInput<'_>,
    character_light_count: M2LocalLightCount,
    pet_light_count: M2LocalLightCount,
) -> Vec<(M2GlueCpuSourceKey, Arc<solarity_asset::DecodedM2Model>)> {
    let mut seen = HashSet::new();
    let mut models = Vec::new();
    let mut push = |model: &Arc<solarity_asset::DecodedM2Model>, light_count| {
        let key = M2GlueCpuSourceKey::new(model.path().clone(), light_count);
        if seen.insert(key.clone()) {
            models.push((key, Arc::clone(model)));
        }
    };
    push(input.model(), character_light_count);
    for attachment in input.attachments() {
        push(attachment.model(), character_light_count);
        for effect in attachment.visual_effects() {
            push(effect.model(), character_light_count);
        }
    }
    if let Some(pet) = input.pet() {
        push(pet.model(), pet_light_count);
    }
    models
}

/// Measures the worker-owned immutable preparation stage without main-thread wait time.
fn prepare_glue_cpu_task(
    model: &Arc<solarity_asset::DecodedM2Model>,
    local_light_count: M2LocalLightCount,
) -> Result<PreparedGlueCpuSource, RuntimeTerrainFrameError> {
    let started = std::time::Instant::now();
    let source = prepare_glue_cpu_source(model, local_light_count)?;
    Ok(PreparedGlueCpuSource {
        source,
        elapsed: started.elapsed(),
    })
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
    ambient: Vec3,
    diffuse: Vec3,
    light_direction: Vec3,
    fog_color: Vec3,
    fog_range: Vec4,
    background_directional_lights: [Option<M2DirectionalLight>; 4],
    character_local_lights: [M2LocalLightState; 4],
    pet_local_lights: [M2LocalLightState; 4],
    shared_point_light_count: usize,
    character_uses_camera_light: bool,
    pet_inherits_character_light: bool,
    bounds: UiScreenRect,
    alpha: f32,
    glow: f32,
}

impl GlueModelEnvironment {
    fn from_presentation(
        model: &UiModelPresentation,
        light_variant: GlueModelLightVariant,
    ) -> Self {
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
        let background_directional_lights =
            model_directional_lights(light_variant.select(model.background_lights()));
        let character_lights = light_variant.select(model.character_lights());
        let character_uses_camera_light = !character_lights.iter().any(Option::is_some);
        let character_local_lights = model_light_states(model_directional_lights(character_lights));
        let pet_lights = light_variant.select(model.pet_lights());
        let pet_inherits_character_light = !pet_lights.iter().any(Option::is_some);
        let pet_local_lights = model_light_states(model_directional_lights(pet_lights));
        Self {
            ambient: STOCK_GLUE_AMBIENT,
            diffuse: STOCK_GLUE_DIFFUSE,
            light_direction: STOCK_GLUE_LIGHT_DIRECTION,
            fog_color,
            fog_range,
            background_directional_lights,
            character_local_lights,
            pet_local_lights,
            shared_point_light_count: 0,
            character_uses_camera_light,
            pet_inherits_character_light,
            bounds: model.bounds(),
            alpha: model.alpha(),
            glow: model.glow(),
        }
    }

    /// Returns the shader light count after stock directional-light merging.
    fn character_light_count(self) -> M2LocalLightCount {
        let directional_count = if self.character_uses_camera_light {
            1
        } else {
            active_local_light_count(self.character_local_lights)
        };
        local_light_count_from_len((directional_count + self.shared_point_light_count).min(4))
    }

    /// Applies stock's pet fallback to the effective character light bank.
    fn pet_light_count(self) -> M2LocalLightCount {
        if self.pet_inherits_character_light {
            self.character_light_count()
        } else {
            let directional_count = active_local_light_count(self.pet_local_lights);
            local_light_count_from_len((directional_count + self.shared_point_light_count).min(4))
        }
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

fn model_light_states(lights: [Option<M2DirectionalLight>; 4]) -> [M2LocalLightState; 4] {
    let directional_lights = lights.into_iter().flatten().collect::<Vec<_>>();
    let mut merged = [M2LocalLightState::disabled(); 4];
    if let Some(sunlight) = merge_wotlk_directional_lights(&directional_lights) {
        merged[0] = sunlight.local_light_state();
    }
    merged
}

/// Process-long model and texture caches plus the current Glue M2 generation.
pub(crate) struct RuntimeGlueModelScene {
    models: M2ModelCache,
    textures: BlpTextureCache,
    active: Option<ActiveGlueModel>,
    pending: Vec<PendingGlueModel>,
    prepared: HashMap<GlueModelGenerationKey, PreparedGlueModel>,
    character_sources: HashMap<M2GlueCpuSourceKey, Arc<M2GlueCpuSource>>,
    pending_character_sources: Vec<PendingGlueCharacterSource>,
    character_replacement_required: bool,
}

impl RuntimeGlueModelScene {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            models: M2ModelCache::new(),
            textures: BlpTextureCache::new(),
            active: None,
            pending: Vec::new(),
            prepared: HashMap::new(),
            character_sources: HashMap::new(),
            pending_character_sources: Vec::new(),
            character_replacement_required: false,
        }
    }

    /// Begins immutable environment-model preparation before its Glue screen
    /// becomes visible. The movie screen intentionally has no visible model,
    /// but stock has already assigned AccountLogin's source in `OnLoad`.
    pub(crate) fn prewarm(
        &mut self,
        path: AssetPath,
        background_light_count: usize,
        assets: &AssetStoreHandle,
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
        let (model, textures) = self.load_model_generation(assets, &path)?;
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

    /// Completes and activates a hidden prewarm before the cinematic presents.
    ///
    /// Vulkan resource ownership stays on the presentation thread, while the
    /// mesh plan and shader compilation are joined from the bounded CPU pool.
    /// Playback begins here so the cinematic can advance the same live effects
    /// that EULA reveals, rather than constructing an empty instance afterward.
    pub(crate) fn finish_prewarm(
        &mut self,
        renderer: &mut VulkanRenderer,
        presentation: &UiModelPresentation,
        random: &mut CrtRand,
        particle_twinkle: Arc<M2ParticleTwinkleTable>,
    ) -> Result<(), RuntimeGlueModelError> {
        let key = GlueModelKey::from_presentation(presentation);
        let environment =
            GlueModelEnvironment::from_presentation(presentation, GlueModelLightVariant::Live);
        let generation = GlueModelGenerationKey {
            path: key.path.clone(),
            external_directional_light: environment
                .background_directional_lights
                .iter()
                .any(Option::is_some),
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
        self.activate_prepared(
            renderer,
            generation,
            key,
            environment,
            None,
            random,
            particle_twinkle,
        )?;
        Ok(())
    }

    /// Publishes every finished backdrop prewarm while authentication UI is
    /// covering the login scene.
    ///
    /// CPU preparation begins at startup, but Vulkan ownership requires the
    /// presentation thread. Draining that work here makes the subsequent Glue
    /// screen change a cache lookup instead of exposing a clear-only frame.
    pub(crate) fn finish_authentication_prewarms(
        &mut self,
        renderer: &mut VulkanRenderer,
    ) -> Result<bool, RuntimeGlueModelError> {
        let mut completed = 0_usize;
        while let Some(index) = self
            .pending
            .iter()
            .position(|pending| pending.task.is_finished())
        {
            self.complete_pending(
                renderer,
                index,
                "prepared authentication-covered Glue model generation",
            )?;
            completed += 1;
        }
        let complete = self.pending.is_empty();
        if completed != 0 {
            tracing::info!(completed, complete, "serviced Glue authentication prewarm");
        }
        if complete
            && completed != 0
            && let Err(source) = renderer.save_pipeline_cache()
        {
            tracing::warn!(
                error = %source,
                "could not checkpoint authentication Vulkan pipeline cache"
            );
        }
        Ok(complete)
    }

    /// Advances the resident login model while the cinematic covers it.
    ///
    /// This performs no swapchain submission. It only keeps animation,
    /// particle, ribbon, material, and light state current for the first EULA
    /// frame that follows the movie.
    pub(crate) fn advance_hidden(
        &mut self,
        renderer: &VulkanRenderer,
        global_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeGlueModelError> {
        let Some(active) = self.active.as_mut() else {
            return Ok(());
        };
        let aspect_ratio =
            active.environment.bounds.width() as f32 / active.environment.bounds.height() as f32;
        let animation_time_ms = active.frame.animation_time_ms();
        let clock =
            active
                .frame
                .advance_glue_animation_clock(animation_time_ms, global_time_ms, random)?;
        let camera_index = usize::try_from(active.key.camera).map_err(|_source| {
            RuntimeGlueModelError::NegativeCamera {
                object_index: active.key.object_index,
                camera: active.key.camera,
            }
        })?;
        let camera =
            sample_m2_camera_frame(active.model.animations(), camera_index, clock, aspect_ratio)?;
        let particle_view_scale =
            1.0_f32.hypot(STOCK_M2_CAMERA_ASPECT_RATIO) / 1.0_f32.hypot(aspect_ratio);
        let frustum =
            WorldFrustum::new(camera, WorldScreenWindow::FULL).map_err(M2CameraFrameError::from)?;
        active.frame.prepare_visible_draws(
            renderer,
            frustum,
            camera,
            active.environment.fog_color,
            particle_view_scale,
            animation_time_ms,
            global_time_ms,
            random,
        )?;
        Ok(())
    }

    /// Rebuilds GPU state only when Glue changes the selected model generation.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn synchronize(
        &mut self,
        renderer: &mut VulkanRenderer,
        glue: &GlueManager,
        assets: &AssetStoreHandle,
        cpu: &CpuExecutor,
        random: &mut CrtRand,
        particle_twinkle: Arc<M2ParticleTwinkleTable>,
        glue_character: Option<ResidentGlueCharacterFrameInput<'_>>,
        glue_character_changed: bool,
    ) -> Result<(), RuntimeGlueModelError> {
        let visible = glue.presentation().models();
        if visible.is_empty() {
            // CharacterSelect briefly owns an empty directory before its
            // asynchronous enumeration arrives. Keep the last fully rendered
            // Glue generation underneath that transition instead of clearing
            // the swapchain between complete scenes.
            if self.active.is_some()
                && matches!(glue.current_screen().as_str(), "charselect" | "charcreate")
            {
                return Ok(());
            }
            self.active = None;
            return Ok(());
        }
        if visible.len() != 1 {
            return Err(RuntimeGlueModelError::VisibleModelCount {
                count: visible.len(),
            });
        }
        let presentation = &visible[0];
        let key = GlueModelKey::from_presentation(presentation);
        let light_variant = if glue_character
            .as_ref()
            .is_some_and(ResidentGlueCharacterFrameInput::is_ghost)
        {
            GlueModelLightVariant::Ghost
        } else {
            GlueModelLightVariant::Live
        };
        let mut environment = GlueModelEnvironment::from_presentation(presentation, light_variant);
        let external_directional_light = environment
            .background_directional_lights
            .iter()
            .any(Option::is_some);
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
                let loaded = self.load_model_generation(assets, &key.path)?;
                let model = Arc::clone(&loaded.0);
                newly_loaded_generation = Some(loaded);
                model
            }
        };
        environment.shared_point_light_count = maximum_glue_point_light_count(&lighting_model);
        if glue_character_changed {
            self.character_replacement_required = true;
        }
        let active_matches = self.active.as_ref().is_some_and(|active| {
            active.key == key
                && active.local_light_count
                    == maximum_glue_light_count(&active.model, external_directional_light)
        });
        if !active_matches {
            self.character_replacement_required = glue_character.is_some();
        }
        let character_sources_ready = if self.character_replacement_required {
            self.ensure_character_cpu_sources(
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
            active.environment = environment;
            if self.character_replacement_required && character_sources_ready {
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
                active.frame.update_glue_character_transform(
                    character.model_scale(),
                    character.facing_radians(),
                )?;
            }
            active.frame.set_glue_opacity(active.environment.alpha)?;
            return Ok(());
        }
        if key.camera < 0 {
            return Err(RuntimeGlueModelError::NegativeCamera {
                object_index: key.object_index,
                camera: key.camera,
            });
        }
        if self.prepared.contains_key(&generation) {
            if self.character_replacement_required && !character_sources_ready {
                return Ok(());
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
            return Ok(());
        }
        if let Some(pending_index) = self
            .pending
            .iter()
            .position(|pending| pending.generation == generation)
        {
            if !self.pending[pending_index].task.is_finished() {
                return Ok(());
            }
            self.complete_pending(
                renderer,
                pending_index,
                "prepared resident Glue model generation",
            )?;
            if self.character_replacement_required && !character_sources_ready {
                return Ok(());
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
            return Ok(());
        }
        let (model, texture_sources) = match newly_loaded_generation {
            Some(loaded) => loaded,
            None => self.load_model_generation(assets, &key.path)?,
        };
        let environment_light_count = maximum_glue_light_count(&model, external_directional_light);
        let task_model = Arc::clone(&model);
        let task =
            cpu.try_submit(move || prepare_glue_cpu_task(&task_model, environment_light_count))?;
        self.pending.push(PendingGlueModel {
            generation,
            local_light_count: environment_light_count,
            model,
            textures: texture_sources,
            submitted_at: std::time::Instant::now(),
            task,
        });
        Ok(())
    }

    /// Reports whether every immutable CPU source for the current preview is resident.
    fn ensure_character_cpu_sources(
        &mut self,
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
            .all(|(key, _model)| self.character_sources.contains_key(key))
        {
            return Ok(true);
        }
        let mut submitted_count = 0_usize;
        for (key, model) in models {
            if self.character_sources.contains_key(&key)
                || self
                    .pending_character_sources
                    .iter()
                    .any(|pending| pending.key == key)
            {
                continue;
            }
            let local_light_count = key.local_light_count();
            let task_model = Arc::clone(&model);
            let task =
                cpu.try_submit(move || prepare_glue_cpu_task(&task_model, local_light_count))?;
            self.pending_character_sources
                .push(PendingGlueCharacterSource {
                    key,
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
        Ok(false)
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
        if key.camera < 0 {
            return Err(RuntimeGlueModelError::NegativeCamera {
                object_index: key.object_index,
                camera: key.camera,
            });
        }
        let animation_id = u16::try_from(key.sequence).map_err(|_source| {
            RuntimeGlueModelError::AnimationCapacity {
                object_index: key.object_index,
                sequence: key.sequence,
            }
        })?;
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
            animation_id,
            key.model_scale,
            random,
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
        tracing::info!(model = %model.path(), "activated resident Glue model generation");
        self.active = Some(ActiveGlueModel {
            key,
            environment,
            local_light_count,
            model,
            frame,
        });
        Ok(())
    }

    fn load_model_generation(
        &mut self,
        assets: &AssetStoreHandle,
        path: &AssetPath,
    ) -> Result<(Arc<solarity_asset::DecodedM2Model>, Vec<GlueM2Texture>), RuntimeGlueModelError>
    {
        let asset_started = std::time::Instant::now();
        let mut store = assets.borrow_mut();
        let model = self.models.load(&mut store, path)?;
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
                M2HardcodedTextureSource::Archive(path) => {
                    match self.textures.load(&mut store, path) {
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
                    }
                }
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
        drop(store);
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
        let aspect_ratio =
            active.environment.bounds.width() as f32 / active.environment.bounds.height() as f32;
        let animation_time_ms = active.frame.animation_time_ms();
        let clock =
            active
                .frame
                .advance_glue_animation_clock(animation_time_ms, global_time_ms, random)?;
        let camera_index = usize::try_from(active.key.camera).map_err(|_source| {
            RuntimeGlueModelError::NegativeCamera {
                object_index: active.key.object_index,
                camera: active.key.camera,
            }
        })?;
        let camera =
            sample_m2_camera_frame(active.model.animations(), camera_index, clock, aspect_ratio)?;
        // Build 12340 carries its 4:3-authored diagonal-camera correction in
        // the view-model matrix. Particle flag 0x20 inherits that scale even
        // though our camera path converts the FOV directly.
        let particle_view_scale =
            1.0_f32.hypot(STOCK_M2_CAMERA_ASPECT_RATIO) / 1.0_f32.hypot(aspect_ratio);
        let frustum =
            WorldFrustum::new(camera, WorldScreenWindow::FULL).map_err(M2CameraFrameError::from)?;
        let visible = active.frame.prepare_visible_draws(
            renderer,
            frustum,
            camera,
            active.environment.fog_color,
            particle_view_scale,
            animation_time_ms,
            global_time_ms,
            random,
        )?;
        let external_directional_lights = active
            .environment
            .background_directional_lights
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let directional_lights = if external_directional_lights.is_empty() {
            visible.glue_directional_lights
        } else {
            &external_directional_lights
        };
        let mut environment_local_lights = [M2LocalLightState::disabled(); 4];
        let mut environment_light_count = 0_usize;
        if let Some(sunlight) = merge_wotlk_directional_lights(directional_lights) {
            environment_local_lights[0] = sunlight.local_light_state();
            environment_light_count = 1;
        }
        for point in visible
            .glue_point_lights
            .iter()
            .take(4 - environment_light_count)
        {
            environment_local_lights[environment_light_count] = point.local_light_state();
            environment_light_count += 1;
        }
        let (environment_ambient, environment_diffuse) = if directional_lights.is_empty() {
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
        let model = M2SceneUniform::new(
            camera.view_projection(),
            camera.camera().position(),
            environment_ambient,
            environment_diffuse,
            active.environment.light_direction,
            active.environment.fog_range,
            active.environment.fog_color,
            environment_local_lights,
        );
        let character_local_lights = if active.environment.character_uses_camera_light {
            let mut lights = [M2LocalLightState::disabled(); 4];
            lights[0] = glue_character_sunlight(camera.camera()).local_light_state();
            lights
        } else {
            active.environment.character_local_lights
        };
        let mut character_local_lights = character_local_lights;
        append_point_lights(&mut character_local_lights, visible.glue_point_lights);
        let mut pet_local_lights = if active.environment.pet_inherits_character_light {
            character_local_lights
        } else {
            active.environment.pet_local_lights
        };
        if !active.environment.pet_inherits_character_light {
            append_point_lights(&mut pet_local_lights, visible.glue_point_lights);
        }
        let character_model = M2SceneUniform::new(
            camera.view_projection(),
            camera.camera().position(),
            Vec3::ZERO,
            Vec3::ZERO,
            active.environment.light_direction,
            active.environment.fog_range,
            active.environment.fog_color,
            character_local_lights,
        );
        let pet_model = character_model.with_local_lights(pet_local_lights);
        let mut ui_draws = Vec::with_capacity(ui.draws().len() + overlay.len());
        ui_draws.extend_from_slice(ui.draws());
        ui_draws.extend_from_slice(overlay);
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
        if strength > 0.0 || (gamma - 1.0).abs() > 0.0001 {
            renderer.present_world_frame_with_ui_and_glow(
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
                &ui_draws,
            )?;
        } else {
            renderer.present_world_frame_with_ui(
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
                &ui_draws,
            )?;
        }
        Ok(true)
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
