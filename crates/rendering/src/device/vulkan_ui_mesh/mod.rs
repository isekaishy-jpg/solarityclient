//! Renderer-owned device-local UI geometry and typed resource handles.

mod registry;
mod types;

pub(in crate::device) use registry::UiMeshRegistry;
pub use types::{UiMeshHandle, UiMeshResourceInfo};
