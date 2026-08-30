//! Renderer-owned device-local M2 geometry and synchronous initial upload.

mod registry;
mod types;
mod upload;

pub use types::{M2MeshHandle, M2MeshResourceInfo};

pub(in crate::device) use registry::M2MeshRegistry;
pub(in crate::device) use upload::MeshUploadContext;
