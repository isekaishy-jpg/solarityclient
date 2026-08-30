//! Immutable UI draw data safe to consume during command recording.

use crate::device::{UiMeshHandle, UiPipelineHandle, UiTextureSetHandle};

/// One fully validated indexed UI material batch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiPreparedDraw {
    mesh: UiMeshHandle,
    pipeline: UiPipelineHandle,
    texture_set: Option<UiTextureSetHandle>,
    first_index: u32,
    index_count: u32,
}

impl UiPreparedDraw {
    /// Constructs a packet after every renderer-local resource join is validated.
    pub(super) const fn new(
        mesh: UiMeshHandle,
        pipeline: UiPipelineHandle,
        texture_set: Option<UiTextureSetHandle>,
        first_index: u32,
        index_count: u32,
    ) -> Self {
        Self {
            mesh,
            pipeline,
            texture_set,
            first_index,
            index_count,
        }
    }

    /// Returns the device-local UI geometry allocation identity.
    #[must_use]
    pub const fn mesh(self) -> UiMeshHandle {
        self.mesh
    }

    /// Returns the exact source/blend pipeline identity.
    #[must_use]
    pub const fn pipeline(self) -> UiPipelineHandle {
        self.pipeline
    }

    /// Returns the sampled-image set required only by texture-backed batches.
    #[must_use]
    pub const fn texture_set(self) -> Option<UiTextureSetHandle> {
        self.texture_set
    }

    /// Returns the first unsigned 32-bit index submitted by this batch.
    #[must_use]
    pub const fn first_index(self) -> u32 {
        self.first_index
    }

    /// Returns the number of sequential indices submitted by this batch.
    #[must_use]
    pub const fn index_count(self) -> u32 {
        self.index_count
    }
}
