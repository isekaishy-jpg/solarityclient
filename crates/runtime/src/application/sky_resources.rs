//! Process-retained authored celestial texture requests.

use super::terrain_coordinator::m2_residency::ResidentM2Source;
use super::terrain_frame::RuntimeTerrainFrameError;
use super::terrain_frame::m2::sky::{SkyM2Model, sky_scene};
use solarity_asset::{AssetError, AssetPath, AssetStore, BlpTextureSource};
use solarity_rendering::{BlpColorSpace, BlpTextureHandle, VulkanRenderer};
use std::sync::Arc;

pub(super) struct RuntimeSkyResources {
    sources: [Option<BlpTextureSource>; 3],
    textures: [Option<BlpTextureHandle>; 3],
    lighting: solarity_rendering::WorldCelestialLighting,
    stars: Option<SkyM2Model>,
    animations: Arc<solarity_asset::AnimationDataCatalog>,
}

#[derive(Clone, Copy)]
pub(super) struct RuntimeCelestialResources {
    pub(super) textures: [BlpTextureHandle; 3],
    pub(super) colors: [u32; 3],
}

impl RuntimeSkyResources {
    pub(super) fn load(
        store: &mut AssetStore,
        animations: Arc<solarity_asset::AnimationDataCatalog>,
    ) -> Result<Self, AssetError> {
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
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_models(
        &mut self,
        renderer: &mut VulkanRenderer,
        camera: solarity_rendering::WorldCameraFrame,
        time_ms: u32,
        day: f32,
        world_bone_count: usize,
        random: &mut crate::random::CrtRand,
    ) -> Result<solarity_rendering::WorldSkyModelFrame<'_>, RuntimeTerrainFrameError> {
        let alpha = solarity_rendering::world_stars_alpha(day);
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
        Ok(solarity_rendering::WorldSkyModelFrame::new(
            sky_scene(camera),
            self.stars.as_ref().map_or(&[], SkyM2Model::bones),
            self.stars.as_ref().map_or(&[], SkyM2Model::draws),
            &[],
        ))
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
