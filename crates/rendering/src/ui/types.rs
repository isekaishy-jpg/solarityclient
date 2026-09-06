//! Fixed UI vertex, source, sampling, and indexed-batch values.

use solarity_asset::AssetPath;

/// Source-alpha or additive framebuffer composition selected by GlueXML.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UiRenderBlend {
    /// Source alpha over the existing framebuffer color.
    Alpha,
    /// Source color added to the existing framebuffer color.
    Additive,
    /// Replace destination alpha with texture alpha, preserving destination RGB.
    /// Used by the stock portrait mask pass.
    AlphaMask,
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

/// Retained draw-state slot whose motion does not modify immutable UI vertices.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UiRenderTransform {
    /// The assigned ScrollFrame child moves beneath a fixed rectangular clip.
    ScrollFrame(usize),
    /// A native unanchored Slider thumb moves along its fixed track.
    Slider(usize),
}

/// Independently mutable visibility owned by immutable UI geometry.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UiRenderState {
    /// The insertion cell retained by one EditBox text layout.
    EditBoxCaret(usize),
}

/// A UI batch either samples one archive BLP or uses vertex color alone.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiRenderSource {
    /// Canonical MPQ path selected through normal patch precedence.
    Texture(AssetPath),
    /// Process-local immutable coverage atlas generated from archive fonts.
    GlyphAtlas(u64),
    /// A renderer-owned offscreen portrait for one canonical unit token.
    UnitPortrait(String),
    /// No sampled image; the interpolated vertex color is the final source.
    VertexColor,
}

/// A second archive image whose alpha masks a logical rectangle independently
/// of the source image's UVs. Translation moves the mask with its geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct UiRenderMask {
    path: AssetPath,
    bounds: [f32; 4],
}

impl UiRenderMask {
    /// Selects the mask and its left, bottom, right, top logical edges.
    #[must_use]
    pub const fn new(path: AssetPath, bounds: [f32; 4]) -> Self {
        Self { path, bounds }
    }

    /// Returns the mask's archive identity.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns the mask rectangle before retained draw translation.
    #[must_use]
    pub const fn bounds(&self) -> [f32; 4] {
        self.bounds
    }
}

/// Rectangular UI regions and native transformed four-corner geometry.
#[derive(Clone, Debug, PartialEq)]
enum UiQuadGeometry {
    Rectangle([f32; 4]),
    Corners([[f32; 2]; 4]),
}

