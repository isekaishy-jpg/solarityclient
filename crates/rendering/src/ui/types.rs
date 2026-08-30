//! Fixed UI vertex, source, sampling, and indexed-batch values.

use solarity_asset::AssetPath;

/// Source-alpha or additive framebuffer composition selected by GlueXML.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UiRenderBlend {
    /// Source alpha over the existing framebuffer color.
    Alpha,
    /// Source color added to the existing framebuffer color.
    Additive,
}

/// Texture addressing selected independently for each UI texture axis.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UiTextureAddressMode {
    /// Coordinates outside the authored edge use the edge texel.
    Clamp,
    /// Coordinates outside the authored edge repeat the texture.
    Repeat,
}

/// Stock residency behavior carried from a texture's `nonBlocking` flag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiTextureResidency {
    /// Resolve the texture before presenting its quad.
    Blocking,
    /// Permit archive decode and upload to complete asynchronously.
    NonBlocking,
}

/// A UI batch either samples one archive BLP or uses vertex color alone.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiRenderSource {
    /// Canonical MPQ path selected through normal patch precedence.
    Texture(AssetPath),
    /// No sampled image; the interpolated vertex color is the final source.
    VertexColor,
}

/// One post-layout quad supplied to renderer-side mesh preparation.
#[derive(Clone, Debug, PartialEq)]
pub struct UiRenderQuad {
    object_index: usize,
    source: UiRenderSource,
    blend: UiRenderBlend,
    horizontal_address: UiTextureAddressMode,
    vertical_address: UiTextureAddressMode,
    residency: UiTextureResidency,
    bounds: [f32; 4],
    texture_coordinates: [[f32; 2]; 4],
    colors: [[f32; 4]; 4],
}

impl UiRenderQuad {
    /// Retains one already ordered quad without changing stock coordinates.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        object_index: usize,
        source: UiRenderSource,
        blend: UiRenderBlend,
        horizontal_address: UiTextureAddressMode,
        vertical_address: UiTextureAddressMode,
        residency: UiTextureResidency,
        bounds: [f32; 4],
        texture_coordinates: [[f32; 2]; 4],
        colors: [[f32; 4]; 4],
    ) -> Self {
        Self {
            object_index,
            source,
            blend,
            horizontal_address,
            vertical_address,
            residency,
            bounds,
            texture_coordinates,
            colors,
        }
    }

    /// Returns the live UI object-arena identity.
    #[must_use]
    pub const fn object_index(&self) -> usize {
        self.object_index
    }

    /// Returns left, bottom, right, and top logical screen edges.
    #[must_use]
    pub const fn bounds(&self) -> [f32; 4] {
        self.bounds
    }

    /// Returns upper-left, lower-left, upper-right, and lower-right UV pairs.
    #[must_use]
    pub const fn texture_coordinates(&self) -> [[f32; 2]; 4] {
        self.texture_coordinates
    }

    /// Returns colors parallel to the four ordered quad corners.
    #[must_use]
    pub const fn colors(&self) -> [[f32; 4]; 4] {
        self.colors
    }

    pub(super) const fn source(&self) -> &UiRenderSource {
        &self.source
    }

    pub(super) const fn blend(&self) -> UiRenderBlend {
        self.blend
    }

    pub(super) const fn horizontal_address(&self) -> UiTextureAddressMode {
        self.horizontal_address
    }

    pub(super) const fn vertical_address(&self) -> UiTextureAddressMode {
        self.vertical_address
    }

    pub(super) const fn residency(&self) -> UiTextureResidency {
        self.residency
    }
}

/// Fixed 32-byte logical-screen UI vertex consumed by the future UI pipeline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiRenderVertex {
    position: [f32; 2],
    texture_coordinates: [f32; 2],
    color: [f32; 4],
}

impl UiRenderVertex {
    /// Size of one explicitly serialized vertex in the Vulkan vertex buffer.
    pub const BYTE_SIZE: usize = 32;

    pub(super) const fn new(
        position: [f32; 2],
        texture_coordinates: [f32; 2],
        color: [f32; 4],
    ) -> Self {
        Self {
            position,
            texture_coordinates,
            color,
        }
    }

    /// Returns the bottom-left-origin logical screen position.
    #[must_use]
    pub const fn position(self) -> [f32; 2] {
        self.position
    }

    /// Returns the authored texture coordinate for this corner.
    #[must_use]
    pub const fn texture_coordinates(self) -> [f32; 2] {
        self.texture_coordinates
    }

    /// Returns the live RGBA multiplier with inherited alpha composed.
    #[must_use]
    pub const fn color(self) -> [f32; 4] {
        self.color
    }

    pub(super) fn append_bytes(self, bytes: &mut Vec<u8>) {
        for value in self.position {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        for value in self.texture_coordinates {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        for value in self.color {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
}

/// One maximal adjacent run sharing sampled-image and fixed material state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiRenderBatch {
    source: UiRenderSource,
    blend: UiRenderBlend,
    horizontal_address: UiTextureAddressMode,
    vertical_address: UiTextureAddressMode,
    residency: UiTextureResidency,
    first_index: u32,
    index_count: u32,
    first_quad: u32,
    quad_count: u32,
}

impl UiRenderBatch {
    pub(super) fn from_quad(quad: &UiRenderQuad, first_index: u32, first_quad: u32) -> Self {
        Self {
            source: quad.source().clone(),
            blend: quad.blend(),
            horizontal_address: quad.horizontal_address(),
            vertical_address: quad.vertical_address(),
            residency: quad.residency(),
            first_index,
            index_count: 6,
            first_quad,
            quad_count: 1,
        }
    }

    pub(super) fn can_append(&self, quad: &UiRenderQuad) -> bool {
        self.source == *quad.source()
            && self.blend == quad.blend()
            && self.horizontal_address == quad.horizontal_address()
            && self.vertical_address == quad.vertical_address()
            && self.residency == quad.residency()
    }

    pub(super) fn append_quad(&mut self) {
        self.index_count += 6;
        self.quad_count += 1;
    }

    /// Returns the sampled BLP or vertex-color-only source.
    #[must_use]
    pub const fn source(&self) -> &UiRenderSource {
        &self.source
    }

    /// Returns the fixed framebuffer blend operation.
    #[must_use]
    pub const fn blend(&self) -> UiRenderBlend {
        self.blend
    }

    /// Returns the U-axis sampler address mode.
    #[must_use]
    pub const fn horizontal_address(&self) -> UiTextureAddressMode {
        self.horizontal_address
    }

    /// Returns the V-axis sampler address mode.
    #[must_use]
    pub const fn vertical_address(&self) -> UiTextureAddressMode {
        self.vertical_address
    }

    /// Returns whether loading this batch may complete asynchronously.
    #[must_use]
    pub const fn residency(&self) -> UiTextureResidency {
        self.residency
    }

    /// Returns the first unsigned 32-bit index submitted by this batch.
    #[must_use]
    pub const fn first_index(&self) -> u32 {
        self.first_index
    }

    /// Returns the number of sequential indices submitted by this batch.
    #[must_use]
    pub const fn index_count(&self) -> u32 {
        self.index_count
    }

    /// Returns the first source quad represented by this batch.
    #[must_use]
    pub const fn first_quad(&self) -> u32 {
        self.first_quad
    }

    /// Returns the number of adjacent compatible quads represented.
    #[must_use]
    pub const fn quad_count(&self) -> u32 {
        self.quad_count
    }
}
