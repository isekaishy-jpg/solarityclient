//! Bounded external GPU completion; no CPU executor worker waits on a fence.

mod service;
mod state;

pub(super) use service::GpuCompletionService;
pub use state::GpuCompletion;

#[cfg(test)]
#[path = "../../../tests/device/gpu_completion.rs"]
mod tests;
