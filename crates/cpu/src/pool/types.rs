//! Configuration, observable state, and failures for CPU execution.

use std::num::NonZeroUsize;

use thiserror::Error;

/// Explicit capacity for the application-owned CPU pool.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CpuPoolConfig {
    worker_count: NonZeroUsize,
    max_in_flight: NonZeroUsize,
}

impl CpuPoolConfig {
    /// Creates a pool configuration without guessing machine policy.
    ///
    /// The runtime composition layer is responsible for deriving these values
    /// from configuration and platform capabilities.
    #[must_use]
    pub const fn new(worker_count: NonZeroUsize, max_in_flight: NonZeroUsize) -> Self {
        Self {
            worker_count,
            max_in_flight,
        }
    }

    /// Returns the exact number of Rayon workers to create.
    #[must_use]
    pub const fn worker_count(self) -> NonZeroUsize {
        self.worker_count
    }

    /// Returns the maximum admitted running-plus-queued task count.
    #[must_use]
    pub const fn max_in_flight(self) -> NonZeroUsize {
        self.max_in_flight
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

/// A stable CPU-executor failure independent of Rayon internals.
#[derive(Debug, Error)]
pub enum CpuError {
    /// Rayon could not construct the requested private pool.
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
