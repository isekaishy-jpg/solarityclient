//! Retained stock M2 scene inserted beneath pre-world Glue presentation.

use std::sync::Arc;

use glam::{Vec3, Vec4};
use solarity_asset::{
    AssetError, AssetPath, AssetStoreHandle, BlpTextureCache, M2HardcodedTextureSource,
    M2ModelCache, M2TextureKind,
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
    GlueM2Texture, M2Frame, M2GlueCpuSource, prepare_glue_cpu_source,
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
    model: Arc<solarity_asset::DecodedM2Model>,
    frame: M2Frame,
}

/// One scene generation whose CPU plan and shaders are still being prepared.
struct PendingGlueModel {
    key: GlueModelKey,
    environment: GlueModelEnvironment,
    model: Arc<solarity_asset::DecodedM2Model>,
    textures: Vec<GlueM2Texture>,
    submitted_at: std::time::Instant,
    task: CpuTask<Result<M2GlueCpuSource, RuntimeTerrainFrameError>>,
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
    local_lights: [M2LocalLightState; 4],
    character_local_lights: [M2LocalLightState; 4],
    pet_local_lights: [M2LocalLightState; 4],
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
        let authored_lights = light_variant.select(model.background_lights());
        let has_authored_lights = authored_lights.iter().any(Option::is_some);
        let local_lights = model_light_states(authored_lights);
        let character_lights = light_variant.select(model.character_lights());
        let character_uses_camera_light = !character_lights.iter().any(Option::is_some);
        let character_local_lights = model_light_states(character_lights);
        let pet_lights = light_variant.select(model.pet_lights());
        let pet_inherits_character_light = !pet_lights.iter().any(Option::is_some);
        let pet_local_lights = model_light_states(pet_lights);
        Self {
            ambient: if has_authored_lights {
                Vec3::ZERO
            } else {
                STOCK_GLUE_AMBIENT
            },
            diffuse: if has_authored_lights {
                Vec3::ZERO
            } else {
                STOCK_GLUE_DIFFUSE
            },
            light_direction: STOCK_GLUE_LIGHT_DIRECTION,
            fog_color,
            fog_range,
            local_lights,
            character_local_lights,
            pet_local_lights,
            character_uses_camera_light,
            pet_inherits_character_light,
            bounds: model.bounds(),
            alpha: model.alpha(),
            glow: model.glow(),
        }
    }

    /// Returns the shader light count after stock directional-light merging.
    fn character_light_count(self) -> M2LocalLightCount {
        if self.character_uses_camera_light {
            M2LocalLightCount::One
        } else {
            local_light_count(self.character_local_lights)
        }
    }

    /// Applies stock's pet fallback to the effective character light bank.
    fn pet_light_count(self) -> M2LocalLightCount {
        if self.pet_inherits_character_light {
            self.character_light_count()
        } else {
            local_light_count(self.pet_local_lights)
        }
    }
}

/// Merges one ModelFFX bank through build 12340's SetupSunlight boundary.
fn model_light_states(lights: [Option<UiModelLight>; 4]) -> [M2LocalLightState; 4] {
    let directional_lights = lights
        .into_iter()
        .flatten()
        .map(|light| {
            M2DirectionalLight::new(
                Vec3::from_array(light.direction()),
                Vec3::from_array(light.ambient()),
                Vec3::from_array(light.diffuse()),
            )
        })
        .collect::<Vec<_>>();
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
    pending: Option<PendingGlueModel>,
}

