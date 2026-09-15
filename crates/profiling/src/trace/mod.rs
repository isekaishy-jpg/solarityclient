//! Sampled causal contexts connect synchronous work, queued jobs and publication.

mod context;
mod record;
mod span;

pub use context::{TraceContext, TraceGuard};
pub(crate) use record::{TRACE_CAPACITY, TraceRecord, timestamp};
pub use span::TraceSpan;
