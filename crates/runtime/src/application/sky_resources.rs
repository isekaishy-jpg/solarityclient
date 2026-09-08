//! Process-retained authored celestial texture requests.

mod skybox;

#[cfg(test)]
#[path = "../../tests/application/sky_resources.rs"]
mod tests;

use super::terrain_coordinator::m2_residency::ResidentM2Source;
use super::terrain_frame::RuntimeTerrainFrameError;
use super::terrain_frame::m2::sky::{SkyM2Model, sky_scene};
use solarity_asset::{
    AssetError, AssetPath, AssetStoreHandle, BlpTextureCache, BlpTextureSource, LightCatalog,
    LightSkybox, M2ModelCache,
};
use solarity_rendering::{
    BlpColorSpace, BlpTextureHandle, M2PreparedDraw, VulkanRenderer, WorldSkyModelBatch,
};
use std::collections::HashMap;
use std::sync::Arc;

pub(super) struct RuntimeSkyResources {
    sources: [Option<BlpTextureSource>; 3],
    textures: [Option<BlpTextureHandle>; 3],
    lighting: solarity_rendering::WorldCelestialLighting,
    stars: Option<SkyM2Model>,
    animations: Arc<solarity_asset::AnimationDataCatalog>,
    store: AssetStoreHandle,
    definitions: HashMap<u32, LightSkybox>,
    skyboxes: Vec<CachedSkybox>,
    model_cache: M2ModelCache,
    texture_cache: BlpTextureCache,
    bones: Vec<glam::Mat4>,
    skybox_draws: [Vec<M2PreparedDraw>; 3],
}

struct CachedSkybox {
    path: AssetPath,
    phase: skybox::SkyboxPhase,
    model: Option<SkyM2Model>,
}

#[derive(Clone, Copy)]
struct SkyModelInput {
    day: f32,
    realm_minute: i32,
    skyboxes: [(u32, f32); 3],
}

#[derive(Clone, Copy)]
pub(super) struct RuntimeCelestialResources {
    pub(super) textures: [BlpTextureHandle; 3],
    pub(super) colors: [u32; 3],
}

