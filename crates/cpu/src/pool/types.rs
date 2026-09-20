//! Configuration, observable state, and failures for CPU execution.

use std::num::NonZeroUsize;

use thiserror::Error;

/// Explicit capacity for the application-owned CPU pool.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CpuPoolConfig {
    execution: super::CpuExecutionPlan,
    max_in_flight: NonZeroUsize,
    storage: crate::CpuStoragePlan,
}

impl CpuPoolConfig {
    /// Creates a pool configuration without guessing machine policy.
    ///
    /// The runtime composition layer is responsible for deriving these values
    /// from configuration and platform capabilities.
    #[must_use]
    pub const fn new(
        execution: super::CpuExecutionPlan,
        max_in_flight: NonZeroUsize,
        storage: crate::CpuStoragePlan,
    ) -> Self {
        Self {
            execution,
            max_in_flight,
            storage,
        }
    }

    /// Returns the exact number of worker threads to create.
    #[must_use]
    pub const fn worker_count(self) -> NonZeroUsize {
        self.execution.worker_count()
    }

    /// The validated split is enforced unchanged at pool startup.
    #[must_use]
    pub const fn execution(self) -> super::CpuExecutionPlan {
        self.execution
    }

    /// Returns the maximum admitted running-plus-queued task count.
    #[must_use]
    pub const fn max_in_flight(self) -> NonZeroUsize {
        self.max_in_flight
    }
    /// Explicit byte admission policy, independent of the task-count bound.
    #[must_use]
    pub const fn storage(self) -> crate::CpuStoragePlan {
        self.storage
    }
}

/// A lock-consistent view of executor admission and load.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CpuPoolSnapshot {
    accepting: bool,
    in_flight: usize,
    max_in_flight: NonZeroUsize,
}

impl CpuPoolSnapshot {
    /// Creates one snapshot while the lifecycle lock is held.
    #[must_use]
    pub(crate) const fn new(
        accepting: bool,
        in_flight: usize,
        max_in_flight: NonZeroUsize,
    ) -> Self {
        Self {
            accepting,
            in_flight,
            max_in_flight,
        }
    }

    /// Returns whether new work may be admitted.
    #[must_use]
    pub const fn is_accepting(self) -> bool {
        self.accepting
    }

    /// Returns the number of admitted tasks that have not finished.
    #[must_use]
    pub const fn in_flight(self) -> usize {
        self.in_flight
    }

    /// Returns the configured admission bound.
    #[must_use]
    pub const fn max_in_flight(self) -> NonZeroUsize {
        self.max_in_flight
    }
}

