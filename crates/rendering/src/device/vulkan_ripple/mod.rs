//! Native surface ripple passes and fence-local streaming resources.

mod frame;
mod pipeline;
mod types;

pub(in crate::device) use frame::RippleFrameResources;
pub(in crate::device) use pipeline::RipplePipeline;
pub use types::{WaterRippleFrame, WaterRippleFrameError, WaterRipplePass};
