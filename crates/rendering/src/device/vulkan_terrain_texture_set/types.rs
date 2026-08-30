//! Public terrain sampled-resource identities.

use crate::device::{BlpTextureHandle, TerrainMaterialHandle};
use crate::{TerrainLayerCount, TerrainLayerCountError};

/// One material atlas and the exact ordered MCLY diffuse images for a chunk.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TerrainTextureSet {
    material: TerrainMaterialHandle,
    layers: Vec<BlpTextureHandle>,
    layer_count: TerrainLayerCount,
}

impl TerrainTextureSet {
    /// Captures a strict stock one-through-four layer descriptor request.
    ///
    /// # Errors
    ///
    /// Returns [`TerrainLayerCountError`] when the layer slice is empty or
    /// exceeds the four MCLY entries accepted by build 12340.
    pub fn new(
        material: TerrainMaterialHandle,
        layers: &[BlpTextureHandle],
    ) -> Result<Self, TerrainLayerCountError> {
        let layer_count = TerrainLayerCount::try_from(layers.len())?;
        Ok(Self {
            material,
            layers: layers.to_vec(),
            layer_count,
        })
    }

    /// Returns the shared blend/shadow atlas identity.
    #[must_use]
    pub const fn material(&self) -> TerrainMaterialHandle {
        self.material
    }

    /// Returns diffuse images in exact MCLY order.
    #[must_use]
    pub fn layers(&self) -> &[BlpTextureHandle] {
        &self.layers
    }

    /// Returns the matching statically consumed shader count.
    #[must_use]
    pub const fn layer_count(&self) -> TerrainLayerCount {
        self.layer_count
    }
}

/// Stable index into one renderer's terrain texture-set registry.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TerrainTextureSetHandle {
    pub(super) registry_id: u64,
    pub(super) slot: u32,
}

/// Immutable diagnostics for one live terrain material descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerrainTextureSetInfo {
    layer_count: TerrainLayerCount,
}

impl TerrainTextureSetInfo {
    pub(super) const fn new(layer_count: TerrainLayerCount) -> Self {
        Self { layer_count }
    }

    /// Returns the number of bound diffuse images.
    #[must_use]
    pub const fn layer_count(self) -> TerrainLayerCount {
        self.layer_count
    }
}
