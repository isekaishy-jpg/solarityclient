//! Process-retained authored celestial texture requests.

use super::terrain_frame::RuntimeTerrainFrameError;
use solarity_asset::{AssetError, AssetPath, AssetStore, BlpTextureSource};
use solarity_rendering::{BlpColorSpace, BlpTextureHandle, VulkanRenderer};

pub(super) struct RuntimeSkyResources {
    sources: [Option<BlpTextureSource>; 3],
    textures: [Option<BlpTextureHandle>; 3],
    lighting: solarity_rendering::WorldCelestialLighting,
}

#[derive(Clone, Copy)]
pub(super) struct RuntimeCelestialResources {
    pub(super) textures: [BlpTextureHandle; 3],
    pub(super) colors: [u32; 3],
}

impl RuntimeSkyResources {
    pub(super) fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
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
        Ok(Self {
            sources,
            textures: [None; 3],
            lighting: Default::default(),
        })
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
