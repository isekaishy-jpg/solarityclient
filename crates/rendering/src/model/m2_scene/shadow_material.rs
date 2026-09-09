//! Original 834660 caster-batch admission and 82DA40 alpha-test queues.

use solarity_asset::{M2Batch, M2BlendMode};

/// The two material queues consumed by stock's ShadowMapSL shader.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M2ShadowMaterial {
    /// A solid silhouette without texture sampling.
    Opaque,
    /// The first untransformed texture coordinates use an alpha cutoff of 128/255.
    AlphaTest,
}

impl M2ShadowMaterial {
    /// Selects a caster queue from an admitted geoset's authored batch and opacity.
    ///
    /// `element_alpha` includes instance, animated color, and first texture-weight
    /// alpha. Geoset visibility, model readiness, and spatial admission belong to
    /// the caller, as they do before the material branch of original 834660.
    #[must_use]
    pub fn select(
        batch: M2Batch,
        material_flags: u16,
        blend_mode: M2BlendMode,
        element_alpha: f32,
    ) -> Option<Self> {
        if batch.flags & 4 != 0
            || batch.shader_id == 0x8000
            || batch.material_layer != 0
            || material_flags & 0x40 != 0
            || element_alpha < 0.55
            || element_alpha.is_nan()
        {
            return None;
        }
        if material_flags & 0x80 == 0
            && !matches!(blend_mode, M2BlendMode::Opaque | M2BlendMode::AlphaKey)
        {
            return None;
        }
        Some(if blend_mode == M2BlendMode::Opaque {
            Self::Opaque
        } else {
            Self::AlphaTest
        })
    }

    /// Returns the original pixel-alpha cutoff; zero disables texture rejection.
    #[must_use]
    pub const fn alpha_reference(self) -> f32 {
        match self {
            Self::Opaque => 0.0,
            Self::AlphaTest => 128.0 / 255.0,
        }
    }
}
