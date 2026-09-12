//! Native glare state and fence-retired GPU occlusion resources.

mod renderer;
mod slot;

pub(super) use renderer::GlareRenderer;
pub(super) use slot::GlareSlot;
