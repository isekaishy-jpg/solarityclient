//! Packed M2 vertex, texture-stage, and indexed-draw values.

use solarity_asset::{M2Batch, M2Material, M2Vertex};

/// Fixed 48-byte vertex payload consumed by the M2 graphics pipeline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2RenderVertex {
    position: [f32; 3],
    bone_weights: [u8; 4],
    bone_indices: [u8; 4],
    normal: [f32; 3],
    texture_coordinates: [[f32; 2]; 2],
}

impl M2RenderVertex {
    /// Size of one serialized vertex in the Vulkan vertex buffer.
    pub const BYTE_SIZE: usize = 48;

    /// Converts one decoder-independent model vertex without coordinate changes.
    pub(super) fn from_model(vertex: M2Vertex) -> Self {
        Self {
            position: vertex.position().to_array(),
            bone_weights: vertex.bone_weights(),
            bone_indices: vertex.bone_indices(),
            normal: vertex.normal().to_array(),
            texture_coordinates: vertex.texture_coordinates().map(|value| value.to_array()),
        }
    }

    /// Returns the untransformed stock model position.
    #[must_use]
    pub const fn position(self) -> [f32; 3] {
        self.position
    }

    /// Returns byte-normalized bone influence weights.
    #[must_use]
    pub const fn bone_weights(self) -> [u8; 4] {
        self.bone_weights
    }

    /// Returns model bone indices parallel to the influence weights.
    #[must_use]
    pub const fn bone_indices(self) -> [u8; 4] {
        self.bone_indices
    }

    /// Returns the untransformed stock model normal.
    #[must_use]
    pub const fn normal(self) -> [f32; 3] {
        self.normal
    }

    /// Returns both build-12340 texture-coordinate sets.
    #[must_use]
    pub const fn texture_coordinates(self) -> [[f32; 2]; 2] {
        self.texture_coordinates
    }

    /// Appends the explicit little-endian Vulkan vertex payload.
    pub(super) fn append_bytes(self, bytes: &mut Vec<u8>) {
        for value in self.position {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&self.bone_weights);
        bytes.extend_from_slice(&self.bone_indices);
        for value in self.normal {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        for coordinates in self.texture_coordinates {
            for value in coordinates {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
    }
}

/// One resolved texture declaration and coordinate set for a material stage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M2TextureBinding {
    stage: u16,
    texture_index: u16,
    texture_coordinate: u16,
}

impl M2TextureBinding {
    /// Creates one validated material-stage binding.
    pub(super) const fn new(stage: u16, texture_index: u16, texture_coordinate: u16) -> Self {
        Self {
            stage,
            texture_index,
            texture_coordinate,
        }
    }

    /// Returns the zero-based stage within this material batch.
    #[must_use]
    pub const fn stage(self) -> u16 {
        self.stage
    }

    /// Returns the selected model texture-declaration index.
    #[must_use]
    pub const fn texture_index(self) -> u16 {
        self.texture_index
    }

    /// Returns the stock texture-coordinate lookup value.
    #[must_use]
    pub const fn texture_coordinate(self) -> u16 {
        self.texture_coordinate
    }
}

/// One SKIN material batch translated to a direct indexed Vulkan draw range.
#[derive(Clone, Debug, PartialEq)]
pub struct M2DrawCall {
    geoset_id: u16,
    first_index: u32,
    index_count: u32,
    batch: M2Batch,
    material: M2Material,
    texture_bindings: Vec<M2TextureBinding>,
}

impl M2DrawCall {
    /// Creates one draw after every cross-array reference has been validated.
    pub(super) fn new(
        geoset_id: u16,
        first_index: u32,
        index_count: u32,
        batch: M2Batch,
        material: M2Material,
        texture_bindings: Vec<M2TextureBinding>,
    ) -> Self {
        Self {
            geoset_id,
            first_index,
            index_count,
            batch,
            material,
            texture_bindings,
        }
    }

    /// Returns the character geoset/submesh identifier controlling visibility.
    #[must_use]
    pub const fn geoset_id(&self) -> u16 {
        self.geoset_id
    }

    /// Returns the first entry in the resolved GPU index buffer.
    #[must_use]
    pub const fn first_index(&self) -> u32 {
        self.first_index
    }

    /// Returns the number of index entries submitted by this batch.
    #[must_use]
    pub const fn index_count(&self) -> u32 {
        self.index_count
    }

    /// Returns all exact SKIN batch selectors retained for pipeline preparation.
    #[must_use]
    pub const fn batch(&self) -> M2Batch {
        self.batch
    }

    /// Returns the resolved model render flags and blend operation.
    #[must_use]
    pub const fn material(&self) -> M2Material {
        self.material
    }

    /// Returns texture declarations and coordinate sets in material-stage order.
    #[must_use]
    pub fn texture_bindings(&self) -> &[M2TextureBinding] {
        &self.texture_bindings
    }
}
