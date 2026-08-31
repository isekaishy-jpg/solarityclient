//! Packed M2 vertex, texture-stage, and indexed-draw values.

use solarity_asset::{M2Batch, M2Material, M2Submesh, M2Vertex};

/// Fixed 52-byte profile-local vertex payload consumed by the M2 pipeline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2RenderVertex {
    position: [f32; 3],
    bone_weights: [u8; 4],
    bone_indices: [u16; 4],
    normal: [f32; 3],
    texture_coordinates: [[f32; 2]; 2],
}

impl M2RenderVertex {
    /// Size of one serialized vertex in the Vulkan vertex buffer.
    pub const BYTE_SIZE: usize = 52;

    /// Converts one decoder-independent model vertex without coordinate changes.
    pub(super) fn from_profile(
        vertex: M2Vertex,
        bone_weights: [u8; 4],
        bone_indices: [u16; 4],
    ) -> Self {
        Self {
            position: vertex.position().to_array(),
            bone_weights,
            bone_indices,
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
    pub const fn bone_indices(self) -> [u16; 4] {
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
        for index in self.bone_indices {
            bytes.extend_from_slice(&index.to_le_bytes());
        }
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
    texture_coordinate: i16,
}

impl M2TextureBinding {
    /// Creates one validated material-stage binding.
    pub(super) const fn new(stage: u16, texture_index: u16, texture_coordinate: i16) -> Self {
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
    pub const fn texture_coordinate(self) -> i16 {
        self.texture_coordinate
    }
}

/// One SKIN material batch translated to a direct indexed Vulkan draw range.
#[derive(Clone, Debug, PartialEq)]
pub struct M2DrawCall {
    submesh: M2Submesh,
    first_index: u32,
    index_count: u32,
    batch: M2Batch,
    material: M2Material,
    transparent_sort_unit: bool,
    texture_bindings: Vec<M2TextureBinding>,
}

impl M2DrawCall {
    /// Creates one draw after every cross-array reference has been validated.
    pub(super) fn new(
        submesh: M2Submesh,
        first_index: u32,
        index_count: u32,
        batch: M2Batch,
        material: M2Material,
        transparent_sort_unit: bool,
        texture_bindings: Vec<M2TextureBinding>,
    ) -> Self {
        Self {
            submesh,
            first_index,
            index_count,
            batch,
            material,
            transparent_sort_unit,
            texture_bindings,
        }
    }

    /// Returns the character geoset/submesh identifier controlling visibility.
    #[must_use]
    pub const fn geoset_id(&self) -> u16 {
        self.submesh.id
    }

    /// Returns the number of bones available to this draw's local palette.
    #[must_use]
    pub const fn bone_count(&self) -> u16 {
        self.submesh.bone_count
    }

    /// Returns the first model bone-lookup entry in the local palette.
    #[must_use]
    pub const fn bone_start(&self) -> u16 {
        self.submesh.bone_start
    }

    /// Returns stock's vertex-shader bone-influence permutation selector.
    #[must_use]
    pub const fn bone_influence(&self) -> u16 {
        self.submesh.bone_influence
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

    /// Returns the model bone that animates this section's sorting sphere.
    #[must_use]
    pub const fn center_bone_index(&self) -> u16 {
        self.submesh.center_bone_index
    }

    /// Returns the authored center of this section's sorting sphere.
    #[must_use]
    pub const fn sort_center(&self) -> glam::Vec3 {
        self.submesh.sort_center
    }

    /// Returns the authored radius of this section's sorting sphere.
    #[must_use]
    pub const fn sort_radius(&self) -> f32 {
        self.submesh.bounding_radius
    }

    /// Reports whether the base layer routes this whole material unit through
    /// stock's transparent scene pass.
    #[must_use]
    pub const fn transparent_sort_unit(&self) -> bool {
        self.transparent_sort_unit
    }

    /// Returns texture declarations and coordinate sets in material-stage order.
    #[must_use]
    pub fn texture_bindings(&self) -> &[M2TextureBinding] {
        &self.texture_bindings
    }
}
