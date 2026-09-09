//! Immutable map-wide horizon upload and native fixed-function pass ownership.

mod pipeline;
mod resource;

pub(in crate::device) use pipeline::LowDetailPipelines;
pub(in crate::device) use resource::{LowDetailGpuMap, LowDetailRegistry};
