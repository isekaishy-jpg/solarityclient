//! Typed sampler identity and observable stock addressing state.

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
        }
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
