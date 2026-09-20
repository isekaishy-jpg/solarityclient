//! Startup capability discovery; domain kernels retain their existing ISA choices.

use std::num::NonZeroUsize;

/// Process-usable concurrency, distinguished from unknown physical topology.
#[derive(Clone, Copy, Debug)]
pub struct CpuCapabilities {
    available_workers: Option<NonZeroUsize>,
    architecture: &'static str,
    sse2: bool,
    avx2: bool,
}

impl CpuCapabilities {
    /// Queries the process execution limit without inventing a physical-core count.
    #[must_use]
    pub fn discover() -> Self {
        Self {
            available_workers: std::thread::available_parallelism().ok(),
            architecture: std::env::consts::ARCH,
            sse2: {
                #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
                {
                    std::is_x86_feature_detected!("sse2")
                }
                #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
                {
                    false
                }
            },
            avx2: {
                #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
                {
                    std::is_x86_feature_detected!("avx2")
                }
                #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
                {
                    false
                }
            },
        }
    }

    /// Returns usable logical concurrency, or unknown if platform discovery failed.
    #[must_use]
    pub const fn available_workers(self) -> Option<NonZeroUsize> {
        self.available_workers
    }
    /// Target architecture is reported separately from optional topology discovery.
    #[must_use]
    pub const fn architecture(self) -> &'static str {
        self.architecture
    }
    /// Process-usable SSE2 reported through Rust's platform feature detection.
    #[must_use]
    pub const fn sse2(self) -> bool {
        self.sse2
    }
    /// Includes the OS state support checked by Rust's feature detection. This
    /// report does not select a new kernel or change stock arithmetic.
    #[must_use]
    pub const fn avx2(self) -> bool {
        self.avx2
    }
}
