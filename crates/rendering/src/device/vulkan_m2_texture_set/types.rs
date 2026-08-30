//! Validated one/two-stage texture sets and renderer-local identities.

use crate::device::{BlpTextureHandle, M2SamplerHandle};

/// One uploaded image and stock sampler paired for an M2 texture stage.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct M2SampledTexture {
    texture: BlpTextureHandle,
    sampler: M2SamplerHandle,
}

impl M2SampledTexture {
    /// Pairs independently cached image and sampling state identities.
    #[must_use]
    pub const fn new(texture: BlpTextureHandle, sampler: M2SamplerHandle) -> Self {
        Self { texture, sampler }
    }

    /// Returns the renderer-local uploaded image identity.
    #[must_use]
    pub const fn texture(self) -> BlpTextureHandle {
        self.texture
    }

    /// Returns the renderer-local stock sampler identity.
    #[must_use]
    pub const fn sampler(self) -> M2SamplerHandle {
        self.sampler
    }
}

/// The closed build-12340 M2 material texture-stage domain.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum M2TextureSet {
    /// A material effect consuming one sampled texture.
    One(M2SampledTexture),
    /// A material effect consuming two sampled textures in stage order.
    Two([M2SampledTexture; 2]),
}

impl M2TextureSet {
    /// Returns the sampled stages in exact material order.
    #[must_use]
    pub fn stages(&self) -> &[M2SampledTexture] {
        match self {
            Self::One(stage) => std::slice::from_ref(stage),
            Self::Two(stages) => stages,
        }
    }

    /// Returns the statically specialized stage count.
    #[must_use]
    pub const fn stage_count(self) -> u16 {
        match self {
            Self::One(_) => 1,
            Self::Two(_) => 2,
        }
    }
}

/// Stable renderer-local handle to one persistent M2 texture descriptor set.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct M2TextureSetHandle {
    pub(super) registry_id: u64,
    pub(super) slot: u32,
}

/// Observable material-stage count represented by a live descriptor set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M2TextureSetInfo {
    stage_count: u16,
}

impl M2TextureSetInfo {
    /// Captures the statically specialized stage count.
    pub(super) const fn new(stage_count: u16) -> Self {
        Self { stage_count }
    }

    /// Returns the number of sampled image descriptors written into the set.
    #[must_use]
    pub const fn stage_count(self) -> u16 {
        self.stage_count
    }
}
