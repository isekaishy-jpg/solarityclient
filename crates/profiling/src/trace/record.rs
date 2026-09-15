//! Fixed trace records contain static labels and numeric workload facts only.

use std::sync::OnceLock;
use std::time::Instant;

pub(crate) const TRACE_CAPACITY: usize = 32768;

/// Times share a process monotonic origin; GPU durations use a separate clock.
pub(crate) fn timestamp(now: Instant) -> u64 {
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    now.saturating_duration_since(*ORIGIN.get_or_init(|| now))
        .as_nanos()
        .min(u128::from(u64::MAX)) as u64
}

/// A span, dependency edge, or numeric observation owned by a sampled causal chain.
pub(crate) struct TraceRecord {
    pub(crate) id: u64,
    pub(crate) parent: u64,
    pub(crate) related: u64,
    pub(crate) origin_frame: u64,
    pub(crate) completion_frame: u64,
    pub(crate) started_ns: u64,
    pub(crate) duration_ns: u64,
    pub(crate) kind: &'static str,
    pub(crate) label: &'static str,
    pub(crate) owner: u64,
    pub(crate) reason: u64,
    pub(crate) value: u64,
    pub(crate) name: Option<String>,
}
