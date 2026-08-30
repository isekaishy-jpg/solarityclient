//! WMO-specific fixed-function state without M2 flag reinterpretation.

use solarity_asset::{WorldModelBlendMode, WorldModelMaterial, WorldModelShader};

/// Backend-independent factors from build 12340's direct `EGxBlend` table.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorldModelBlendFactor {
    /// Numeric zero.
    Zero,
    /// Numeric one.
    One,
    /// Incoming fragment alpha.
    SourceAlpha,
    /// One minus incoming fragment alpha.
    OneMinusSourceAlpha,
    /// Incoming fragment color.
    SourceColor,
    /// Existing framebuffer color.
    DestinationColor,
    /// Existing framebuffer alpha.
    DestinationAlpha,
}

/// Complete color and alpha blending equation for one WMO pass.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WorldModelBlendState {
    mode: WorldModelBlendMode,
    enabled: bool,
    source_color: WorldModelBlendFactor,
    destination_color: WorldModelBlendFactor,
    source_alpha: WorldModelBlendFactor,
    destination_alpha: WorldModelBlendFactor,
}

impl WorldModelBlendState {
    /// Resolves one direct GX blend index without M2's translation table.
    #[must_use]
    pub const fn for_mode(mode: WorldModelBlendMode) -> Self {
        use WorldModelBlendFactor::{
            DestinationAlpha, DestinationColor, One, OneMinusSourceAlpha, SourceAlpha, SourceColor,
            Zero,
        };

        let (enabled, source_color, destination_color, source_alpha, destination_alpha) = match mode
        {
            WorldModelBlendMode::Opaque | WorldModelBlendMode::AlphaKey => {
                (false, One, Zero, One, Zero)
            }
            WorldModelBlendMode::Alpha => (
                true,
                SourceAlpha,
                OneMinusSourceAlpha,
                One,
                OneMinusSourceAlpha,
            ),
            WorldModelBlendMode::Add => (true, SourceAlpha, One, Zero, One),
            WorldModelBlendMode::Mod => (true, DestinationColor, Zero, DestinationAlpha, Zero),
            WorldModelBlendMode::Mod2x => (
                true,
                DestinationColor,
                SourceColor,
                DestinationAlpha,
                SourceAlpha,
            ),
            WorldModelBlendMode::ModAdd => (true, DestinationColor, One, DestinationAlpha, One),
            WorldModelBlendMode::InverseSourceAlphaAdd => {
                (true, OneMinusSourceAlpha, One, OneMinusSourceAlpha, One)
            }
            WorldModelBlendMode::InverseSourceAlphaOpaque => {
                (true, OneMinusSourceAlpha, Zero, OneMinusSourceAlpha, Zero)
            }
            WorldModelBlendMode::SourceAlphaOpaque => (true, SourceAlpha, Zero, SourceAlpha, Zero),
            WorldModelBlendMode::NoAlphaAdd => (true, One, One, Zero, One),
        };
        Self {
            mode,
            enabled,
            source_color,
            destination_color,
            source_alpha,
            destination_alpha,
        }
    }

    /// Returns the direct `EGxBlend` classification.
    #[must_use]
    pub const fn mode(self) -> WorldModelBlendMode {
        self.mode
    }

    /// Reports whether framebuffer blending is active.
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    /// Returns the source and destination color factors.
    #[must_use]
    pub const fn color_factors(self) -> [WorldModelBlendFactor; 2] {
        [self.source_color, self.destination_color]
    }

    /// Returns the source and destination alpha factors.
    #[must_use]
    pub const fn alpha_factors(self) -> [WorldModelBlendFactor; 2] {
        [self.source_alpha, self.destination_alpha]
    }
}

/// Fog color specialization selected by the direct GX blend mode.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorldModelFogMode {
    /// MOMT `0x02` disables fog.
    Disabled,
    /// Use the active world fog color.
    SceneColor,
    /// Additive paths fade toward black.
    Black,
    /// Modulation fades toward white.
    White,
    /// Doubled modulation fades toward half-white.
    HalfWhite,
}

