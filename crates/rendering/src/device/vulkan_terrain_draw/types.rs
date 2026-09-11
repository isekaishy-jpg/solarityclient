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
    material_flags: [u32; 3],
    point_light_words: [u32; 27],
}

impl TerrainPreparedDraw {
    /// Complete draw block, within Vulkan's guaranteed 128-byte push capacity.
    pub const PUSH_BYTE_SIZE: usize = 128;

    pub(super) const fn new(
        mesh: TerrainMeshHandle,
        pipeline: TerrainPipelineHandle,
        texture_set: TerrainTextureSetHandle,
        first_index: u32,
        index_count: u32,
        atlas_chunk: [u8; 2],
        material_flags: [u32; 3],
    ) -> Self {
        Self {
            mesh,
            pipeline,
            texture_set,
            first_index,
            index_count,
            atlas_chunk,
            material_flags,
            point_light_words: [0; 27],
        }
    }

    /// Captures this frame's selected terrain point lights without changing mesh
    /// or material ownership. An unmodified resident draw has no point lights.
    #[must_use]
    pub fn with_point_lights(mut self, lights: [crate::TerrainPointLight; 3]) -> Self {
        for (words, light) in self
            .point_light_words
            .as_chunks_mut::<9>()
            .0
            .iter_mut()
            .zip(lights)
        {
            words.copy_from_slice(&light.words());
        }
        self
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

    /// Serializes atlas coordinates, lighting flags, and packed layer animation.
    #[must_use]
    pub const fn push_bytes(self) -> [u8; Self::PUSH_BYTE_SIZE] {
        let x = (self.atlas_chunk[0] as u32).to_le_bytes();
        let y = (self.atlas_chunk[1] as u32).to_le_bytes();
        let blend = self.material_flags[0].to_le_bytes();
        let unlit = self.material_flags[1].to_le_bytes();
        let animation = self.material_flags[2].to_le_bytes();
        let material = [
            x[0],
            x[1],
            x[2],
            x[3],
            y[0],
            y[1],
            y[2],
            y[3],
            blend[0],
            blend[1],
            blend[2],
            blend[3],
            unlit[0],
            unlit[1],
            unlit[2],
            unlit[3],
            animation[0],
            animation[1],
            animation[2],
            animation[3],
        ];
        let mut bytes = [0; Self::PUSH_BYTE_SIZE];
        let mut index = 0;
        while index < material.len() {
            bytes[index] = material[index];
            index += 1;
        }
        let mut word = 0;
        while word < self.point_light_words.len() {
            let value = self.point_light_words[word].to_le_bytes();
            let offset = material.len() + word * 4;
            bytes[offset] = value[0];
            bytes[offset + 1] = value[1];
            bytes[offset + 2] = value[2];
            bytes[offset + 3] = value[3];
            word += 1;
        }
        bytes
    }
}
