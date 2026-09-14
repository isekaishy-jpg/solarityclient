//! Static probe identity and frame sampling avoid hot-path formatting or allocation.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::recorder;

pub(crate) static ACTIVE: AtomicU64 = AtomicU64::new(0);
static FRAME: AtomicU64 = AtomicU64::new(0);
static DETAIL: AtomicBool = AtomicBool::new(false);
/// Detailed CPU and GPU probes run on one frame in this many live frames.
pub const DETAIL_INTERVAL: u64 = 128;
const PHASE_CAPACITY: usize = 32;

/// Returns the capture generation, or zero while disabled.
#[inline]
pub fn generation() -> u64 {
    ACTIVE.load(Ordering::Relaxed)
}

/// Reports whether any capture is accepting measurements.
#[inline]
pub fn enabled() -> bool {
    generation() != 0
}

/// Reports the shared detail-frame selection; never reads a clock.
#[inline]
pub fn detail_enabled() -> bool {
    enabled() && DETAIL.load(Ordering::Relaxed)
}

/// Completion-frame correlation is shared with worker and GPU observations.
pub(crate) fn frame_number() -> u64 {
    FRAME.load(Ordering::Relaxed)
}

/// Selects detail sampling once at the real application frame boundary.
/// Returns a complete-frame scope, classified separately from detail frames.
pub fn begin_frame() -> Profile {
    if enabled() {
        let next = FRAME.fetch_add(1, Ordering::Relaxed);
        DETAIL.store(next.is_multiple_of(DETAIL_INTERVAL), Ordering::Relaxed);
    }
    crate::profile!("frame.live")
}

/// One static call site and its bounded set of named phase metrics.
pub struct Site {
    label: &'static str,
    detail: bool,
    unit: &'static str,
    total: OnceLock<Option<usize>>,
    phases: [OnceLock<(&'static str, Option<usize>)>; PHASE_CAPACITY],
}

impl Site {
    /// Declares a duration site; `detail` selects sparse inner-work sampling.
    pub const fn new(label: &'static str, detail: bool) -> Self {
        Self {
            label,
            detail,
            unit: "ns",
            total: OnceLock::new(),
            phases: [const { OnceLock::new() }; PHASE_CAPACITY],
        }
    }

    /// Declares values such as bytes, draws, voices, queue depth or resident owners.
    pub const fn counter(label: &'static str) -> Self {
        let mut site = Self::new(label, false);
        site.unit = "value";
        site
    }

    /// Registers each phase once. Phase identity follows its name, not branch order.
    fn metric(&'static self, phase: &'static str) -> Option<usize> {
        if phase.is_empty() {
            return *self
                .total
                .get_or_init(|| recorder::register(self.label, phase, self.unit, self.detail));
        }
        for cell in &self.phases {
            let &(name, index) = cell.get_or_init(|| {
                (
                    phase,
                    recorder::register(self.label, phase, self.unit, self.detail),
                )
            });
            if name == phase {
                return index;
            }
        }
        recorder::overflow();
        None
    }

    /// Adds a workload observation to the current capture.
    pub fn value(&'static self, value: u64) {
        self.record(generation(), "", value, DETAIL.load(Ordering::Relaxed));
    }

    /// Records an asynchronous duration only in the generation that submitted it.
    /// Used by retired GPU queries and queue-residence measurements.
    pub fn duration(&'static self, epoch: u64, phase: &'static str, duration: Duration) {
        self.record(
            epoch,
            phase,
            duration.as_nanos().min(u128::from(u64::MAX)) as u64,
            true,
        );
    }

    /// Records an existing synchronous CPU timer with this frame's sampling lane.
    pub fn cpu_duration(&'static self, epoch: u64, phase: &'static str, duration: Duration) {
        self.record(
            epoch,
            phase,
            nanos(duration),
            DETAIL.load(Ordering::Relaxed),
        );
    }

    /// Resolves static identity outside the recorder's per-thread critical section.
    fn record(&'static self, epoch: u64, phase: &'static str, value: u64, lane: bool) {
        if epoch == 0 || generation() != epoch {
            return;
        }
        if let Some(metric) = self.metric(phase) {
            let keep_event = (self.unit == "value" && lane)
                || (self.unit == "ns"
                    && ((self.label == "frame.live" && phase.is_empty()) || value >= 2_000_000));
            recorder::record(epoch, metric, lane, value, keep_event);
        }
    }
}

/// A synchronous scope. Do not retain this across `.await`; time polls instead.
pub struct Profile {
    site: &'static Site,
    epoch: u64,
    started: Option<Instant>,
    previous: Option<Instant>,
    lane: bool,
    marked: bool,
}

impl Profile {
    /// Captures the enable state once; off scopes never touch thread-local storage.
    #[inline]
    pub fn new(site: &'static Site) -> Self {
        let epoch = generation();
        let lane = epoch != 0 && DETAIL.load(Ordering::Relaxed);
        let started = (epoch != 0 && (!site.detail || lane)).then(Instant::now);
        Self {
            site,
            epoch,
            started,
            previous: started,
            lane,
            marked: false,
        }
    }

    /// Accounts time since the previous mark; totals and phases must not be added.
    pub fn mark(&mut self, phase: &'static str) {
        let Some(previous) = self.previous else {
            return;
        };
        let now = Instant::now();
        self.site.record(
            self.epoch,
            phase,
            nanos(now.duration_since(previous)),
            self.lane,
        );
        // Recorder bookkeeping belongs to the scope total, not the following phase.
        self.previous = Some(Instant::now());
        self.marked = true;
    }
}

impl Drop for Profile {
    fn drop(&mut self) {
        let Some(started) = self.started else { return };
        let now = Instant::now();
        if self.marked
            && let Some(previous) = self.previous
        {
            self.site.record(
                self.epoch,
                "unattributed tail",
                nanos(now.duration_since(previous)),
                self.lane,
            );
        }
        self.site.record(
            self.epoch,
            "",
            nanos(now.duration_since(started)),
            self.lane,
        );
    }
}

/// Saturates unusually long diagnostics without wrapping their aggregate value.
fn nanos(duration: Duration) -> u64 {
    duration.as_nanos().min(u128::from(u64::MAX)) as u64
}