/// Immutable WMO material state compiled into a Vulkan pipeline key.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WorldModelMaterialState {
    shader: WorldModelShader,
    blend: WorldModelBlendState,
    cull_enabled: bool,
    is_unlit: bool,
    is_unfogged: bool,
    clamp_s: bool,
    clamp_t: bool,
}

impl WorldModelMaterialState {
    /// Converts one normalized MOMT material using WMO-only flag meanings.
    #[must_use]
    pub const fn from_material(material: &WorldModelMaterial) -> Self {
        Self::from_material_with_blend(material, material.blend_mode())
    }

    pub(super) const fn from_material_with_blend(
        material: &WorldModelMaterial,
        blend_mode: WorldModelBlendMode,
    ) -> Self {
        let flags = material.flags();
        Self {
            shader: material.shader(),
            blend: WorldModelBlendState::for_mode(blend_mode),
            cull_enabled: flags & 0x04 == 0,
            is_unlit: flags & 0x01 != 0,
            is_unfogged: flags & 0x02 != 0,
            clamp_s: flags & 0x40 != 0,
            clamp_t: flags & 0x80 != 0,
        }
    }

    /// Returns the normalized MapObj shader selector.
    #[must_use]
    pub const fn shader(self) -> WorldModelShader {
        self.shader
    }

    /// Returns one or two sampled textures according to the stock effect.
    #[must_use]
    pub const fn texture_count(self) -> u8 {
        match self.shader {
            WorldModelShader::Environment
            | WorldModelShader::EnvironmentMetal
            | WorldModelShader::Composite => 2,
            WorldModelShader::Diffuse
            | WorldModelShader::Specular
            | WorldModelShader::Metal
            | WorldModelShader::Opaque => 1,
        }
    }

    /// Returns the complete direct GX blend equation.
    #[must_use]
    pub const fn blend(self) -> WorldModelBlendState {
        self.blend
    }

    /// Reports whether stock culls back-facing WMO triangles.
    #[must_use]
    pub const fn cull_enabled(self) -> bool {
        self.cull_enabled
    }

    /// Reports the invariant enabled WMO depth comparison state.
    #[must_use]
    pub const fn depth_test_enabled(self) -> bool {
        true
    }

    /// Reports the invariant enabled WMO depth-write state.
    #[must_use]
    pub const fn depth_write_enabled(self) -> bool {
        true
    }

    /// Reports whether the material bypasses dynamic surface lighting.
    #[must_use]
    pub const fn is_unlit(self) -> bool {
        self.is_unlit
    }

    /// Reports whether the material bypasses world fog.
    #[must_use]
    pub const fn is_unfogged(self) -> bool {
        self.is_unfogged
    }

    /// Returns stock's independent horizontal and vertical texture clamps.
    #[must_use]
    pub const fn texture_clamps(self) -> [bool; 2] {
        [self.clamp_s, self.clamp_t]
    }

    /// Returns the fixed alpha-test reference selected by direct GX index.
    #[must_use]
    pub const fn alpha_reference(self) -> f32 {
        match self.blend.mode() {
            WorldModelBlendMode::AlphaKey => 224.0 / 255.0,
            WorldModelBlendMode::Alpha
            | WorldModelBlendMode::Add
            | WorldModelBlendMode::Mod
            | WorldModelBlendMode::Mod2x
            | WorldModelBlendMode::ModAdd => 1.0 / 255.0,
            WorldModelBlendMode::Opaque
            | WorldModelBlendMode::InverseSourceAlphaAdd
            | WorldModelBlendMode::InverseSourceAlphaOpaque
            | WorldModelBlendMode::SourceAlphaOpaque
            | WorldModelBlendMode::NoAlphaAdd => 0.0,
        }
    }

    /// Returns the fog specialization used by the stock pixel path.
    #[must_use]
    pub const fn fog_mode(self) -> WorldModelFogMode {
        if self.is_unfogged {
            return WorldModelFogMode::Disabled;
        }
        match self.blend.mode() {
            WorldModelBlendMode::NoAlphaAdd | WorldModelBlendMode::Add => WorldModelFogMode::Black,
            WorldModelBlendMode::Mod => WorldModelFogMode::White,
            WorldModelBlendMode::Mod2x => WorldModelFogMode::HalfWhite,
            _ => WorldModelFogMode::SceneColor,
        }
    }
}
