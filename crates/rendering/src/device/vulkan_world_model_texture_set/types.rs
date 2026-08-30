//! Validated one/two-stage WMO texture sets and stable identities.

use crate::device::{BlpTextureHandle, WorldModelSamplerHandle};

/// One uploaded WMO image paired with its material/global sampler state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WorldModelSampledTexture {
    texture: BlpTextureHandle,
    sampler: WorldModelSamplerHandle,
}

impl WorldModelSampledTexture {
    /// Pairs independently cached image and sampler identities.
    #[must_use]
    pub const fn new(texture: BlpTextureHandle, sampler: WorldModelSamplerHandle) -> Self {
        Self { texture, sampler }
    }

    /// Returns the renderer-local uploaded image identity.
    #[must_use]
    pub const fn texture(self) -> BlpTextureHandle {
        self.texture
    }

    /// Returns the renderer-local WMO sampler identity.
    #[must_use]
    pub const fn sampler(self) -> WorldModelSamplerHandle {
        self.sampler
    }
}

/// Stock's closed one/two-texture MapObj material domain.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorldModelTextureSet {
    /// Diffuse, Specular, Metal, or Opaque effect input.
    One(WorldModelSampledTexture),
    /// Environment, EnvironmentMetal, or Composite inputs in stage order.
    Two([WorldModelSampledTexture; 2]),
}

impl WorldModelTextureSet {
    /// Returns sampled stages in exact MOMT effect order.
    #[must_use]
    pub fn stages(&self) -> &[WorldModelSampledTexture] {
        match self {
            Self::One(stage) => std::slice::from_ref(stage),
            Self::Two(stages) => stages,
        }
    }

    /// Returns the statically consumed effect texture count.
    #[must_use]
    pub const fn stage_count(self) -> u8 {
        match self {
            Self::One(_) => 1,
            Self::Two(_) => 2,
        }
    }
}

/// Stable renderer-local WMO material descriptor-set handle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WorldModelTextureSetHandle {
    pub(super) registry_id: u64,
    pub(super) slot: u32,
}

/// Observable texture count represented by one descriptor set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldModelTextureSetInfo {
    stage_count: u8,
}

impl WorldModelTextureSetInfo {
    pub(super) const fn new(stage_count: u8) -> Self {
        Self { stage_count }
    }

    /// Returns one or two statically consumed sampled images.
    #[must_use]
    pub const fn stage_count(self) -> u8 {
        self.stage_count
    }
}
