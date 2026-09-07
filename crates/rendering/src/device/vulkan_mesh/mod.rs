//! Renderer-owned device-local M2 geometry with deferred transfer retirement.

mod registry;
mod types;
mod upload;

pub use types::{M2MeshHandle, M2MeshResourceInfo};

pub(in crate::device) use registry::M2MeshRegistry;
pub(in crate::device) use upload::{
    DeferredMeshTransfer, GpuMeshBuffers, MeshUploadContext, upload_mesh_buffers,
    upload_mesh_buffers_deferred,
};
