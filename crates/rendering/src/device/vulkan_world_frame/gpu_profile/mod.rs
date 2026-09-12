//! Sampled GPU intervals; query ownership follows the existing frame-slot fence.

mod profiler;
mod query;

pub(in crate::device) use profiler::GpuFrameProfiler;
pub(super) use profiler::QUERY_COUNT;
pub(super) use query::GpuTimestampSlot;
