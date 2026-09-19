//! Bounded worker-pool configuration and lifecycle.
//!
//! This module wraps the threading responsibility visible in the stock
//! `SThread.cpp` family. It owns protected/flexible worker creation and shutdown, not network
//! async execution or detached background work.

mod batch;
mod demand;
mod dispatch;
mod epochs;
mod executor;
mod permit;
mod service;
mod task;
mod types;
mod worker;

pub use batch::{
    FrameBatch, FrameBatchPlan, FrameGraphTemplate, FrameJob, FramePriority, JobOutcome, LoadBatch,
};
pub use demand::{CpuServiceDemand, CpuServiceInterest};
pub use executor::CpuExecutor;
pub use permit::CpuTaskPermit;
pub use service::CpuService;
pub use task::{CpuServiceControl, CpuTask};
pub use types::{CpuError, CpuPoolConfig, CpuPoolSnapshot};
