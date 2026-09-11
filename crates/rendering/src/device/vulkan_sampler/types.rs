//! Typed sampler identity and observable stock addressing state.

use crate::device::{WorldModelBaseMip, WorldModelTextureFiltering};

/// One build-12340 M2 texture-axis addressing operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum M2TextureAddressMode {
    /// Sample the nearest edge texel outside the normalized image range.
    Clamp,
    /// Repeat the image for coordinates outside the normalized image range.
    Repeat,
}

/// Stable renderer-local handle to one Vulkan M2 sampler.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct M2SamplerHandle {
    pub(super) registry_id: u64,
    pub(super) slot: u32,
}

/// Observable addressing state represented by one live Vulkan sampler.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct M2SamplerInfo {
    address_u: M2TextureAddressMode,
    address_v: M2TextureAddressMode,
    file_filtering: Option<WorldModelTextureFiltering>,
    base_mip: WorldModelBaseMip,
    anisotropy_bits: u32,
}

impl M2SamplerInfo {
    /// Captures both authored M2 texture-axis flags.
    pub(super) const fn new(
        address_u: M2TextureAddressMode,
        address_v: M2TextureAddressMode,
    ) -> Self {
        Self {
            address_u,
            address_v,
            file_filtering: None,
            base_mip: WorldModelBaseMip::Zero,
            anisotropy_bits: 1.0_f32.to_bits(),
        }
    }

    pub(super) fn with_file_filtering(
        mut self,
        filtering: WorldModelTextureFiltering,
        base_mip: WorldModelBaseMip,
        maximum_anisotropy: f32,
    ) -> Self {
        self.file_filtering = Some(filtering);
        self.base_mip = base_mip;
        self.anisotropy_bits = filtering
            .requested_anisotropy()
            .min(maximum_anisotropy.max(1.0))
            .to_bits();
        self
    }

    /// Returns the shared file-texture filtering, or explicit unmipped sampling.
    #[must_use]
    pub const fn file_filtering(self) -> Option<WorldModelTextureFiltering> {
        self.file_filtering
    }

    /// Returns the first authored mip admitted by the shared BaseMip setting.
    #[must_use]
    pub const fn base_mip(self) -> WorldModelBaseMip {
        self.base_mip
    }

    /// Returns the requested anisotropy capped to the enabled device capability.
    #[must_use]
    pub const fn effective_anisotropy(self) -> f32 {
        f32::from_bits(self.anisotropy_bits)
    }

    /// Returns the stock horizontal texture addressing operation.
    #[must_use]
    pub const fn address_u(self) -> M2TextureAddressMode {
        self.address_u
    }

    /// Returns the stock vertical texture addressing operation.
    #[must_use]
    pub const fn address_v(self) -> M2TextureAddressMode {
        self.address_v
    }
}
