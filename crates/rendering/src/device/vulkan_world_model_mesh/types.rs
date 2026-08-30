//! Public typed identity and allocation diagnostics for one WMO mesh.

use solarity_asset::AssetPath;

/// Stable index into one renderer's WMO geometry registry.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WorldModelMeshHandle {
    pub(super) registry_id: u64,
    pub(super) slot: u32,
}

/// Observable facts retained beside one device-local WMO allocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldModelMeshResourceInfo {
    path: AssetPath,
    vertex_count: usize,
    index_count: usize,
    vertex_byte_count: usize,
    index_byte_count: usize,
}

impl WorldModelMeshResourceInfo {
    pub(super) const fn new(
        path: AssetPath,
        vertex_count: usize,
        index_count: usize,
        vertex_byte_count: usize,
        index_byte_count: usize,
    ) -> Self {
        Self {
            path,
            vertex_count,
            index_count,
            vertex_byte_count,
            index_byte_count,
        }
    }

    /// Returns the canonical root-WMO resource identity.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns the combined MOVT vertex count.
    #[must_use]
    pub const fn vertex_count(&self) -> usize {
        self.vertex_count
    }

    /// Returns the combined direct `u32` index count.
    #[must_use]
    pub const fn index_count(&self) -> usize {
        self.index_count
    }

    /// Returns the unpadded serialized vertex payload size.
    #[must_use]
    pub const fn vertex_byte_count(&self) -> usize {
        self.vertex_byte_count
    }

    /// Returns the unpadded serialized index payload size.
    #[must_use]
    pub const fn index_byte_count(&self) -> usize {
        self.index_byte_count
    }
}
