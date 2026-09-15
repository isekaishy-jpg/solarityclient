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
    pub(crate) label: &'static str,
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
        self.record(
            generation(),
            "",
            value,
            crate::TraceContext::capture().is_sampled(),
        );
    }

    /// Publishes a delayed synchronous observation only to its original capture.
    pub fn value_for_generation(&'static self, epoch: u64, value: u64) {
        self.record(
            epoch,
            "",
            value,
            crate::TraceContext::capture().is_sampled(),
        );
    }

    /// Retains an infrequent causal value in the event stream on ordinary frames too.
    /// Callers must not use this for per-item logging in a hot traversal.
    pub fn event_value(&'static self, epoch: u64, value: u64) {
        if epoch != 0
            && generation() == epoch
            && let Some(metric) = self.metric("")
        {
            crate::TraceContext::capture().value(self.label, 0, 0, value);
            recorder::record(
                epoch,
                metric,
                crate::TraceContext::capture().is_sampled(),
                value,
                true,
            );
        }
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
        if epoch == 0 || generation() != epoch {
            return;
        }
        let context = crate::TraceContext::capture();
        context.timing(if phase.is_empty() { self.label } else { phase }, duration);
        self.record(epoch, phase, nanos(duration), context.is_sampled());
    }

    /// Resolves static identity outside the recorder's per-thread critical section.
    fn record(&'static self, epoch: u64, phase: &'static str, value: u64, lane: bool) {
        if epoch == 0 || generation() != epoch {
            return;
        }
        if self.unit == "value" && lane {
            crate::TraceContext::capture().value(self.label, 0, 0, value);
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
    trace: crate::TraceSpan,
}

impl Profile {
    /// Captures the enable state once; off scopes never touch thread-local storage.
    #[inline]
    pub fn new(site: &'static Site) -> Self {
        let epoch = generation();
        let context = if epoch == 0 {
            crate::TraceContext::default()
        } else {
            crate::TraceContext::capture()
        };
        let lane = epoch != 0 && context.is_sampled();
        let started = (epoch != 0 && (!site.detail || lane)).then(Instant::now);
        Self {
            site,
            epoch,
            started,
            previous: started,
            lane,
            marked: false,
            trace: crate::TraceSpan::start(
                site.label,
                context,
                if site.detail { None } else { started },
                [0; 3],
            ),
        }
    }

    /// Promotes one sampled inner operation to an owner trace using its existing timer.
    /// Owner IDs are interpreted within the originating frame, not as persistent GUIDs.
    pub fn trace_owner(&mut self, owner: u64, reason: u64) {
        if self.started.is_none() {
            return;
        }
        if self.trace.annotate(owner, reason) {
            return;
        }
        if self.started.is_some() && crate::TraceContext::capture().is_sampled() {
            self.trace = crate::TraceSpan::start(
                self.site.label,
                crate::TraceContext::capture(),
                self.started,
                [owner, reason, 0],
            );
        }
    }

    /// Accounts time since the previous mark; totals and phases must not be added.
    pub fn mark(&mut self, phase: &'static str) {
        let Some(previous) = self.previous else {
            return;
        };
        let now = Instant::now();
        self.trace.phase(phase, previous, now);
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
        if !self.lane {
            self.trace.slow(started, now);
        }
        self.trace.finish(now);
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
