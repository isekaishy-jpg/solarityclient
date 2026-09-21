//! Bounded external GPU completion; no CPU executor worker waits on a fence.

mod acquire;
mod operation;
mod service;
mod state;

pub(super) use acquire::acquire_image;
pub(super) use operation::{HostOperation, HostOutput};
pub(super) use service::GpuCompletionService;
pub use state::GpuCompletion;

#[cfg(test)]
#[path = "../../../tests/device/gpu_completion.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/device/gpu_completion_trace.rs"]
mod trace_tests;
