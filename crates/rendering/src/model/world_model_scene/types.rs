//! Packed WMO vertices, draw ranges, and group ranges.

use solarity_asset::WorldModelBatchClass;

/// Fixed 72-byte MapObj vertex payload consumed by the future WMO pipeline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldModelRenderVertex {
    position: [f32; 3],
    normal: [f32; 3],
    texture_coordinates: [[f32; 2]; 2],
    color: [f32; 4],
    blend_color: [f32; 4],
}

impl WorldModelRenderVertex {
    /// Size of one explicitly serialized Vulkan vertex.
    pub const BYTE_SIZE: usize = 72;

    pub(super) const fn new(
        position: [f32; 3],
        normal: [f32; 3],
        texture_coordinates: [[f32; 2]; 2],
        color: [f32; 4],
        blend_color: [f32; 4],
    ) -> Self {
        Self {
            position,
            normal,
            texture_coordinates,
            color,
            blend_color,
        }
    }

    /// Returns the root-local MOVT position.
    #[must_use]
    pub const fn position(self) -> [f32; 3] {
        self.position
    }

    /// Returns the root-local MONR direction.
    #[must_use]
    pub const fn normal(self) -> [f32; 3] {
        self.normal
    }

    /// Returns MapObj's primary and secondary texture coordinates.
    #[must_use]
    pub const fn texture_coordinates(self) -> [[f32; 2]; 2] {
        self.texture_coordinates
    }

    /// Returns the fixed first MOCV color as linear shader inputs.
    #[must_use]
    pub const fn color(self) -> [f32; 4] {
        self.color
    }

    /// Returns CVERTS2 or the stock default composite mask.
    #[must_use]
    pub const fn blend_color(self) -> [f32; 4] {
        self.blend_color
    }

    pub(super) fn append_bytes(self, bytes: &mut Vec<u8>) {
        for value in self
            .position
            .into_iter()
            .chain(self.normal)
            .chain(self.texture_coordinates.into_iter().flatten())
            .chain(self.color)
            .chain(self.blend_color)
        {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
}

/// One combined indexed draw retaining the owning group and MOMT identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldModelDrawCall {
    group_index: u32,
    first_index: u32,
    index_count: u32,
    material_id: u8,
    class: WorldModelBatchClass,
    bounds: [[i16; 3]; 2],
}

impl WorldModelDrawCall {
    pub(super) const fn new(
        group_index: u32,
        first_index: u32,
        index_count: u32,
        material_id: u8,
        class: WorldModelBatchClass,
        bounds: [[i16; 3]; 2],
    ) -> Self {
        Self {
            group_index,
            first_index,
            index_count,
            material_id,
            class,
            bounds,
        }
    }

    /// Returns the numeric group file owning this batch.
    #[must_use]
    pub const fn group_index(self) -> u32 {
        self.group_index
    }

    /// Returns the first combined `u32` index.
    #[must_use]
    pub const fn first_index(self) -> u32 {
        self.first_index
    }

    /// Returns the submitted combined index count.
    #[must_use]
    pub const fn index_count(self) -> u32 {
        self.index_count
    }

    /// Returns the root MOMT slot.
    #[must_use]
    pub const fn material_id(self) -> u8 {
        self.material_id
    }

    /// Returns the stock table-position lighting/pass class.
    #[must_use]
    pub const fn class(self) -> WorldModelBatchClass {
        self.class
    }

    /// Returns the authored integer-quantized local culling bounds.
    #[must_use]
    pub const fn bounds(self) -> [[i16; 3]; 2] {
        self.bounds
    }
}

/// One group domain within a shared combined WMO mesh.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldModelGroupRange {
    group_index: u32,
    first_vertex: u32,
    vertex_count: u32,
    first_index: u32,
    index_count: u32,
    flags: u32,
    bounds: [[f32; 3]; 2],
    /// Half-open range of logical drawable batches in the combined mesh plan.
    draw_range: [usize; 2],
    /// Complete shadow ranges, including the merged entirely opaque group span.
    shadow_draw_range: [usize; 2],
}

impl WorldModelGroupRange {
    #[allow(clippy::too_many_arguments)]
    pub(super) const fn new(
        group_index: u32,
        first_vertex: u32,
        vertex_count: u32,
        first_index: u32,
        index_count: u32,
        flags: u32,
        bounds: [[f32; 3]; 2],
        draw_range: [usize; 2],
        shadow_draw_range: [usize; 2],
    ) -> Self {
        Self {
            group_index,
            first_vertex,
            vertex_count,
            first_index,
            index_count,
            flags,
            bounds,
            draw_range,
            shadow_draw_range,
        }
    }

    /// Returns the numeric group file index.
    #[must_use]
    pub const fn group_index(self) -> u32 {
        self.group_index
    }

    /// Returns the first vertex and count in the combined buffer.
    #[must_use]
    pub const fn vertex_range(self) -> [u32; 2] {
        [self.first_vertex, self.vertex_count]
    }

    /// Returns the first index and count in the combined buffer.
    #[must_use]
    pub const fn index_range(self) -> [u32; 2] {
        [self.first_index, self.index_count]
    }

    /// Returns the raw MOGP group flags.
    #[must_use]
    pub const fn flags(self) -> u32 {
        self.flags
    }

    /// Returns the root-local MOGP bounds.
    #[must_use]
    pub const fn bounds(self) -> [[f32; 3]; 2] {
        self.bounds
    }

    /// Returns this group's drawable logical MOBA indices in the combined plan.
    #[must_use]
    pub fn draw_range(self) -> std::ops::Range<usize> {
        self.draw_range[0]..self.draw_range[1]
    }

    /// Returns this group's range in the plan's complete shadow-batch table.
    #[must_use]
    pub fn shadow_draw_range(self) -> std::ops::Range<usize> {
        self.shadow_draw_range[0]..self.shadow_draw_range[1]
    }
}
