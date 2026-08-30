//! Persistent sampled-image descriptors for stock UI texture batches.

mod registry;
mod types;

pub(in crate::device) use registry::UiTextureSetRegistry;
pub use types::{UiSampledTexture, UiTextureSetHandle, UiTextureSetInfo};
