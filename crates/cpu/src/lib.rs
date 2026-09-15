//! CPU execution and platform-capability boundaries.

mod capabilities;
mod environment;
mod job;
mod pool;
mod random;
mod reciprocal;
mod synchronization;

pub use capabilities::CpuCapabilities;
pub use pool::{
    CpuError, CpuExecutor, CpuPoolConfig, CpuPoolSnapshot, CpuTask, CpuTaskPermit, FrameBatch,
};
pub use random::BlizzardRand;
pub use reciprocal::reciprocal_sqrt_estimate;
