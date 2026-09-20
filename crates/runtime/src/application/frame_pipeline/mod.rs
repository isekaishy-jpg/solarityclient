//! Main-owned frame consumption; workers never acquire platform or gameplay state.

mod gpu;
mod recording;
mod wait;

pub(super) use gpu::GpuFrameWaitError;
pub(super) use wait::{FrameWait, FrameWaitError};

#[cfg(test)]
#[path = "../../../tests/application/frame_wait.rs"]
mod tests;
