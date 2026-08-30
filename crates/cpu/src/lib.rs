//! CPU execution and platform-capability boundaries.

mod job;
mod pool;
mod synchronization;

pub use pool::{CpuError, CpuExecutor, CpuPoolConfig, CpuPoolSnapshot, CpuTask};
