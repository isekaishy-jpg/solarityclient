//! CPU execution and platform-capability boundaries.

mod capabilities;
mod completion;
mod environment;
mod job;
mod pool;
mod random;
mod reciprocal;
mod synchronization;

pub use capabilities::CpuCapabilities;
pub use completion::{CompletionPort, CompletionProducer, CoordinatorNotifier, ReadyToken};
pub use pool::{
    CpuError, CpuExecutor, CpuPoolConfig, CpuPoolSnapshot, CpuTask, CpuTaskPermit, FrameBatch,
    FrameBatchPlan, FrameGraphTemplate, FrameJob, JobOutcome,
};
pub use random::BlizzardRand;
pub use reciprocal::reciprocal_sqrt_estimate;
