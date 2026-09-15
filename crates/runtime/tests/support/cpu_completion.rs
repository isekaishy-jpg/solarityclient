//! Completion fences for isolated background-service fixtures.

use solarity_cpu::CpuExecutor;
use std::{
    error::Error,
    time::{Duration, Instant},
};

/// The fixture must own admission exclusively and have no frame jobs. A sliced
/// task retains its admission through every yield, so an empty job alone is no
/// longer a completion fence. Wait for all logical work, then fence the single
/// flexible service worker's terminal publication epilogue. The timeout bounds
/// test failure; it does not determine when a generation is considered complete.
pub(crate) fn wait_for_background_service(cpu: &CpuExecutor) -> Result<(), Box<dyn Error>> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while cpu.snapshot()?.in_flight() != 0 {
        if Instant::now() >= deadline {
            return Err("isolated background service did not finish".into());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    // Admission is released just before terminal result publication. This job
    // cannot run until that publisher has finished on the flexible worker.
    cpu.try_submit(|| ())?.join()?;
    Ok(())
}
