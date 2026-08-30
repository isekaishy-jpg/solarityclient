//! Named stock M2 vertex and pixel shader permutations.

/// Vertex shader selected by texture count, UV routing, and environment bits.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum M2VertexShader {
    /// One stage using the first authored UV set.
    DiffuseT1,
    /// One stage using the second authored UV set.
    DiffuseT2,
    /// One environment-mapped stage.
    DiffuseEnv,
    /// Two ordinary texture-coordinate stages.
    DiffuseT1T2,
    /// Environment mapping followed by an ordinary second stage.
    DiffuseEnvT2,
    /// Ordinary first stage followed by environment mapping.
    DiffuseT1Env,
    /// Two environment-mapped stages.
    DiffuseEnvEnv,
}

impl M2VertexShader {
    /// Returns the exact BLS basename requested by stock.
    #[must_use]
    pub const fn stock_name(self) -> &'static str {
        match self {
            Self::DiffuseT1 => "Diffuse_T1",
            Self::DiffuseT2 => "Diffuse_T2",
            Self::DiffuseEnv => "Diffuse_Env",
            Self::DiffuseT1T2 => "Diffuse_T1_T2",
            Self::DiffuseEnvT2 => "Diffuse_Env_T2",
            Self::DiffuseT1Env => "Diffuse_T1_Env",
            Self::DiffuseEnvEnv => "Diffuse_Env_Env",
        }
    }
}

/// Pixel shader selected by the one- or two-stage stock combiner pair.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum M2PixelShader {
    /// One opaque stage.
    Opaque,
    /// One modulated stage.
    Mod,
    /// One decal stage.
    Decal,
    /// One additive stage.
    Add,
    /// One doubled modulation stage.
    Mod2x,
    /// One fade stage.
    Fade,
    /// Opaque then opaque.
    OpaqueOpaque,
    /// Opaque then modulation.
    OpaqueMod,
    /// Opaque then additive.
    OpaqueAdd,
    /// Opaque then doubled modulation.
    OpaqueMod2x,
    /// Opaque then doubled modulation without texture alpha.
    OpaqueMod2xNoAlpha,
    /// Opaque then additive without texture alpha.
    OpaqueAddNoAlpha,
    /// Modulation then opaque.
    ModOpaque,
    /// Two modulation stages.
    ModMod,
    /// Modulation then additive.
    ModAdd,
    /// Modulation then doubled modulation.
    ModMod2x,
    /// Modulation then doubled modulation without texture alpha.
    ModMod2xNoAlpha,
    /// Modulation then additive without texture alpha.
    ModAddNoAlpha,
    /// Additive then modulation.
    AddMod,
    /// Two doubled-modulation stages.
    Mod2xMod2x,
    /// Specialized opaque/environment reflection with alpha.
    OpaqueMod2xNoAlphaAlpha,
    /// Specialized opaque/environment additive alpha.
    OpaqueAddAlpha,
    /// Specialized opaque/environment additive alpha with output alpha.
    OpaqueAddAlphaAlpha,
}

impl M2PixelShader {
    /// Returns the exact BLS basename requested by stock.
    #[must_use]
    pub const fn stock_name(self) -> &'static str {
        match self {
            Self::Opaque => "Combiners_Opaque",
            Self::Mod => "Combiners_Mod",
            Self::Decal => "Combiners_Decal",
            Self::Add => "Combiners_Add",
            Self::Mod2x => "Combiners_Mod2x",
            Self::Fade => "Combiners_Fade",
            Self::OpaqueOpaque => "Combiners_Opaque_Opaque",
            Self::OpaqueMod => "Combiners_Opaque_Mod",
            Self::OpaqueAdd => "Combiners_Opaque_Add",
            Self::OpaqueMod2x => "Combiners_Opaque_Mod2x",
            Self::OpaqueMod2xNoAlpha => "Combiners_Opaque_Mod2xNA",
            Self::OpaqueAddNoAlpha => "Combiners_Opaque_AddNA",
            Self::ModOpaque => "Combiners_Mod_Opaque",
            Self::ModMod => "Combiners_Mod_Mod",
            Self::ModAdd => "Combiners_Mod_Add",
            Self::ModMod2x => "Combiners_Mod_Mod2x",
            Self::ModMod2xNoAlpha => "Combiners_Mod_Mod2xNA",
            Self::ModAddNoAlpha => "Combiners_Mod_AddNA",
            Self::AddMod => "Combiners_Add_Mod",
            Self::Mod2xMod2x => "Combiners_Mod2x_Mod2x",
            Self::OpaqueMod2xNoAlphaAlpha => "Combiners_Opaque_Mod2xNA_Alpha",
            Self::OpaqueAddAlpha => "Combiners_Opaque_AddAlpha",
            Self::OpaqueAddAlphaAlpha => "Combiners_Opaque_AddAlpha_Alpha",
        }
    }
}
