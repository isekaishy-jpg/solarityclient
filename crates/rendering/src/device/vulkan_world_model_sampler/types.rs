//! Typed filtering, mip, addressing, and renderer-local sampler identity.

/// Stock's six-value global `textureFilteringMode` domain.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorldModelTextureFiltering {
    /// Linear minification/magnification with point mip selection.
    Bilinear,
    /// Linear minification/magnification and linear mip selection.
    Trilinear,
    /// Anisotropic filtering requested at two samples.
    Anisotropic2x,
    /// Anisotropic filtering requested at four samples.
    Anisotropic4x,
    /// Anisotropic filtering requested at eight samples.
    Anisotropic8x,
    /// Anisotropic filtering requested at sixteen samples.
    Anisotropic16x,
}

impl WorldModelTextureFiltering {
    pub(in crate::device) const fn requested_anisotropy(self) -> f32 {
        match self {
            Self::Bilinear | Self::Trilinear => 1.0,
            Self::Anisotropic2x => 2.0,
            Self::Anisotropic4x => 4.0,
            Self::Anisotropic8x => 8.0,
            Self::Anisotropic16x => 16.0,
        }
    }

    pub(in crate::device) const fn uses_linear_mips(self) -> bool {
        !matches!(self, Self::Bilinear)
    }
}

/// Stock's closed shared `BaseMip` domain.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorldModelBaseMip {
    /// Begin sampling at authored mip zero.
    Zero,
    /// Begin sampling at authored mip one.
    One,
}

impl WorldModelBaseMip {
    pub(super) const fn level(self) -> f32 {
        match self {
            Self::Zero => 0.0,
            Self::One => 1.0,
        }
    }
}

/// One MOMT texture-axis addressing operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorldModelTextureAddressMode {
    /// Sample the edge texel outside the normalized range.
    Clamp,
    /// Repeat the image outside the normalized range.
    Repeat,
}

/// Stable renderer-local handle to one Vulkan WMO sampler.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WorldModelSamplerHandle {
    pub(super) registry_id: u64,
    pub(super) slot: u32,
}

/// Observable global and material-local state represented by one sampler.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WorldModelSamplerInfo {
    filtering: WorldModelTextureFiltering,
    base_mip: WorldModelBaseMip,
    address_u: WorldModelTextureAddressMode,
    address_v: WorldModelTextureAddressMode,
    effective_anisotropy_bits: u32,
}

impl WorldModelSamplerInfo {
    pub(super) fn new(
        filtering: WorldModelTextureFiltering,
        base_mip: WorldModelBaseMip,
        address_u: WorldModelTextureAddressMode,
        address_v: WorldModelTextureAddressMode,
        effective_anisotropy: f32,
    ) -> Self {
        Self {
            filtering,
            base_mip,
            address_u,
            address_v,
            effective_anisotropy_bits: effective_anisotropy.to_bits(),
        }
    }

    /// Returns the selected stock global filtering class.
    #[must_use]
    pub const fn filtering(self) -> WorldModelTextureFiltering {
        self.filtering
    }

    /// Returns the selected shared first mip.
    #[must_use]
    pub const fn base_mip(self) -> WorldModelBaseMip {
        self.base_mip
    }

    /// Returns independent horizontal and vertical MOMT addressing.
    #[must_use]
    pub const fn addressing(self) -> [WorldModelTextureAddressMode; 2] {
        [self.address_u, self.address_v]
    }

    /// Returns anisotropy after capping against selected-adapter capability.
    #[must_use]
    pub const fn effective_anisotropy(self) -> f32 {
        f32::from_bits(self.effective_anisotropy_bits)
    }
}
