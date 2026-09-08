//! Native surface ripple passes and fence-local streaming resources.

mod frame;
mod types;

pub(in crate::device) use frame::RippleFrameResources;
pub use types::{WaterRippleFrame, WaterRippleFrameError, WaterRipplePass};
