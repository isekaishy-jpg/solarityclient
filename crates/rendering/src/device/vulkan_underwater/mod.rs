//! Native camera-relative underwater billboards, submitted after the world queues.

mod frame;
mod types;

pub(in crate::device) use frame::UnderwaterFrameResources;
pub use types::{UnderwaterParticleFog, UnderwaterParticleFrame, UnderwaterParticleFrameError};
