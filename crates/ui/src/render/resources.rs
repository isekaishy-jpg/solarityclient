//! Archive texture requests and batch-to-resource joins for live UI geometry.

use std::collections::HashMap;
use std::sync::Arc;

use solarity_asset::{AssetPath, AssetStore, BlpTextureCache, BlpTextureSource};
use solarity_rendering::{UiMeshPlan, UiRenderSource, UiTextureResidency};

use crate::UiRenderError;

/// One unique archive BLP requested by the current UI presentation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiTextureAssetRequest {
    path: AssetPath,
    residency: UiTextureResidency,
}

impl UiTextureAssetRequest {
    /// Returns the canonical path resolved through ordinary MPQ precedence.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns whether initial presentation must wait for this source.
    #[must_use]
    pub const fn residency(&self) -> UiTextureResidency {
        self.residency
    }
}

/// Unique texture requests plus a direct resource index for every draw batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiTextureAssetPlan {
    requests: Vec<UiTextureAssetRequest>,
    batch_requests: Vec<Option<u32>>,
}

impl UiTextureAssetPlan {
    /// Deduplicates image identity while retaining per-batch sampler state.
    ///
    /// A path requested both ways becomes blocking because at least one stock
    /// region cannot be presented pending. Vertex-color batches have no image
    /// request and retain `None` in the parallel batch map.
    ///
    /// # Errors
    ///
    /// Returns [`UiRenderError::TextureRequestCapacity`] if the unique path
    /// count cannot fit the renderer's unsigned 32-bit resource index.
    pub fn prepare(mesh: &UiMeshPlan) -> Result<Self, UiRenderError> {
        let mut requests = Vec::<UiTextureAssetRequest>::new();
        let mut indices = HashMap::<AssetPath, u32>::new();
        let mut batch_requests = Vec::with_capacity(mesh.batches().len());
        for batch in mesh.batches() {
            let UiRenderSource::Texture(path) = batch.source() else {
                batch_requests.push(None);
                continue;
            };
            let request_index = if let Some(index) = indices.get(path).copied() {
                if batch.residency() == UiTextureResidency::Blocking {
                    requests[index as usize].residency = UiTextureResidency::Blocking;
                }
                index
            } else {
                let index = u32::try_from(requests.len())
                    .map_err(|_source| UiRenderError::TextureRequestCapacity)?;
                requests.push(UiTextureAssetRequest {
                    path: path.clone(),
                    residency: batch.residency(),
                });
                indices.insert(path.clone(), index);
                index
            };
            batch_requests.push(Some(request_index));
        }
        Ok(Self {
            requests,
            batch_requests,
        })
    }

    /// Returns unique archive requests in first-presentation order.
    #[must_use]
    pub fn requests(&self) -> &[UiTextureAssetRequest] {
        &self.requests
    }

    /// Returns the image request used by one material batch, if it samples one.
    #[must_use]
    pub fn request_for_batch(&self, batch_index: usize) -> Option<&UiTextureAssetRequest> {
        let request = self.batch_requests.get(batch_index).copied().flatten()?;
        self.requests.get(request as usize)
    }

    /// Loads only sources whose stock residency contract blocks presentation.
    ///
    /// Non-blocking entries remain explicitly unresolved. The later streaming
    /// owner must load and attach those entries; this boundary never replaces a
    /// missing source with an invented texture.
    ///
    /// # Errors
    ///
    /// Returns [`UiRenderError::Asset`] when a blocking MPQ entry is missing,
    /// unreadable, or not a supported stock BLP.
    pub fn load_blocking(
        &self,
        store: &mut AssetStore,
        cache: &mut BlpTextureCache,
    ) -> Result<UiTextureAssetBindings, UiRenderError> {
        let mut sources = Vec::with_capacity(self.requests.len());
        for request in &self.requests {
            let source = match request.residency {
                UiTextureResidency::Blocking => Some(cache.load(store, &request.path)?),
                UiTextureResidency::NonBlocking => None,
            };
            sources.push(source);
        }
        Ok(UiTextureAssetBindings { sources })
    }
}

/// Parsed blocking BLP sources parallel to a [`UiTextureAssetPlan`].
pub struct UiTextureAssetBindings {
    sources: Vec<Option<Arc<BlpTextureSource>>>,
}

impl UiTextureAssetBindings {
    /// Returns a parsed source when its request has reached resident CPU state.
    #[must_use]
    pub fn source(&self, request_index: usize) -> Option<&Arc<BlpTextureSource>> {
        self.sources.get(request_index)?.as_ref()
    }

    /// Returns the number of blocking sources ready for Vulkan upload.
    #[must_use]
    pub fn resident_count(&self) -> usize {
        self.sources.iter().flatten().count()
    }

    /// Returns the number of non-blocking sources still awaiting streaming.
    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.sources
            .iter()
            .filter(|source| source.is_none())
            .count()
    }
}
