//! Per-thread aggregation never waits for the report writer.

mod aggregate;
mod registry;

pub(crate) use aggregate::Aggregate;
pub(crate) use registry::{
    EVENT_CAPACITY, Metric, Sample, Shard, overflow, overflows, record, record_trace, register,
    registry,
};

pub(crate) const METRIC_CAPACITY: usize = 1024;
pub(crate) const THREAD_CAPACITY: usize = 64;
pub(crate) const ROW_CAPACITY: usize = METRIC_CAPACITY * 2;
