//! Process-retained authored celestial texture requests.

mod skybox;
mod sources;

use solarity_cpu::CpuExecutor;
use sources::{CelestialRequest, ModelRequest};

#[cfg(test)]
#[path = "../../tests/application/sky_resources.rs"]
mod tests;

use super::terrain_coordinator::m2_residency::ResidentM2Source;
use super::terrain_frame::RuntimeTerrainFrameError;
use super::terrain_frame::m2::sky::{SkyM2Model, sky_scene};
use solarity_asset::{
    ArchiveCatalog, AssetError, AssetPath, BlpTextureCache, BlpTextureSource, LightCatalog,
    LightSkybox,
};
use solarity_rendering::{
    BlpColorSpace, BlpTextureHandle, M2PreparedDraw, VulkanRenderer, WorldSkyModelBatch,
};
use std::collections::HashMap;
use std::sync::Arc;

/// Process-retained celestial textures and shared authored sky model scenes.
pub(super) struct RuntimeSkyResources {
    sources: [Option<BlpTextureSource>; 5],
    textures: [Option<BlpTextureHandle>; 5],
    lighting: solarity_rendering::WorldCelestialLighting,
    stars: Option<SkyM2Model>,
    animations: Arc<solarity_asset::AnimationDataCatalog>,
    catalog: ArchiveCatalog,
    celestial_request: CelestialRequest,
    stars_request: ModelRequest,
    definitions: HashMap<u32, LightSkybox>,
    skyboxes: Vec<CachedSkybox>,
    /// Raw authored names resolve once; aliases share the canonical model cache.
    names: HashMap<String, Option<usize>>,
    bones: Vec<glam::Mat4>,
    skybox_draws: [Vec<M2PreparedDraw>; 4],
}

/// One canonical path owns its first phase flags even across name aliases.
struct CachedSkybox {
    path: AssetPath,
    phase: skybox::SkyboxPhase,
    model: Option<SkyM2Model>,
    request: ModelRequest,
}

#[derive(Clone, Copy)]
/// One palette request followed by WMO replacement and draw visibility.
struct SkyModelInput<'a> {
    day: f32,
    realm_minute: i32,
    skyboxes: [(u32, f32); 3],
    global_skybox: Option<(u32, f32)>,
    world_model: Option<(&'a str, f32)>,
    visible: bool,
}

#[derive(Clone, Copy)]
/// Uploaded celestial handles paired with the current packed light colors.
pub(super) struct RuntimeCelestialResources {
    pub(super) textures: [BlpTextureHandle; 3],
    pub(super) glare_textures: [BlpTextureHandle; 2],
    pub(super) colors: [u32; 3],
}

impl RuntimeSkyResources {
    /// Requests the stock celestial names and retains their process lifetime.
    pub(super) fn load(
        catalog: ArchiveCatalog,
        lights: &LightCatalog,
        animations: Arc<solarity_asset::AnimationDataCatalog>,
    ) -> Result<Self, AssetError> {
        Ok(Self {
            sources: Default::default(),
            stars: None,
            stars_request: ModelRequest::new(
                AssetPath::new("Environments/Stars/stars.mdl")?,
                sdl3::timer::ticks() as u32,
                false,
            ),
            celestial_request: CelestialRequest::Deferred,
            animations,
            textures: [None; 5],
            lighting: Default::default(),
            catalog,
            definitions: lights
                .skyboxes()
                .map(|row| (row.id(), row.clone()))
                .collect(),
            skyboxes: Vec::new(),
            names: HashMap::new(),
            bones: Vec::new(),
            skybox_draws: std::array::from_fn(|_| Vec::new()),
        })
    }

