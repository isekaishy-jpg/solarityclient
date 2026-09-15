//! CPU execution and platform-capability boundaries.

mod capabilities;
mod completion;
mod environment;
mod job;
mod pool;
mod random;
mod reciprocal;
mod storage;
mod synchronization;

pub use storage::{
    ByteReservation, CpuBuffer, CpuResultLease, CpuResultPage, CpuStorageBudget, CpuStorageClass,
    CpuStorageKind, CpuStoragePlan, CpuStorageSnapshot, FixedWriter, OutputBuffer,
};

pub use capabilities::CpuCapabilities;
pub use completion::{CompletionPort, CompletionProducer, CoordinatorNotifier, ReadyToken};
pub use pool::{
    CpuError, CpuExecutor, CpuPoolConfig, CpuPoolSnapshot, CpuService, CpuTask, CpuTaskPermit,
    FrameBatch, FrameBatchPlan, FrameGraphTemplate, FrameJob, FramePriority, JobOutcome,
};
pub use random::BlizzardRand;
pub use reciprocal::reciprocal_sqrt_estimate;
