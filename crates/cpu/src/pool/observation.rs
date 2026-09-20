//! Sampled scheduler observations publish only after releasing metadata locks.

use solarity_profiling::Site;
use std::{
    ops::{Deref, DerefMut},
    sync::{Condvar, Mutex, MutexGuard},
    time::{Duration, Instant},
};

/// Only selected detail frames start intervals; their endpoints may occur later.
#[derive(Clone, Copy)]
pub(super) struct SampleTime {
    epoch: u64,
    started: Instant,
}

impl SampleTime {
    /// Observe capture state before touching the monotonic clock.
    pub fn now() -> Option<Self> {
        let epoch = solarity_profiling::generation();
        (epoch != 0 && solarity_profiling::detail_enabled()).then(|| Self {
            epoch,
            started: Instant::now(),
        })
    }

    /// Capture the endpoint immediately; observer publication is not measured.
    pub fn finish(self, site: &'static Site) -> Measurement {
        Measurement {
            epoch: self.epoch,
            duration: self.started.elapsed(),
            site,
        }
    }
}

/// A completed interval can cross a lock-release boundary without extending it.
pub(super) struct Measurement {
    epoch: u64,
    duration: Duration,
    site: &'static Site,
}

impl Measurement {
    /// Reject old capture generations through the profiler's existing boundary.
    pub fn report(self) {
        self.site.duration(self.epoch, "", self.duration);
    }
}

/// Records acquisition, excluding ownership duration and condition waiting.
/// The optional interval is emitted after unlock, including early-return paths.
pub(super) struct ObservedGuard<'a, T> {
    guard: Option<MutexGuard<'a, T>>,
    acquisition: Option<Measurement>,
}

impl<'a, T> ObservedGuard<'a, T> {
    /// A publisher can unlock and notify before emitting observer records.
    pub fn unlock(mut self) -> Option<Measurement> {
        drop(self.guard.take());
        self.acquisition.take()
    }

    /// Preserve the native mutex contract, timing only its acquisition.
    pub fn lock(mutex: &'a Mutex<T>, site: &'static Site) -> Self {
        let started = SampleTime::now();
        let guard = mutex
            .lock()
            .unwrap_or_else(|_| unreachable!("scheduler metadata mutations cannot panic"));
        let acquisition = started.map(|start| start.finish(site));
        Self {
            guard: Some(guard),
            acquisition,
        }
    }

    /// Preserve the existing atomic unlock/park protocol. Reacquisition belongs
    /// to the separately instrumented condition wait, not this acquisition.
    pub fn wait(mut self, ready: &Condvar) -> Self {
        let guard = self
            .guard
            .take()
            .unwrap_or_else(|| unreachable!("live observed guard owns its mutex"));
        self.guard = Some(
            ready
                .wait(guard)
                .unwrap_or_else(|_| unreachable!("scheduler metadata mutations cannot panic")),
        );
        self
    }
}

impl<T> Deref for ObservedGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.guard
            .as_deref()
            .unwrap_or_else(|| unreachable!("live observed guard owns its mutex"))
    }
}

impl<T> DerefMut for ObservedGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard
            .as_deref_mut()
            .unwrap_or_else(|| unreachable!("live observed guard owns its mutex"))
    }
}

impl<T> Drop for ObservedGuard<'_, T> {
    fn drop(&mut self) {
        drop(self.guard.take());
        if let Some(acquisition) = self.acquisition.take() {
            acquisition.report();
        }
    }
}