    #[allow(clippy::too_many_arguments)]
    /// Combines current environment slots with camera-root sky admission.
    pub(super) fn prepare_models(
        &mut self,
        cpu: &CpuExecutor,
        renderer: &mut VulkanRenderer,
        camera: solarity_rendering::WorldCameraFrame,
        time_ms: u32,
        environment: super::environment_coordinator::RuntimeWorldEnvironmentFrame,
        window: Option<solarity_rendering::WorldSkyWindow>,
        world_model: Option<&str>,
        world_bone_count: usize,
        random: &mut crate::random::CrtRand,
    ) -> Result<(bool, solarity_rendering::WorldSkyModelFrame<'_>), RuntimeTerrainFrameError> {
        self.prepare_model_input(
            cpu,
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
                global_skybox: environment.light().global_skybox().map(|id| (id, 1.)),
                world_model: world_model
                    .map(|path| (path, environment.world_model_skybox_weight())),
                visible: window.is_some()
                    && !environment.has_camera_liquid()
                    && environment.sky_enabled(),
            },
            world_bone_count,
            random,
        )
    }

    #[allow(clippy::too_many_arguments)]
    /// Resolves requests before the native gate, then advances the visible scene.
    fn prepare_model_input(
        &mut self,
        cpu: &CpuExecutor,
        renderer: &mut VulkanRenderer,
        camera: solarity_rendering::WorldCameraFrame,
        time_ms: u32,
        input: SkyModelInput<'_>,
        world_bone_count: usize,
        random: &mut crate::random::CrtRand,
    ) -> Result<(bool, solarity_rendering::WorldSkyModelFrame<'_>), RuntimeTerrainFrameError> {
        let (global, slots) = self.resolve_slots(input, time_ms)?;
        self.service_models(cpu)?;
        // 7EF6E0 checks owner presence and weight, independently of model readiness,
        // replacement flags and the window used to draw authored sky geometry.
        let glare_suppression = if global.model.is_some() && global.weight > 0. {
            global.weight
        } else {
            slots
                .iter()
                .filter(|slot| slot.model.is_some())
                .map(|slot| slot.weight)
                .fold(0.0_f32, f32::max)
        };
        // Palette requests and 7F31C0 precede the draw gate. Invisible scenes
        // retain resident owners, but do not advance animation or consume RNG.
        if !input.visible {
            return Ok((
                false,
                solarity_rendering::WorldSkyModelFrame::new(sky_scene(camera), &[], &[], &[])
                    .with_glare_suppression(glare_suppression),
            ));
        }
        let slots = [slots[0], slots[1], slots[2], global];
        let default_sky = skybox::default_sky(&slots, |index| self.skyboxes[index].model.is_some());
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
        let mut scenes = [sky_scene(camera); 4];
        for (slot_index, slot) in slots.into_iter().enumerate() {
            self.skybox_draws[slot_index].clear();
            // Readiness controls default-sky suppression separately. A resident
            // owner at full global weight suppresses ordinary model submissions.
            if !skybox::admits_slot(slot_index, global) {
                continue;
            }
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
            .with_glare_suppression(glare_suppression)
            .with_skybox_batches(std::array::from_fn(|index| {
                WorldSkyModelBatch::new(scenes[index], &self.skybox_draws[index])
            }))
            .with_global_skybox(WorldSkyModelBatch::new(scenes[3], &self.skybox_draws[3])),
        ))
    }

    /// Owner order and first-request flags are independent of worker readiness.
    fn resolve_slots(
        &mut self,
        input: SkyModelInput<'_>,
        time_ms: u32,
    ) -> Result<(skybox::SkyboxSlot, [skybox::SkyboxSlot; 3]), RuntimeTerrainFrameError> {
        // 7F3230 resolves the global override before its three ordinary requests.
        // An alias therefore inherits the flags of whichever request came first.
        let global = if let Some((id, weight)) = input.global_skybox {
            self.resolve_skybox(id, time_ms)?.map_or_else(
                skybox::SkyboxSlot::default,
                |(model, _)| skybox::SkyboxSlot {
                    model,
                    weight,
                    flags: 0,
                },
            )
        } else {
            skybox::SkyboxSlot::default()
        };
        let mut slots = skybox::select_slots::<RuntimeTerrainFrameError>(input.skyboxes, |id| {
            self.resolve_skybox(id, time_ms)
        })?;
        if let Some((path, weight)) = input.world_model {
            let model = self.resolve_name(path, 0, time_ms)?;
            skybox::replace_world_model(&mut slots, model, weight);
        }
        Ok((global, slots))
    }

    /// Resolves a LightSkybox row while preserving its slot and phase flags.
    fn resolve_skybox(
        &mut self,
        id: u32,
        time_ms: u32,
    ) -> Result<Option<(Option<usize>, u32)>, RuntimeTerrainFrameError> {
        let Some(definition) = self.definitions.get(&id) else {
            return Ok(None);
        };
        let flags = definition.flags();
        if let Some(&model) = self.names.get(definition.model_path()) {
            return Ok(Some((model, flags)));
        }
        // Only a new DBC name needs independent ownership while resolving it.
        let name = definition.model_path().to_owned();
        Ok(Some((self.resolve_name(&name, flags, time_ms)?, flags)))
    }

    /// 7F30C0 reuses models by path and preserves their first phase flags.
    fn resolve_name(
        &mut self,
        name: &str,
        flags: u32,
        time_ms: u32,
    ) -> Result<Option<usize>, RuntimeTerrainFrameError> {
        if let Some(&model) = self.names.get(name) {
            return Ok(model);
        }
        if name.is_empty() {
            self.names.insert(name.to_owned(), None);
            return Ok(None);
        }
        let path = match AssetPath::new(name) {
            Ok(path) => path,
            Err(error) => {
                tracing::warn!(model = name, %error, "invalid authored skybox path");
                self.names.insert(name.to_owned(), None);
                return Ok(None);
            }
        };
        if let Some(index) = self.skyboxes.iter().position(|entry| entry.path == path) {
            self.names.insert(name.to_owned(), Some(index));
            return Ok(Some(index));
        }
        let index = self.skyboxes.len();
        self.skyboxes.push(CachedSkybox {
            request: ModelRequest::new(path.clone(), time_ms, true),
            path,
            model: None,
            phase: skybox::SkyboxPhase {
                flags,
                ..Default::default()
            },
        });
        self.names.insert(name.to_owned(), Some(index));
        Ok(Some(index))
    }

    /// Uploads once; all world generations keep the renderer-owned handles.
    pub(super) fn prepare(
        &mut self,
        cpu: &CpuExecutor,
        renderer: &mut VulkanRenderer,
        environment: super::environment_coordinator::RuntimeWorldEnvironmentFrame,
    ) -> Result<RuntimeCelestialResources, RuntimeTerrainFrameError> {
        self.service_textures(cpu)?;
        for (slot, source) in self.textures.iter_mut().zip(&self.sources) {
            if slot.is_none() {
                *slot = Some(match source {
                    Some(source) => renderer.upload_blp_texture(source, BlpColorSpace::Linear)?,
                    None => renderer.upload_stock_m2_failure()?,
                });
            }
        }
        let [
            Some(sun),
            Some(moon),
            Some(second),
            Some(sun_glare),
            Some(moon_glare),
        ] = self.textures
        else {
            return Err(solarity_rendering::VulkanError::WorldFrameCapacity.into());
        };
        self.lighting.update(
            environment.light().specular_color(),
            environment.weather_blend(),
        );
        Ok(RuntimeCelestialResources {
            textures: [sun, moon, second],
            glare_textures: [sun_glare, moon_glare],
            colors: self.lighting.colors(),
        })
    }
}