/// A stable CPU-executor failure independent of scheduler internals.
#[derive(Debug, Error)]
pub enum CpuError {
    /// A foreign service boundary failed to restore approved floating-point controls.
    #[error("cpu worker numeric environment restoration failed")]
    WorkerEnvironment,
    /// Temporary storage belongs to a different executor generation.
    #[error("cpu worker scratch belongs to another executor")]
    WorkerScratchOwner,
    /// The same operation tried to borrow its worker lane recursively.
    #[error("cpu worker scratch lane is already borrowed")]
    WorkerScratchBorrowed,
    /// Worker/service counts must fit one explicit finite execution allowance.
    #[error("cpu execution plan has invalid worker or service counts")]
    InvalidExecutionPlan,
    /// A domain writer exceeded its preadmitted element count.
    #[error("cpu output needs {requested} elements with {available} available")]
    OutputCapacity {
        /// Additional elements requested by the producer.
        requested: usize,
        /// Remaining elements in the preallocated destination.
        available: usize,
    },
    /// Byte saturation preserves the caller's pending work and prior reservation.
    #[error("cpu storage needs {requested} bytes with {available} available in {class:?}")]
    StorageAtCapacity {
        /// Admission allowance that cannot satisfy the reservation.
        class: crate::CpuStorageClass,
        /// Additional logical capacity requested, in bytes.
        requested: usize,
        /// Unreserved bytes remaining in that class.
        available: usize,
    },
    /// Element counts cannot be represented as byte capacity.
    #[error("cpu storage byte size overflow")]
    StorageSizeOverflow,
    /// Allocation failed after byte admission; the reservation is returned.
    #[error("cpu storage allocation failed")]
    StorageAllocation,
    /// A phase lists the same resource generation more than once.
    #[error("readiness prerequisite is duplicated")]
    DuplicateReadiness,
    /// A template edge is duplicated, cyclic or outside its earlier-node prefix.
    #[error("frame graph dependency is invalid")]
    InvalidGraph,
    /// A binding must supply every input in its declared template.
    #[error("frame graph input count does not match its template")]
    GraphInputCount,
    /// A removed producer or recycled readiness generation was referenced.
    #[error("readiness generation is stale")]
    StaleReadiness,
    /// Reset or producer creation would invalidate active completion ownership.
    #[error("readiness generation still has active ownership")]
    ReadinessActive,
    /// The resource's declared subscriber allowance is exhausted.
    #[error("readiness subscriber capacity is exhausted")]
    ReadinessCapacity,
    /// A second terminal publication contradicts the first.
    #[error("readiness completion conflicts with its terminal outcome")]
    ReadinessConflict,
    /// A handle belongs to another batch or a reclaimed generation.
    #[error("CPU frame job identity is stale")]
    StaleJob,
    /// Reuse cannot wrap into an old valid handle.
    #[error("CPU frame epoch generation is exhausted")]
    EpochExhausted,
    /// A producer appended after closing its phase.
    #[error("CPU frame batch admission is closed")]
    BatchClosed,
    /// A terminal-phase wait cannot precede the last producer append.
    #[error("cpu batch producer is still open")]
    BatchOpen,
    /// A phase exceeded its reserved node or edge count.
    #[error("CPU frame batch exhausted its reserved node or edge capacity")]
    BatchCapacity,
    /// Metadata could not be allocated before ownership transfer.
    #[error("CPU frame batch storage could not be reserved")]
    BatchStorage,
    /// The owned job retains a domain failure for its consumer.
    #[error("CPU frame job reported failure")]
    JobFailed,
    /// The owner cancelled this node without discarding its input.
    #[error("CPU frame job was cancelled")]
    JobCancelled,
    /// A prerequisite failed or was cancelled.
    #[error("CPU frame job prerequisite did not succeed")]
    DependencyFailed,
    /// Worker joins would turn a dependency into a deadlock or steal unrelated work.
    #[error("CPU worker attempted a blocking result wait")]
    WorkerWait,
    /// A reusable batch still owns its preceding epoch.
    #[error("CPU frame batch is already active")]
    BatchActive,
    /// A result was requested without an admitted epoch.
    #[error("CPU frame batch is inactive")]
    BatchInactive,
    /// A result index does not belong to this batch.
    #[error("CPU frame job index is invalid")]
    InvalidJob,
    /// The requested worker threads could not be created.
    #[error("failed to create CPU worker pool: {message}")]
    PoolBuild {
        /// Dependency context without exposing its concrete error type.
        message: String,
    },
    /// Shutdown has closed task admission.
    #[error("CPU executor is shutting down")]
    ShuttingDown,
    /// Bounded admission rejected work rather than blocking the caller.
    #[error("CPU executor reached its in-flight task limit of {limit}")]
    AtCapacity {
        /// The configured running-plus-queued task limit.
        limit: NonZeroUsize,
    },
    /// A task panicked and its result cannot be produced.
    #[error("CPU task panicked")]
    TaskPanicked,
    /// The completion channel closed without a task outcome.
    #[error("CPU task completion was lost")]
    CompletionLost,
    /// Internal lifecycle state could not be observed consistently.
    #[error("CPU executor lifecycle state is unavailable")]
    StateUnavailable,
}
