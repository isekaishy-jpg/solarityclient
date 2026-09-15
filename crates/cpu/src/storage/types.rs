//! Explicit byte limits and accounting categories, independent of machine policy.

/// Separately protected admission allowances. Speculation cannot spend frame bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum CpuStorageClass {
    /// Storage needed to prepare and publish interactive frames.
    Frame,
    /// Storage for required resource loading and service work.
    Required,
    /// Optional prefetch storage that cannot consume other allowances.
    Speculative,
}

/// Ownership purpose, separate from execution eligibility and resource identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum CpuStorageKind {
    /// Scheduler nodes, queues, edges and readiness slots.
    Metadata,
    /// Exclusively owned transient domain work storage.
    Scratch,
    /// Retained output pinned by its producer or consumers.
    Result,
}

/// Application-supplied logical capacity limits; zero disables a class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CpuStoragePlan {
    pub(super) limits: [usize; 3],
}
impl CpuStoragePlan {
    /// Defines separate frame, required-load and speculative byte allowances.
    #[must_use]
    pub const fn new(frame_bytes: usize, required_bytes: usize, speculative_bytes: usize) -> Self {
        Self {
            limits: [frame_bytes, required_bytes, speculative_bytes],
        }
    }
    /// Configured byte allowance for one admission class.
    #[must_use]
    pub const fn limit(self, class: CpuStorageClass) -> usize {
        self.limits[class as usize]
    }
}

/// Ledger-consistent capacity and peak observations, not an OS working-set estimate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CpuStorageSnapshot {
    pub(super) limits: [usize; 3],
    pub(super) used: [[usize; 3]; 3],
    pub(super) peaks: [usize; 3],
}
impl CpuStorageSnapshot {
    /// Total currently retained capacity in a class.
    #[must_use]
    pub fn used(self, class: CpuStorageClass) -> usize {
        self.used[class as usize].iter().sum()
    }
    /// Retained capacity attributed to one ownership purpose.
    #[must_use]
    pub const fn bytes(self, class: CpuStorageClass, kind: CpuStorageKind) -> usize {
        self.used[class as usize][kind as usize]
    }
    /// Highest simultaneously reserved capacity, including replacement growth.
    #[must_use]
    pub const fn peak(self, class: CpuStorageClass) -> usize {
        self.peaks[class as usize]
    }
    /// Configured byte allowance for one admission class.
    #[must_use]
    pub const fn limit(self, class: CpuStorageClass) -> usize {
        self.limits[class as usize]
    }
}
