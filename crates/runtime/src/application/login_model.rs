//! Retained stock M2 scene inserted beneath pre-world Glue presentation.

use std::sync::Arc;

use glam::{Vec3, Vec4};
use solarity_asset::{
    AssetError, AssetPath, AssetStoreHandle, BlpTextureCache, M2ModelCache, M2TextureKind,
};
use solarity_rendering::{
    M2CameraFrameError, M2LocalLightState, M2ParticleTwinkleTable, M2SceneUniform,
    TerrainSceneUniform, VulkanError, VulkanRenderer, WorldFrameScene, WorldFrustum,
    WorldModelSceneUniform, WorldScreenWindow, sample_m2_camera_frame,
};
use solarity_ui::{GlueManager, UiModelLight, UiModelPresentation};
use thiserror::Error;

use crate::application::login_ui::LoginUiFrame;
use crate::application::terrain_frame::RuntimeTerrainFrameError;
use crate::application::terrain_frame::m2::M2Frame;
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

#[derive(Clone, Copy, Debug, PartialEq)]
struct GlueModelEnvironment {
    ambient: Vec3,
    diffuse: Vec3,
    light_direction: Vec3,
    fog_color: Vec3,
    fog_range: Vec4,
    local_lights: [M2LocalLightState; 4],
}

impl GlueModelEnvironment {
    fn from_presentation(model: &UiModelPresentation) -> Self {
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
        let authored_lights = model.background_lights().live();
        let has_authored_lights = authored_lights.iter().any(Option::is_some);
        let local_lights = authored_lights
            .map(|light| light.map_or_else(M2LocalLightState::disabled, model_light_state));
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
        }
    }
}

fn model_light_state(light: UiModelLight) -> M2LocalLightState {
    M2LocalLightState::directional(
        Vec3::from_array(light.direction()),
        Vec3::from_array(light.ambient()),
        Vec3::from_array(light.diffuse()),
    )
}

/// Process-long model and texture caches plus the current Glue M2 generation.
pub(crate) struct RuntimeGlueModelScene {
    models: M2ModelCache,
    textures: BlpTextureCache,
    active: Option<ActiveGlueModel>,
}

impl RuntimeGlueModelScene {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            models: M2ModelCache::new(),
            textures: BlpTextureCache::new(),
            active: None,
        }
    }

    /// Rebuilds GPU state only when Glue changes the selected model generation.
    pub(crate) fn synchronize(
        &mut self,
        renderer: &mut VulkanRenderer,
        glue: &GlueManager,
        assets: &AssetStoreHandle,
        random: &mut CrtRand,
        particle_twinkle: Arc<M2ParticleTwinkleTable>,
    ) -> Result<(), RuntimeGlueModelError> {
        let visible = glue.presentation().models();
        if visible.is_empty() {
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
        let environment = GlueModelEnvironment::from_presentation(presentation);
        if let Some(active) = self.active.as_mut()
            && active.key == key
        {
            active.environment = environment;
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
            let path =
                texture
                    .filename()
                    .ok_or_else(|| RuntimeGlueModelError::MissingTexturePath {
                        model: model.path().clone(),
                        texture_index,
                    })?;
            texture_sources.push(self.textures.load(&mut store, path)?);
        }
        drop(store);
        let frame = M2Frame::prepare_glue_model(
            renderer,
            Arc::clone(&model),
            &texture_sources,
            key.object_index,
            animation_id,
            key.model_scale,
            random,
            particle_twinkle,
        )?;
        self.active = Some(ActiveGlueModel {
            key,
            environment,
            model,
            frame,
        });
        Ok(())
    }

    /// Presents the authored model/effects and then the loaded FrameXML pass.
    pub(crate) fn present(
        &mut self,
        renderer: &mut VulkanRenderer,
        ui: &LoginUiFrame,
        pixel_extent: (u32, u32),
        global_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<bool, RuntimeGlueModelError> {
        let Some(active) = self.active.as_mut() else {
            return Ok(false);
        };
        let aspect_ratio = pixel_extent.0 as f32 / pixel_extent.1 as f32;
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
        renderer.present_world_frame_with_ui(
            WorldFrameScene::new(terrain, world_model, model),
            visible.bone_transforms,
            &[],
            &[],
            visible.draws,
            visible.particle_vertices,
            visible.particle_indices,
            visible.particle_draws,
            visible.ribbon_vertices,
            visible.ribbon_draws,
            ui.logical_extent(),
            ui.draws(),
        )?;
        Ok(true)
    }
}
