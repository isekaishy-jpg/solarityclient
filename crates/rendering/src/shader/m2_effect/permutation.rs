//! Build-12340 indices into each 90-vertex/16-pixel BLS permutation array.

use solarity_asset::M2BlendMode;

use crate::model::M2DrawCall;

use super::M2MaterialState;

/// Stock's bounded count of local lights compiled into an M2 vertex shader.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum M2LocalLightCount {
    /// No local lights.
    Zero,
    /// One local light.
    One,
    /// Two local lights.
    Two,
    /// Three local lights.
    Three,
    /// Four local lights.
    Four,
}

impl M2LocalLightCount {
    /// Returns the count encoded in stock's vertex permutation formula.
    const fn value(self) -> usize {
        match self {
            Self::Zero => 0,
            Self::One => 1,
            Self::Two => 2,
            Self::Three => 3,
            Self::Four => 4,
        }
    }
}

/// Opaque stock shadow permutation number before vertex-side clamping.
///
/// The exact numeric classes are retained because the recovered executable
/// distinguishes four pixel paths while collapsing classes two and three in
/// the vertex path; names beyond disabled are not yet evidenced semantically.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum M2ShadowPermutation {
    /// No model shadow sampling.
    Disabled,
    /// Stock shadow selector one.
    Mode1,
    /// Stock shadow selector two.
    Mode2,
    /// Stock shadow selector three.
    Mode3,
}

impl M2ShadowPermutation {
    /// Resolves the selector table indexed by stock's effective shadow quality.
    ///
    /// The executable contains seven entries: `0, 1, 1, 2, 2, 3, 3`.
    /// Values outside that table are not stock configuration states.
    #[must_use]
    pub const fn from_stock_quality(quality: usize) -> Option<Self> {
        match quality {
            0 => Some(Self::Disabled),
            1 | 2 => Some(Self::Mode1),
            3 | 4 => Some(Self::Mode2),
            5 | 6 => Some(Self::Mode3),
            _ => None,
        }
    }

    /// Returns the exact pixel-side selector value.
    const fn value(self) -> usize {
        match self {
            Self::Disabled => 0,
            Self::Mode1 => 1,
            Self::Mode2 => 2,
            Self::Mode3 => 3,
        }
    }
}

/// Stock's global choice between direct and percentage-closer shadow sampling.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum M2ShadowFiltering {
    /// Direct shadow-map sampling.
    Direct,
    /// Percentage-closer filtering.
    Pcf,
}

impl M2ShadowFiltering {
    /// Returns the one-bit pixel permutation selector.
    const fn value(self) -> usize {
        match self {
            Self::Direct => 0,
            Self::Pcf => 1,
        }
    }
}

/// Exact indices used to retrieve one vertex and pixel BLS permutation pair.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct M2ShaderPermutation {
    vertex_index: usize,
    pixel_index: usize,
}

impl M2ShaderPermutation {
    /// Replays `CM2Scene::ComputeElementShaders` for the Vulkan shader path.
    ///
    /// Vulkan has no fixed-function alpha test, so every nonopaque stock blend
    /// class selects the pixel permutation that performs the alpha discard.
    #[must_use]
    pub fn resolve(
        draw: &M2DrawCall,
        local_lights: M2LocalLightCount,
        shadows: M2ShadowPermutation,
        filtering: M2ShadowFiltering,
    ) -> Self {
        let material = M2MaterialState::from_material(draw.material());
        let shaded = usize::from(!material.is_unlit());
        let light_count = if shaded == 0 { 0 } else { local_lights.value() };
        let bone_class = usize::from(draw.bone_influence().min(2));
        let shadow_class = shadows.value().min(2);
        let vertex_index = shaded + 2 * light_count + 10 * bone_class + 30 * shadow_class;

        let shader_alpha_test = usize::from(material.blend_mode() != M2BlendMode::Opaque);
        let pixel_index = shadows.value() + 4 * filtering.value() + 8 * shader_alpha_test;
        Self {
            vertex_index,
            pixel_index,
        }
    }

    /// Returns the zero-based entry in the 90-permutation vertex BLS.
    #[must_use]
    pub const fn vertex_index(self) -> usize {
        self.vertex_index
    }

    /// Returns the zero-based entry in the 16-permutation pixel BLS.
    #[must_use]
    pub const fn pixel_index(self) -> usize {
        self.pixel_index
    }

    /// Reports whether this pair statically consumes the shadow descriptor set.
    #[must_use]
    pub const fn has_shadows(self) -> bool {
        !self.pixel_index.is_multiple_of(4)
    }
}
