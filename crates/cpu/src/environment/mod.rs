//! Worker startup matches the coordinator's numeric controls without changing kernels.

use std::cell::Cell;

thread_local! {
    /// Pool workers may not synchronously join unfinished work on this pool.
    static WORKER: Cell<bool> = const { Cell::new(false) };
}

/// Captured control state used by Rust's x86-64 scalar/SIMD floating point.
#[derive(Clone, Copy)]
pub(crate) struct WorkerEnvironment {
    #[cfg(target_arch = "x86_64")]
    mxcsr: u32,
}

impl WorkerEnvironment {
    /// Records rounding, exception masks and denormal modes, excluding status flags.
    #[allow(unsafe_code)]
    pub(crate) fn capture() -> Self {
        #[cfg(target_arch = "x86_64")]
        {
            let mut mxcsr = 0u32;
            // SAFETY: x86-64 guarantees SSE; the output points to writable u32
            // storage. STMXCSR only copies this thread's control/status register.
            unsafe {
                std::arch::asm!("stmxcsr [{address}]", address = in(reg) &mut mxcsr, options(nostack, preserves_flags));
            }
            Self {
                mxcsr: mxcsr & !0x3f,
            }
        }
        #[cfg(not(target_arch = "x86_64"))]
        Self {}
    }

    /// Establishes the exact captured controls before any kernel is admitted.
    #[allow(unsafe_code)]
    pub(crate) fn install(self) -> bool {
        #[cfg(target_arch = "x86_64")]
        {
            // SAFETY: this value came from STMXCSR on this process's supported
            // execution environment; only exception status flags were cleared.
            unsafe {
                std::arch::asm!("ldmxcsr [{address}]", address = in(reg) &self.mxcsr, options(nostack, preserves_flags));
            }
            if Self::capture().mxcsr != self.mxcsr {
                return false;
            }
        }
        WORKER.set(true);
        true
    }
}

/// Detects forbidden synchronous worker waits without consulting shared state.
pub fn is_worker() -> bool {
    WORKER.get()
}