impl RuntimeSkyResources {
    pub(super) fn load(
        store: AssetStoreHandle,
        lights: &LightCatalog,
        animations: Arc<solarity_asset::AnimationDataCatalog>,
    ) -> Result<Self, AssetError> {
        let handle = store.clone();
        let mut assets = store.borrow_mut();
        let store = &mut *assets;
        let mut sources = [None, None, None];
        for (slot, path) in sources.iter_mut().zip([
            "Textures/sunCenter.blp",
            "Textures/moon.blp",
            "Textures/moon02.blp",
        ]) {
            let path = AssetPath::new(path)?;
            *slot = match BlpTextureSource::load(store, &path) {
                Ok(source) => Some(source),
                Err(error) => {
                    tracing::warn!(texture = %path, %error,
                        "celestial texture request failed; using stock green texture");
                    None
                }
            };
        }
        let path = AssetPath::new("Environments/Stars/stars.mdl")?;
        let stars = match ResidentM2Source::load(
            &path,
            &mut Default::default(),
            &mut Default::default(),
            store,
        ) {
            Ok(resident) => Some(SkyM2Model::new(resident, sdl3::timer::ticks() as u32)),
            Err(error) => {
                tracing::warn!(model = %path, %error, "stars model request failed");
                None
            }
        };
        Ok(Self {
            sources,
            stars,
            animations,
            textures: [None; 3],
            lighting: Default::default(),
            store: handle,
            definitions: lights
                .skyboxes()
                .map(|row| (row.id(), row.clone()))
                .collect(),
            skyboxes: Vec::new(),
            model_cache: M2ModelCache::new(),
            texture_cache: BlpTextureCache::new(),
            bones: Vec::new(),
            skybox_draws: std::array::from_fn(|_| Vec::new()),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_models(
        &mut self,
        renderer: &mut VulkanRenderer,
        camera: solarity_rendering::WorldCameraFrame,
        time_ms: u32,
        environment: super::environment_coordinator::RuntimeWorldEnvironmentFrame,
        world_bone_count: usize,
        random: &mut crate::random::CrtRand,
    ) -> Result<(bool, solarity_rendering::WorldSkyModelFrame<'_>), RuntimeTerrainFrameError> {
        self.prepare_model_input(
            renderer,
            camera,
            time_ms,
            SkyModelInput {
                day: environment.day_fraction(),
                realm_minute: environment.realm_minute(),
                skyboxes: environment
                    .light()
                    .skyboxes()
                    .map(|slot| (slot.id(), slot.weight())),
            },
            world_bone_count,
            random,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_model_input(
        &mut self,
        renderer: &mut VulkanRenderer,
        camera: solarity_rendering::WorldCameraFrame,
        time_ms: u32,
        input: SkyModelInput,
        world_bone_count: usize,
        random: &mut crate::random::CrtRand,
    ) -> Result<(bool, solarity_rendering::WorldSkyModelFrame<'_>), RuntimeTerrainFrameError> {
        let slots = skybox::select_slots::<RuntimeTerrainFrameError>(input.skyboxes, |id| {
            self.resolve_skybox(id, time_ms)
        })?;
        let default_sky = !slots.iter().any(|slot| {
            slot.flags == 0
                && slot.weight > 0.99
                && slot
                    .model
                    .and_then(|index| self.skyboxes.get(index))
                    .is_some_and(|entry| entry.model.is_some())
        });
        let alpha = if default_sky {
            solarity_rendering::world_stars_alpha(input.day)
        } else {
            0
        };
        let bone_offset = u32::try_from(world_bone_count)
            .map_err(|_| solarity_rendering::VulkanError::M2BoneTransformRange)?;
        if let Some(stars) = &mut self.stars {
            stars.prepare(
                renderer,
                camera,
                time_ms,
                if alpha > 1 {
                    f32::from(alpha) * (1. / 255.)
                } else {
                    0.
                },
                bone_offset,
                &self.animations,
                random,
            )?;
        }
        self.bones.clear();
        self.bones
            .extend_from_slice(self.stars.as_ref().map_or(&[], SkyM2Model::bones));
        for entry in &mut self.skyboxes {
            if let Some(model) = &mut entry.model {
                model.advance(time_ms, &self.animations, random)?;
            }
        }
        for slot in slots {
            if let Some(index) = slot.model {
                let entry = &mut self.skyboxes[index];
                if let Some(model) = &mut entry.model
                    && let Some((offset, speed)) =
                        entry
                            .phase
                            .update(true, model.primary_span(), input.realm_minute)
                {
                    model.synchronize_phase(offset, speed, time_ms, &self.animations, random)?;
                }
            }
        }
        let mut scenes = [sky_scene(camera); 3];
        for (slot_index, slot) in slots.into_iter().enumerate() {
            self.skybox_draws[slot_index].clear();
            if let Some(index) = slot.model
                && let Some(model) = &mut self.skyboxes[index].model
            {
                let offset = world_bone_count
                    .checked_add(self.bones.len())
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or(solarity_rendering::VulkanError::M2BoneTransformRange)?;
                model.prepare_current(renderer, camera, slot.weight, offset)?;
                self.bones.extend_from_slice(model.bones());
                self.skybox_draws[slot_index].extend_from_slice(model.draws());
                scenes[slot_index] = model.scene(camera);
            }
        }
        Ok((
            default_sky,
            solarity_rendering::WorldSkyModelFrame::new(
                sky_scene(camera),
                &self.bones,
                self.stars.as_ref().map_or(&[], SkyM2Model::draws),
                &[],
            )
            .with_skybox_batches(std::array::from_fn(|index| {
                WorldSkyModelBatch::new(scenes[index], &self.skybox_draws[index])
            })),
        ))
    }

    fn resolve_skybox(
        &mut self,
        id: u32,
        time_ms: u32,
    ) -> Result<Option<(Option<usize>, u32)>, RuntimeTerrainFrameError> {
        let Some(definition) = self.definitions.get(&id) else {
            return Ok(None);
        };
        let flags = definition.flags();
        if definition.model_path().is_empty() {
            return Ok(Some((None, flags)));
        }
        let path = match AssetPath::new(definition.model_path()) {
            Ok(path) => path,
            Err(error) => {
                tracing::warn!(model = definition.model_path(), %error, "invalid authored skybox path");
                return Ok(Some((None, flags)));
            }
        };
        if let Some(index) = self.skyboxes.iter().position(|entry| entry.path == path) {
            return Ok(Some((Some(index), flags)));
        }
        let mut store = self.store.borrow_mut();
        let resident = self
            .model_cache
            .load(&mut store, &path)
            .map_err(super::terrain_coordinator::RuntimeTerrainError::from)
            .and_then(|model| {
                let directional = model
                    .animations()
                    .lights()
                    .iter()
                    .any(|light| light.kind() == solarity_asset::M2LightKind::Directional);
                let points = model
                    .animations()
                    .lights()
                    .iter()
                    .filter(|light| light.kind() == solarity_asset::M2LightKind::Point)
                    .count();
                let count = match (usize::from(directional) + points).min(4) {
                    0 => solarity_rendering::M2LocalLightCount::Zero,
                    1 => solarity_rendering::M2LocalLightCount::One,
                    2 => solarity_rendering::M2LocalLightCount::Two,
                    3 => solarity_rendering::M2LocalLightCount::Three,
                    _ => solarity_rendering::M2LocalLightCount::Four,
                };
                ResidentM2Source::load_with_lights(
                    &path,
                    &mut self.model_cache,
                    &mut self.texture_cache,
                    &mut store,
                    count,
                )
                .map(|resident| SkyM2Model::with_lights(resident, time_ms, count))
            });
        let model = match resident {
            Ok(model) => Some(model),
            Err(error) => {
                tracing::warn!(model = %path, %error, "authored skybox request failed");
                None
            }
        };
        let index = self.skyboxes.len();
        self.skyboxes.push(CachedSkybox {
            path,
            model,
            phase: skybox::SkyboxPhase {
                flags,
                ..Default::default()
            },
        });
        Ok(Some((Some(index), flags)))
    }

    /// Uploads once; all world generations keep the renderer-owned handles.
    pub(super) fn prepare(
        &mut self,
        renderer: &mut VulkanRenderer,
        environment: super::environment_coordinator::RuntimeWorldEnvironmentFrame,
    ) -> Result<RuntimeCelestialResources, RuntimeTerrainFrameError> {
        for (slot, source) in self.textures.iter_mut().zip(&self.sources) {
            if slot.is_none() {
                *slot = Some(match source {
                    Some(source) => renderer.upload_blp_texture(source, BlpColorSpace::Linear)?,
                    None => renderer.upload_stock_m2_failure()?,
                });
            }
        }
        let [Some(sun), Some(moon), Some(second)] = self.textures else {
            return Err(solarity_rendering::VulkanError::WorldFrameCapacity.into());
        };
        self.lighting.update(
            environment.light().specular_color(),
            environment.weather_blend(),
        );
        Ok(RuntimeCelestialResources {
            textures: [sun, moon, second],
            colors: self.lighting.colors(),
        })
    }
}
