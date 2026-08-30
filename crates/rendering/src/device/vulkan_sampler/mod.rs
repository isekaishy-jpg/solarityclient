//! Stock M2 texture addressing and renderer-owned Vulkan samplers.

mod registry;
mod types;

pub(in crate::device) use registry::M2SamplerRegistry;
pub use types::{M2SamplerHandle, M2SamplerInfo, M2TextureAddressMode};
