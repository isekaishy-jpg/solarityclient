//! Immutable draw data safe to consume during Vulkan command recording.

use crate::device::{M2MeshHandle, M2PipelineHandle, M2TextureSetHandle};
use crate::model::{M2DrawPushConstants, M2MaterialUniform};

/// One fully validated M2 indexed draw and its per-draw GPU state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2PreparedDraw {
    mesh: M2MeshHandle,
    pipeline: M2PipelineHandle,
    texture_set: M2TextureSetHandle,
    first_index: u32,
    index_count: u32,
    material: M2MaterialUniform,
    push_constants: M2DrawPushConstants,
    required_bone_transforms: usize,
}

impl M2PreparedDraw {
    /// Constructs a packet after the renderer validates every resource join.
    #[allow(clippy::too_many_arguments)]
    pub(super) const fn new(
        mesh: M2MeshHandle,
        pipeline: M2PipelineHandle,
        texture_set: M2TextureSetHandle,
        first_index: u32,
        index_count: u32,
        material: M2MaterialUniform,
        push_constants: M2DrawPushConstants,
        required_bone_transforms: usize,
    ) -> Self {
        Self {
            mesh,
            pipeline,
            texture_set,
            first_index,
            index_count,
            material,
            push_constants,
            required_bone_transforms,
        }
    }

    /// Returns the device-local vertex/index allocation identity.
    #[must_use]
    pub const fn mesh(self) -> M2MeshHandle {
        self.mesh
    }

    /// Returns the exact compiled effect/permutation identity.
    #[must_use]
    pub const fn pipeline(self) -> M2PipelineHandle {
        self.pipeline
    }

    /// Returns the persistent sampled-texture descriptor identity.
    #[must_use]
    pub const fn texture_set(self) -> M2TextureSetHandle {
        self.texture_set
    }

    /// Returns the first unsigned-short index in the uploaded profile.
    #[must_use]
    pub const fn first_index(self) -> u32 {
        self.first_index
    }

    /// Returns the number of indices submitted for this material batch.
    #[must_use]
    pub const fn index_count(self) -> u32 {
        self.index_count
    }

    /// Returns the exact std140 per-draw material block.
    #[must_use]
    pub const fn material(self) -> M2MaterialUniform {
        self.material
    }

    /// Returns the exact 16-byte per-draw push block.
    #[must_use]
    pub const fn push_constants(self) -> M2DrawPushConstants {
        self.push_constants
    }

    /// Returns the minimum global transform count needed by this mesh instance.
    #[must_use]
    pub const fn required_bone_transforms(self) -> usize {
        self.required_bone_transforms
    }
}