/// One post-layout quad supplied to renderer-side mesh preparation.
#[derive(Clone, Debug, PartialEq)]
pub struct UiRenderQuad {
    object_index: usize,
    mask: Option<UiRenderMask>,
    source: UiRenderSource,
    blend: UiRenderBlend,
    horizontal_address: UiTextureAddressMode,
    vertical_address: UiTextureAddressMode,
    residency: UiTextureResidency,
    desaturated: bool,
    geometry: UiQuadGeometry,
    texture_coordinates: [[f32; 2]; 4],
    colors: [[f32; 4]; 4],
    opacity: f32,
    transform: Option<UiRenderTransform>,
    state: Option<UiRenderState>,
    translation: [f32; 2],
    clip: Option<[f32; 4]>,
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
        desaturated: bool,
        bounds: [f32; 4],
        texture_coordinates: [[f32; 2]; 4],
        colors: [[f32; 4]; 4],
    ) -> Self {
        Self {
            object_index,
            mask: None,
            source,
            blend,
            horizontal_address,
            vertical_address,
            residency,
            desaturated,
            geometry: UiQuadGeometry::Rectangle(bounds),
            texture_coordinates,
            colors,
            opacity: 1.0,
            transform: None,
            state: None,
            translation: [0.0, 0.0],
            clip: None,
        }
    }

    /// Multiplies the source alpha by a separately sampled archive mask.
    #[must_use]
    pub fn with_mask(mut self, mask: UiRenderMask) -> Self {
        self.mask = Some(mask);
        self
    }

    /// Returns the optional second-image mask.
    #[must_use]
    pub const fn mask(&self) -> Option<&UiRenderMask> {
        self.mask.as_ref()
    }

    /// Supplies upper-left, lower-left, upper-right, and lower-right corners
    /// after a native 2D transform, retaining the usual quad indices and UVs.
    #[must_use]
    pub const fn with_positions(mut self, positions: [[f32; 2]; 4]) -> Self {
        self.geometry = UiQuadGeometry::Corners(positions);
        self
    }

    /// Clips this packet to a fixed logical rectangle without a scroll transform.
    #[must_use]
    pub const fn with_clip(mut self, bounds: [f32; 4]) -> Self {
        self.clip = Some(bounds);
        self
    }

    /// Applies inherited region alpha as retained draw state.
    ///
    /// Stock simple-render alpha belongs to the draw packet rather than the
    /// immutable corner positions and texture coordinates. Keeping it outside
    /// vertex colors lets a fade update one scalar without replacing every
    /// glyph vertex owned by the region.
    #[must_use]
    pub const fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    /// Assigns immutable geometry to one small retained transform/scissor slot.
    #[must_use]
    pub const fn with_transform(
        mut self,
        transform: UiRenderTransform,
        translation: [f32; 2],
        clip: Option<[f32; 4]>,
    ) -> Self {
        self.transform = Some(transform);
        self.translation = translation;
        self.clip = clip;
        self
    }

    /// Assigns this quad to one independently mutable draw-state slot.
    #[must_use]
    pub const fn with_state(mut self, state: UiRenderState) -> Self {
        self.state = Some(state);
        self
    }

    /// Returns the live UI object-arena identity.
    #[must_use]
    pub const fn object_index(&self) -> usize {
        self.object_index
    }

    /// Returns left, bottom, right, and top logical screen edges.
    #[must_use]
    pub fn bounds(&self) -> [f32; 4] {
        match self.geometry {
            UiQuadGeometry::Rectangle(bounds) => bounds,
            UiQuadGeometry::Corners(positions) => {
                let mut bounds = [
                    f32::INFINITY,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::NEG_INFINITY,
                ];
                for [x, y] in positions {
                    bounds[0] = bounds[0].min(x);
                    bounds[1] = bounds[1].min(y);
                    bounds[2] = bounds[2].max(x);
                    bounds[3] = bounds[3].max(y);
                }
                bounds
            }
        }
    }

    /// Returns transformed positions parallel to texture coordinates and colors.
    #[must_use]
    pub const fn positions(&self) -> [[f32; 2]; 4] {
        match self.geometry {
            UiQuadGeometry::Rectangle([left, bottom, right, top]) => {
                [[left, top], [left, bottom], [right, top], [right, bottom]]
            }
            UiQuadGeometry::Corners(positions) => positions,
        }
    }

    pub(super) const fn custom_positions(&self) -> Option<&[[f32; 2]; 4]> {
        match &self.geometry {
            UiQuadGeometry::Rectangle(_) => None,
            UiQuadGeometry::Corners(positions) => Some(positions),
        }
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

    pub(super) const fn desaturated(&self) -> bool {
        self.desaturated
    }

    pub(super) const fn transform(&self) -> Option<UiRenderTransform> {
        self.transform
    }

    pub(super) const fn state(&self) -> Option<UiRenderState> {
        self.state
    }

    pub(super) const fn translation(&self) -> [f32; 2] {
        self.translation
    }

    pub(super) const fn opacity(&self) -> f32 {
        self.opacity
    }

    pub(super) const fn clip(&self) -> Option<[f32; 4]> {
        self.clip
    }
}

/// Fixed 32-byte logical-screen UI vertex consumed by the future UI pipeline.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UiRenderVertex {
    position: [f32; 2],
    texture_coordinates: [f32; 2],
    color: [f32; 4],
}

impl UiRenderVertex {
    /// Size of one explicitly serialized vertex in the Vulkan vertex buffer.
    pub const BYTE_SIZE: usize = 32;

    /// Creates one renderer vertex in bottom-left logical coordinates.
    #[must_use]
    pub const fn new(position: [f32; 2], texture_coordinates: [f32; 2], color: [f32; 4]) -> Self {
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
}

/// One maximal adjacent run sharing sampled-image and fixed material state.
#[derive(Clone, Debug, PartialEq)]
pub struct UiRenderBatch {
    object_index: usize,
    mask: Option<UiRenderMask>,
    source: UiRenderSource,
    blend: UiRenderBlend,
    horizontal_address: UiTextureAddressMode,
    vertical_address: UiTextureAddressMode,
    residency: UiTextureResidency,
    desaturated: bool,
    first_index: u32,
    index_count: u32,
    first_quad: u32,
    quad_count: u32,
    transform: Option<UiRenderTransform>,
    state: Option<UiRenderState>,
    transform_translation: [f32; 2],
    object_translation: [f32; 2],
    opacity: f32,
    clip: Option<[f32; 4]>,
}

impl UiRenderBatch {
    pub(super) fn from_quad(quad: &UiRenderQuad, first_index: u32, first_quad: u32) -> Self {
        Self {
            object_index: quad.object_index(),
            mask: quad.mask().cloned(),
            source: quad.source().clone(),
            blend: quad.blend(),
            horizontal_address: quad.horizontal_address(),
            vertical_address: quad.vertical_address(),
            residency: quad.residency(),
            desaturated: quad.desaturated(),
            first_index,
            index_count: 6,
            first_quad,
            quad_count: 1,
            transform: quad.transform(),
            state: quad.state(),
            transform_translation: quad.translation(),
            object_translation: [0.0, 0.0],
            opacity: quad.opacity(),
            clip: quad.clip(),
        }
    }

