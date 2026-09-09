//! Frame-owned primary shadow maps and the original M2 caster pass.

mod frame;
mod pipeline;
mod resource;

pub use frame::WorldPrimaryShadowFrame;
pub(in crate::device) use pipeline::ShadowPipelines;
pub(in crate::device) use resource::ShadowFrameResources;
