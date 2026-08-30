//! Persistent WMO material sampled-image descriptor ownership.

mod registry;
mod types;

pub use types::{
    WorldModelSampledTexture, WorldModelTextureSet, WorldModelTextureSetHandle,
    WorldModelTextureSetInfo,
};

pub(in crate::device) use registry::WorldModelTextureSetRegistry;
