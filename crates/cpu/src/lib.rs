//! CPU execution and platform-capability boundaries.

mod capabilities;
mod completion;
mod cost;
mod environment;
mod job;
mod pool;
mod random;
mod reciprocal;
mod storage;
mod synchronization;

pub use job::{JobContext, JobIdentity};
pub use storage::{
    ByteReservation, CpuBuffer, CpuOwnedCell, CpuResultLease, CpuResultPage, CpuStorageBudget,
    CpuStorageClass, CpuStorageKind, CpuStoragePlan, CpuStorageSnapshot, FixedWriter, OutputBuffer,
};
pub use storage::{CpuScratch, ScratchScope};

pub use capabilities::CpuCapabilities;
pub use completion::{
    CompletionPort, CompletionProducer, CoordinatorNotifier, MainReadyQueue, ProductOutcome,
    ProductPublisher, ReadyContinuation, ReadyToken, SharedProduct,
};
pub use cost::{CostCalibration, JobCost, WorkMeasurement};
pub use environment::is_worker as is_worker_thread;
pub use pool::{
    CpuError, CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuPoolSnapshot, CpuService,
    CpuServiceControl, CpuServiceDemand, CpuServiceInterest, CpuTask, CpuTaskPermit, FrameBatch,
    FrameBatchPlan, FrameGraphTemplate, FrameJob, FramePriority, JobOutcome, LoadBatch,
};
pub use random::BlizzardRand;
pub use reciprocal::reciprocal_sqrt_estimate;
