//! Optional aggregate timing for the resident camera's collision providers.

use std::time::{Duration, Instant};

/// Static camera provider diagnostics, enabled by the shared capture gate.
pub(super) struct CameraProfile;
impl CameraProfile {
    /// Publishes the existing provider intervals without synchronous logging.
    pub(super) fn record(&mut self, sample: [Duration; 5]) {
        static SITE: solarity_profiling::Site =
            solarity_profiling::Site::new("camera.collision.providers", false);
        let epoch = solarity_profiling::generation();
        for (phase, duration) in ["terrain", "WMO", "M2", "liquid ray", "camera volume"]
            .into_iter()
            .zip(sample)
        {
            SITE.cpu_duration(epoch, phase, duration);
        }
    }
}

/// Measures one native provider only when the outer query requested diagnostics.
pub(super) fn measure<T>(
    sample: &mut Option<[Duration; 5]>,
    index: usize,
    query: impl FnOnce() -> T,
) -> T {
    let started = sample.as_ref().map(|_| Instant::now());
    let result = query();
    if let (Some(sample), Some(started)) = (sample, started) {
        sample[index] += started.elapsed();
    }
    result
}
