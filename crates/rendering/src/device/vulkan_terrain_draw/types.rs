//! Immutable terrain draw data safe for Vulkan command recording.

use crate::device::{TerrainMeshHandle, TerrainPipelineHandle, TerrainTextureSetHandle};

/// One fully validated camera-selected MCNK indexed draw.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerrainPreparedDraw {
    mesh: TerrainMeshHandle,
    pipeline: TerrainPipelineHandle,
    texture_set: TerrainTextureSetHandle,
    first_index: u32,
    index_count: u32,
    atlas_chunk: [u8; 2],
    material_flags: [u32; 2],
}

impl TerrainPreparedDraw {
    pub(super) const fn new(
        mesh: TerrainMeshHandle,
        pipeline: TerrainPipelineHandle,
        texture_set: TerrainTextureSetHandle,
        first_index: u32,
        index_count: u32,
        atlas_chunk: [u8; 2],
        material_flags: [u32; 2],
    ) -> Self {
        Self {
            mesh,
            pipeline,
            texture_set,
            first_index,
            index_count,
            atlas_chunk,
            material_flags,
        }
    }

    /// Returns the shared resident ADT geometry identity.
    #[must_use]
    pub const fn mesh(self) -> TerrainMeshHandle {
        self.mesh
    }

    /// Returns the exact authored layer-count pipeline identity.
    #[must_use]
    pub const fn pipeline(self) -> TerrainPipelineHandle {
        self.pipeline
    }

    /// Returns the matching atlas and ordered diffuse descriptor identity.
    #[must_use]
    pub const fn texture_set(self) -> TerrainTextureSetHandle {
        self.texture_set
    }

    /// Returns the first unsigned-short index in the combined tile buffer.
    #[must_use]
    pub const fn first_index(self) -> u32 {
        self.first_index
    }

    /// Returns the exact non-hole index count for this MCNK.
    #[must_use]
    pub const fn index_count(self) -> u32 {
        self.index_count
    }

    /// Serializes atlas coordinates, weighted blending, and the unlit layer mask.
    #[must_use]
    pub const fn push_bytes(self) -> [u8; 16] {
        let x = (self.atlas_chunk[0] as u32).to_le_bytes();
        let y = (self.atlas_chunk[1] as u32).to_le_bytes();
        let blend = self.material_flags[0].to_le_bytes();
        let unlit = self.material_flags[1].to_le_bytes();
        [
            x[0], x[1], x[2], x[3], y[0], y[1], y[2], y[3], blend[0], blend[1], blend[2], blend[3],
            unlit[0], unlit[1], unlit[2], unlit[3],
        ]
    }
}
