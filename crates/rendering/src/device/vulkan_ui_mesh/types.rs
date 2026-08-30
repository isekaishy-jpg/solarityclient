//! Public UI geometry identity and immutable allocation diagnostics.

/// Stable index into one renderer's UI mesh registry.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UiMeshHandle {
    pub(super) registry_id: u64,
    pub(super) slot: u32,
}

/// Observable payload facts retained beside one device-local allocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiMeshResourceInfo {
    plan_identity: u64,
    vertex_count: usize,
    index_count: usize,
    vertex_byte_count: usize,
    index_byte_count: usize,
}

impl UiMeshResourceInfo {
    /// Captures the exact logical payload represented by one allocation.
    pub(super) const fn new(
        plan_identity: u64,
        vertex_count: usize,
        index_count: usize,
        vertex_byte_count: usize,
        index_byte_count: usize,
    ) -> Self {
        Self {
            plan_identity,
            vertex_count,
            index_count,
            vertex_byte_count,
            index_byte_count,
        }
    }

    pub(in crate::device) const fn plan_identity(self) -> u64 {
        self.plan_identity
    }

    /// Returns the number of fixed-layout UI vertices available to draw.
    #[must_use]
    pub const fn vertex_count(self) -> usize {
        self.vertex_count
    }

    /// Returns the number of direct unsigned 32-bit indices available.
    #[must_use]
    pub const fn index_count(self) -> usize {
        self.index_count
    }

    /// Returns the unpadded serialized vertex payload size.
    #[must_use]
    pub const fn vertex_byte_count(self) -> usize {
        self.vertex_byte_count
    }

    /// Returns the unpadded serialized index payload size.
    #[must_use]
    pub const fn index_byte_count(self) -> usize {
        self.index_byte_count
    }
}
