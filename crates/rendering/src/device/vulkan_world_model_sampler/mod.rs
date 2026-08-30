//! Stock global filtering plus MOMT-local WMO sampler state.

mod registry;
mod types;

pub use types::{
    WorldModelBaseMip, WorldModelSamplerHandle, WorldModelSamplerInfo,
    WorldModelTextureAddressMode, WorldModelTextureFiltering,
};

pub(in crate::device) use registry::WorldModelSamplerRegistry;