impl RuntimeGlueModelScene {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            models: M2ModelCache::new(),
            textures: BlpTextureCache::new(),
            active: None,
            pending: None,
        }
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
            self.active = None;
            self.pending = None;
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
        let environment = GlueModelEnvironment::from_presentation(presentation, light_variant);
        if let Some(active) = self.active.as_mut()
            && active.key == key
        {
            active.environment = environment;
            if glue_character_changed {
                active.frame.replace_glue_character(
                    renderer,
                    glue_character,
                    active.environment.character_light_count(),
                    active.environment.pet_light_count(),
                    random,
                )?;
            } else if let Some(character) = glue_character {
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
        let animation_id = u16::try_from(key.sequence).map_err(|_source| {
            RuntimeGlueModelError::AnimationCapacity {
                object_index: key.object_index,
                sequence: key.sequence,
            }
        })?;
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.key != key)
        {
            self.pending = None;
        }
        if let Some(pending) = self.pending.as_mut() {
            pending.environment = environment;
            if !pending.task.is_finished() {
                return Ok(());
            }
            let pending = self
                .pending
                .take()
                .ok_or(RuntimeGlueModelError::PendingState)?;
            let worker_elapsed = pending.submitted_at.elapsed();
            let cpu_source = pending.task.join()??;
            let gpu_started = std::time::Instant::now();
            let mut frame = M2Frame::prepare_glue_model(
                renderer,
                Arc::clone(&pending.model),
                &pending.textures,
                &cpu_source,
                pending.key.object_index,
                animation_id,
                pending.key.model_scale,
                local_light_count(pending.environment.local_lights),
                random,
                particle_twinkle,
            )?;
            frame.replace_glue_character(
                renderer,
                glue_character,
                pending.environment.character_light_count(),
                pending.environment.pet_light_count(),
                random,
            )?;
            frame.set_glue_opacity(pending.environment.alpha)?;
            tracing::info!(
                model = %pending.model.path(),
                texture_count = pending.textures.len(),
                worker_elapsed_ms = worker_elapsed.as_secs_f64() * 1_000.0,
                gpu_prepare_ms = gpu_started.elapsed().as_secs_f64() * 1_000.0,
                "published Glue model generation"
            );
            self.active = Some(ActiveGlueModel {
                key: pending.key,
                environment: pending.environment,
                model: pending.model,
                frame,
            });
            return Ok(());
        }
        let asset_started = std::time::Instant::now();
        let mut store = assets.borrow_mut();
        let model = self.models.load(&mut store, &key.path)?;
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
        tracing::info!(
            model = %model.path(),
            texture_count = texture_sources.len(),
            asset_ms = asset_started.elapsed().as_secs_f64() * 1_000.0,
            "loaded Glue model archive generation"
        );
        let task_model = Arc::clone(&model);
        let light_count = local_light_count(environment.local_lights);
        let task = cpu.try_submit(move || prepare_glue_cpu_source(&task_model, light_count))?;
        self.active = None;
        self.pending = Some(PendingGlueModel {
            key,
            environment,
            model,
            textures: texture_sources,
            submitted_at: std::time::Instant::now(),
            task,
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
        let frustum =
            WorldFrustum::new(camera, WorldScreenWindow::FULL).map_err(M2CameraFrameError::from)?;
        let visible = active.frame.prepare_visible_draws(
            renderer,
            frustum,
            camera,
            active.environment.fog_color,
            animation_time_ms,
            global_time_ms,
            random,
        )?;
        let terrain = TerrainSceneUniform::new(
            camera.view_projection(),
            active.environment.ambient,
            active.environment.diffuse,
            active.environment.light_direction,
        );
        let world_model = WorldModelSceneUniform::new(
            camera.view_projection(),
            camera.camera().position(),
            active.environment.ambient,
            active.environment.diffuse,
            active.environment.light_direction,
            active.environment.fog_range,
        );
        let model = M2SceneUniform::new(
            camera.view_projection(),
            camera.camera().position(),
            active.environment.ambient,
            active.environment.diffuse,
            active.environment.light_direction,
            active.environment.fog_range,
            active.environment.local_lights,
        );
        let character_local_lights = if active.environment.character_uses_camera_light {
            let mut lights = [M2LocalLightState::disabled(); 4];
            lights[0] = glue_character_sunlight(camera.camera()).local_light_state();
            lights
        } else {
            active.environment.character_local_lights
        };
        let pet_local_lights = if active.environment.pet_inherits_character_light {
            character_local_lights
        } else {
            active.environment.pet_local_lights
        };
        let character_model = M2SceneUniform::new(
            camera.view_projection(),
            camera.camera().position(),
            Vec3::ZERO,
            Vec3::ZERO,
            active.environment.light_direction,
            active.environment.fog_range,
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
            .with_m2_light_banks(character_model, pet_model);
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

fn local_light_count(lights: [M2LocalLightState; 4]) -> M2LocalLightCount {
    let count = lights
        .iter()
        .filter(|light| **light != M2LocalLightState::disabled())
        .count();
    match count {
        0 => M2LocalLightCount::Zero,
        1 => M2LocalLightCount::One,
        2 => M2LocalLightCount::Two,
        3 => M2LocalLightCount::Three,
        4 => M2LocalLightCount::Four,
        _ => unreachable!("fixed ModelFFX light array exceeded four slots"),
    }
}
