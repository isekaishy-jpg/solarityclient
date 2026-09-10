//! Persistent sampled-texture descriptors for stock M2 material stages.

mod pool;
mod registry;
mod types;

pub(in crate::device) use registry::M2TextureSetRegistry;
pub use types::{
    M2SampledTexture, M2TextureImageHandle, M2TextureSet, M2TextureSetHandle, M2TextureSetInfo,
};
