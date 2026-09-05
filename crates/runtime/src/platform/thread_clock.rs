//! Native CPU-cycle accounting for opt-in main-thread frame diagnostics.

#![allow(unsafe_code)]

/// Returns cycles charged to the calling thread, without a time conversion.
pub(crate) fn current_thread_cycles() -> Option<u64> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Threading::GetCurrentThread;
        use windows_sys::Win32::System::WindowsProgramming::QueryThreadCycleTime;
        let mut cycles = 0;
        // SAFETY: The pseudo-handle identifies this live thread and requires
        // no close. The output points to initialized, writable local storage.
        let succeeded = unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) };
        (succeeded != 0).then_some(cycles)
    }
    #[cfg(not(windows))]
    {
        None
    }
}