    pub(super) fn can_append(&self, quad: &UiRenderQuad) -> bool {
        self.object_index == quad.object_index()
            && self.mask.as_ref() == quad.mask()
            && self.source == *quad.source()
            && self.blend == quad.blend()
            && self.horizontal_address == quad.horizontal_address()
            && self.vertical_address == quad.vertical_address()
            && self.residency == quad.residency()
            && self.desaturated == quad.desaturated()
            && self.transform == quad.transform()
            && self.state == quad.state()
            && self.transform_translation == quad.translation()
            && self.opacity == quad.opacity()
            && self.clip == quad.clip()
    }

    /// Reports whether one existing slot can accept replacement vertices.
    ///
    /// Opacity is intentionally excluded: it is mutable retained draw state
    /// (not vertex topology), and an EditBox caret can be blink-hidden while
    /// its text geometry changes.
    pub(super) fn can_replace(&self, quad: &UiRenderQuad) -> bool {
        self.object_index == quad.object_index()
            && self.mask.as_ref() == quad.mask()
            && self.source == *quad.source()
            && self.blend == quad.blend()
            && self.horizontal_address == quad.horizontal_address()
            && self.vertical_address == quad.vertical_address()
            && self.residency == quad.residency()
            && self.desaturated == quad.desaturated()
            && self.transform == quad.transform()
            && self.state == quad.state()
            && self.transform_translation == quad.translation()
            && self.clip == quad.clip()
    }

    pub(super) fn append_quad(&mut self) {
        self.index_count += 6;
        self.quad_count += 1;
    }

    pub(super) const fn object_index(&self) -> usize {
        self.object_index
    }

    /// Returns the sampled BLP or vertex-color-only source.
    #[must_use]
    pub const fn source(&self) -> &UiRenderSource {
        &self.source
    }

    /// Returns the independently sampled alpha mask shared by this batch.
    #[must_use]
    pub const fn mask(&self) -> Option<&UiRenderMask> {
        self.mask.as_ref()
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

    /// Returns whether this batch selects the grayscale texture shader path.
    #[must_use]
    pub const fn desaturated(&self) -> bool {
        self.desaturated
    }

    /// Returns the first unsigned 32-bit index submitted by this batch.
    ///
    /// Quad batches share the mesh's canonical zero-based index prefix; only
    /// explicitly indexed developer meshes may use another offset.
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

    pub(super) const fn from_indexed(
        source: UiRenderSource,
        index_count: u32,
        clip: Option<[f32; 4]>,
    ) -> Self {
        Self {
            object_index: 0,
            mask: None,
            source,
            blend: UiRenderBlend::Alpha,
            horizontal_address: UiTextureAddressMode::Clamp,
            vertical_address: UiTextureAddressMode::Clamp,
            residency: UiTextureResidency::Blocking,
            desaturated: false,
            first_index: 0,
            index_count,
            first_quad: 0,
            quad_count: 0,
            transform: None,
            state: None,
            transform_translation: [0.0, 0.0],
            object_translation: [0.0, 0.0],
            opacity: 1.0,
            clip,
        }
    }

    /// Returns the retained state slot controlling this batch.
    #[must_use]
    pub const fn transform(&self) -> Option<UiRenderTransform> {
        self.transform
    }

    /// Returns the independently mutable state slot for this batch.
    #[must_use]
    pub const fn state(&self) -> Option<UiRenderState> {
        self.state
    }

    /// Returns the logical translation applied by the vertex shader.
    #[must_use]
    pub const fn translation(&self) -> [f32; 2] {
        [
            self.transform_translation[0] + self.object_translation[0],
            self.transform_translation[1] + self.object_translation[1],
        ]
    }

    pub(super) const fn transform_translation(&self) -> [f32; 2] {
        self.transform_translation
    }

    pub(super) const fn object_translation(&self) -> [f32; 2] {
        self.object_translation
    }

    /// Returns the inherited region opacity applied by the fragment stage.
    #[must_use]
    pub const fn opacity(&self) -> f32 {
        self.opacity
    }

    /// Returns an optional bottom-left-origin logical scissor rectangle.
    #[must_use]
    pub const fn clip(&self) -> Option<[f32; 4]> {
        self.clip
    }

    pub(super) fn set_translation(&mut self, translation: [f32; 2]) {
        self.transform_translation = translation;
    }

    pub(super) fn translate_object(&mut self, delta: [f32; 2]) {
        self.object_translation[0] += delta[0];
        self.object_translation[1] += delta[1];
    }

    /// Newly resolved absolute vertices already include the region's movement.
    pub(super) fn reset_object_translation(&mut self) {
        self.object_translation = [0.0; 2];
    }

    /// Relocates an ordered quad run after another run changes size.
    pub(super) fn set_first_quad(&mut self, first_quad: u32) {
        self.first_quad = first_quad;
    }

    pub(super) fn set_clip(&mut self, clip: Option<[f32; 4]>) {
        self.clip = clip;
    }

    pub(super) fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity;
    }
}
