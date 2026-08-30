//! Renderer-owned device-local WMO geometry and stable resource identity.

mod registry;
mod types;

pub use types::{WorldModelMeshHandle, WorldModelMeshResourceInfo};

pub(in crate::device) use registry::WorldModelMeshRegistry;
