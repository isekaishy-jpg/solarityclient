//! A resident source/frame lease; submitted GPU work has a separate fence owner.

use crate::device::{
    CharacterAtlasTextureHandle, M2MeshHandle, UiGlyphTextureHandle, UiMeshHandle,
};
use std::sync::{Arc, mpsc};

/// Closed domains whose dynamic ownership can end before renderer teardown.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum ResourceKey {
    CharacterAtlas(CharacterAtlasTextureHandle),
    Glyph(UiGlyphTextureHandle),
    UiMesh(UiMeshHandle),
    M2Mesh(M2MeshHandle),
}

/// Shared last-owner notification payload. Clones share the same retirement.
#[derive(Debug)]
pub(super) struct LeaseState {
    pub(super) key: ResourceKey,
    pub(super) sender: mpsc::Sender<ResourceKey>,
}

impl Drop for LeaseState {
    fn drop(&mut self) {
        // Receiver absence means the renderer already tore down its resources.
        let _renderer_gone = self.sender.send(self.key);
    }
}

/// Pins one renderer resource for future CPU submissions. Clone at resident
/// source/frame publication, never per draw or per frame. Dropping the final
/// lease queues retirement after all earlier GPU submissions complete.
#[derive(Debug)]
pub struct GpuResourceLease(pub(super) Arc<LeaseState>);

impl Clone for GpuResourceLease {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}
