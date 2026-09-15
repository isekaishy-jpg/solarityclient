//! Registration is cold; steady records use a thread-owned shard and try-lock only.

use std::cell::OnceCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use super::{Aggregate, METRIC_CAPACITY, ROW_CAPACITY, THREAD_CAPACITY};

/// Static metadata is never reconstructed by the report writer from game state.
#[derive(Clone)]
pub(crate) struct Metric {
    pub(crate) scope: &'static str,
    pub(crate) phase: &'static str,
    pub(crate) unit: &'static str,
    pub(crate) detailed: bool,
}

/// One thread's capture generation and fixed-size accumulator array.
pub(crate) struct Data {
    pub(crate) epoch: u64,
    pub(crate) rows: Vec<Aggregate>,
    pub(crate) events: Vec<Sample>,
    pub(crate) traces: Vec<crate::trace::TraceRecord>,
    pub(crate) dropped_traces_start: u64,
    pub(crate) dropped_start: u64,
    pub(crate) dropped_events_start: u64,
}

/// Complete frames and slow scopes retain correlation without unbounded traces.
pub(crate) struct Sample {
    pub(crate) frame: u64,
    pub(crate) metric: usize,
    pub(crate) value: u64,
    pub(crate) detailed: bool,
}

pub(crate) const EVENT_CAPACITY: usize = 8192;

/// Writer swaps the whole row vector under a short lock, then formats outside it.
pub(crate) struct Shard {
    pub(crate) name: String,
    pub(crate) data: Mutex<Data>,
    pub(crate) dropped: AtomicU64,
    pub(crate) dropped_events: AtomicU64,
    pub(crate) dropped_traces: AtomicU64,
}

/// Bounded process metadata, shared only during cold registration and snapshots.
#[derive(Default)]
pub(crate) struct Registry {
    pub(crate) metrics: Vec<Metric>,
    pub(crate) shards: Vec<Arc<Shard>>,
}

static OVERFLOW: AtomicU64 = AtomicU64::new(0);
thread_local! {
    static LOCAL: OnceCell<Option<Arc<Shard>>> = const { OnceCell::new() };
}

/// Shared registry initialization never happens in a disabled probe.
pub(crate) fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Registry::default()))
}

/// Records capacity exhaustion without logging on the measured thread.
pub(crate) fn overflow() {
    OVERFLOW.fetch_add(1, Ordering::Relaxed);
}

/// Returns cumulative overflow evidence for capture health reporting.
pub(crate) fn overflows() -> u64 {
    OVERFLOW.load(Ordering::Relaxed)
}

/// Assigns a stable numeric slot once per static metric.
pub(crate) fn register(
    scope: &'static str,
    phase: &'static str,
    unit: &'static str,
    detailed: bool,
) -> Option<usize> {
    let Ok(mut registry) = registry().lock() else {
        overflow();
        return None;
    };
    if registry.metrics.len() == METRIC_CAPACITY {
        overflow();
        return None;
    }
    let index = registry.metrics.len();
    registry.metrics.push(Metric {
        scope,
        phase,
        unit,
        detailed,
    });
    Some(index)
}

/// Allocates each thread's bounded storage once, at its first enabled probe.
fn local_shard() -> Option<Arc<Shard>> {
    let thread = std::thread::current();
    let shard = Arc::new(Shard {
        name: format!(
            "{}:{:?}:os={}",
            thread.name().unwrap_or("unnamed"),
            thread.id(),
            crate::host::thread_id()
        ),
        data: Mutex::new(Data {
            epoch: 0,
            rows: vec![Aggregate::default(); ROW_CAPACITY],
            events: Vec::with_capacity(EVENT_CAPACITY),
            traces: Vec::with_capacity(crate::trace::TRACE_CAPACITY),
            dropped_traces_start: 0,
            dropped_start: 0,
            dropped_events_start: 0,
        }),
        dropped: AtomicU64::new(0),
        dropped_events: AtomicU64::new(0),
        dropped_traces: AtomicU64::new(0),
    });
    let Ok(mut registry) = registry().lock() else {
        overflow();
        return None;
    };
    if registry.shards.len() == THREAD_CAPACITY {
        overflow();
        return None;
    }
    registry.shards.push(Arc::clone(&shard));
    Some(shard)
}

/// A busy writer causes an explicitly counted lost sample, never a frame stall.
pub(crate) fn record(
    epoch: u64,
    metric: usize,
    detailed_frame: bool,
    value: u64,
    keep_event: bool,
) {
    LOCAL.with(|local| {
        let Some(shard) = local.get_or_init(local_shard) else {
            overflow();
            return;
        };
        let Ok(mut data) = shard.data.try_lock() else {
            shard.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        };
        if crate::generation() != epoch {
            return;
        }
        if data.epoch != epoch {
            data.dropped_start = shard.dropped.load(Ordering::Relaxed);
            data.dropped_events_start = shard.dropped_events.load(Ordering::Relaxed);
            data.rows.fill(Aggregate::default());
            data.events.clear();
            data.traces.clear();
            data.dropped_traces_start = shard.dropped_traces.load(Ordering::Relaxed);
            data.epoch = epoch;
        }
        data.rows[metric * 2 + usize::from(detailed_frame)].record(value);
        if keep_event {
            if data.events.len() < EVENT_CAPACITY {
                data.events.push(Sample {
                    frame: crate::scope::frame_number(),
                    metric,
                    value,
                    detailed: detailed_frame,
                });
            } else {
                shard.dropped_events.fetch_add(1, Ordering::Relaxed);
            }
        }
    });
}

/// Trace exhaustion or a concurrent writer never blocks a measured producer.
pub(crate) fn record_trace(epoch: u64, trace: crate::trace::TraceRecord) {
    LOCAL.with(|local| {
        let Some(shard) = local.get_or_init(local_shard) else {
            overflow();
            return;
        };
        let Ok(mut data) = shard.data.try_lock() else {
            shard.dropped_traces.fetch_add(1, Ordering::Relaxed);
            return;
        };
        if crate::generation() != epoch {
            return;
        }
        if data.epoch != epoch {
            data.dropped_start = shard.dropped.load(Ordering::Relaxed);
            data.dropped_events_start = shard.dropped_events.load(Ordering::Relaxed);
            data.dropped_traces_start = shard.dropped_traces.load(Ordering::Relaxed);
            data.rows.fill(Aggregate::default());
            data.events.clear();
            data.traces.clear();
            data.epoch = epoch;
        }
        if data.traces.len() < crate::trace::TRACE_CAPACITY {
            data.traces.push(trace);
        } else {
            shard.dropped_traces.fetch_add(1, Ordering::Relaxed);
        }
    });
}
