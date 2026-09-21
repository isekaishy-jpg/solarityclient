//! Ordered world command capture and parallel recording on the shared CPU executor.
mod batch;
mod command;
mod encode;
pub(in crate::device::vulkan_world_frame) use batch::{ScenePools, SceneRecording, begin_inline};
pub(in crate::device::vulkan_world_frame) use command::{SceneBindings, SceneCommand};
pub(in crate::device::vulkan_world_frame) use encode::end;
