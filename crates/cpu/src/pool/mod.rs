//! Bounded worker-pool configuration and lifecycle.
//!
//! This module wraps the threading responsibility visible in the stock
//! `SThread.cpp` family. It owns protected/flexible worker creation and shutdown, not network
//! async execution or detached background work.

mod batch;
mod dispatch;
mod epochs;
mod executor;
mod task;
mod types;
mod worker;

pub use batch::{FrameBatch, FrameBatchPlan, FrameGraphTemplate, FrameJob, JobOutcome};
pub use executor::{CpuExecutor, CpuTaskPermit};
pub use task::CpuTask;
pub use types::{CpuError, CpuPoolConfig, CpuPoolSnapshot};
