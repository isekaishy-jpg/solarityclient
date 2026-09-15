//! Startup capability discovery; domain kernels retain their existing ISA choices.

use std::num::NonZeroUsize;

/// Process-usable concurrency, distinguished from unknown physical topology.
#[derive(Clone, Copy, Debug)]
pub struct CpuCapabilities {
    available_workers: Option<NonZeroUsize>,
}

impl CpuCapabilities {
    /// Queries the process execution limit without inventing a physical-core count.
    #[must_use]
    pub fn discover() -> Self {
        Self {
            available_workers: std::thread::available_parallelism().ok(),
        }
    }

    /// Returns usable logical concurrency, or unknown if platform discovery failed.
    #[must_use]
    pub const fn available_workers(self) -> Option<NonZeroUsize> {
        self.available_workers
    }
}
