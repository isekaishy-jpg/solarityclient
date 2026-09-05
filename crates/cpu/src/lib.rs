//! CPU execution and platform-capability boundaries.

mod job;
mod pool;
mod random;
mod reciprocal;
mod synchronization;

pub use pool::{CpuError, CpuExecutor, CpuPoolConfig, CpuPoolSnapshot, CpuTask};
pub use random::BlizzardRand;
pub use reciprocal::reciprocal_sqrt_estimate;
