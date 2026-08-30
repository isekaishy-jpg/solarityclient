//! Public typed M2 resource identity and immutable allocation diagnostics.

use solarity_asset::AssetPath;

/// Stable index into one [`crate::VulkanRenderer`]'s M2 resource registry.
///
/// Handles are renderer-local. Passing one to another renderer simply produces
/// no matching resource information rather than exposing raw Vulkan handles.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct M2MeshHandle {
    pub(super) registry_id: u64,
    pub(super) slot: u32,
}

/// Observable geometry facts retained beside one device-local M2 allocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M2MeshResourceInfo {
    path: AssetPath,
    profile_index: usize,
    vertex_count: usize,
    index_count: usize,
    vertex_byte_count: usize,
    index_byte_count: usize,
    max_bone_index: Option<u16>,
}

impl M2MeshResourceInfo {
    /// Captures the exact logical payload represented by one resource.
    pub(super) const fn new(
        path: AssetPath,
        profile_index: usize,
        vertex_count: usize,
        index_count: usize,
        vertex_byte_count: usize,
        index_byte_count: usize,
        max_bone_index: Option<u16>,
    ) -> Self {
        Self {
            path,
            profile_index,
            vertex_count,
            index_count,
            vertex_byte_count,
            index_byte_count,
            max_bone_index,
        }
    }

    /// Returns the normalized archive path used for resource deduplication.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns the explicitly selected external SKIN profile.
    #[must_use]
    pub const fn profile_index(&self) -> usize {
        self.profile_index
    }

    /// Returns the number of packed vertices available to draw.
    #[must_use]
    pub const fn vertex_count(&self) -> usize {
        self.vertex_count
    }

    /// Returns the number of direct unsigned-short indices available to draw.
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

    /// Returns the greatest model-bone index consumed by any weighted vertex.
    #[must_use]
    pub const fn max_bone_index(&self) -> Option<u16> {
        self.max_bone_index
    }
}
