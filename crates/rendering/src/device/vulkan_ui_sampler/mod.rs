//! Renderer-owned samplers for stock UI tiling combinations.

mod registry;
mod types;

pub(in crate::device) use registry::UiSamplerRegistry;
pub use types::{UiSamplerHandle, UiSamplerInfo};
