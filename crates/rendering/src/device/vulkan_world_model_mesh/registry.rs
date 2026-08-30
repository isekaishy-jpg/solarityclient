//! Root-path deduplication and renderer-lifetime WMO buffer ownership.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use solarity_asset::AssetPath;

use crate::device::VulkanError;
use crate::device::vulkan_mesh::{GpuMeshBuffers, MeshUploadContext, upload_mesh_buffers};
use crate::model::WorldModelMeshPlan;

use super::{WorldModelMeshHandle, WorldModelMeshResourceInfo};

struct GpuWorldModelMesh {
    buffers: GpuMeshBuffers,
    info: WorldModelMeshResourceInfo,
}

/// Owns each combined root/group WMO allocation until renderer teardown.
pub(in crate::device) struct WorldModelMeshRegistry {
    registry_id: u64,
    handles: HashMap<AssetPath, WorldModelMeshHandle>,
    resources: Vec<GpuWorldModelMesh>,
}

impl Default for WorldModelMeshRegistry {
    fn default() -> Self {
        static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);

        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            handles: HashMap::new(),
            resources: Vec::new(),
        }
    }
}

impl WorldModelMeshRegistry {
    pub(in crate::device) fn upload(
        &mut self,
        context: MeshUploadContext<'_>,
        plan: &WorldModelMeshPlan,
    ) -> Result<WorldModelMeshHandle, VulkanError> {
        if let Some(handle) = self.handles.get(plan.path()) {
            return Ok(*handle);
        }
        let vertex_bytes = plan.vertex_bytes();
        let index_bytes = plan.index_bytes();
        if vertex_bytes.is_empty() {
            return Err(VulkanError::EmptyWorldModelMesh {
                path: plan.path().clone(),
                buffer_kind: "vertex",
            });
        }
        if index_bytes.is_empty() {
            return Err(VulkanError::EmptyWorldModelMesh {
                path: plan.path().clone(),
                buffer_kind: "index",
            });
        }
        let slot = u32::try_from(self.resources.len())
            .map_err(|_source| VulkanError::WorldModelMeshCapacity)?;
        let buffers = upload_mesh_buffers(context, &vertex_bytes, &index_bytes)?;
        let info = WorldModelMeshResourceInfo::new(
            plan.path().clone(),
            plan.vertices().len(),
            plan.indices().len(),
            vertex_bytes.len(),
            index_bytes.len(),
        );
        let handle = WorldModelMeshHandle {
            registry_id: self.registry_id,
            slot,
        };
        self.resources.push(GpuWorldModelMesh { buffers, info });
        self.handles.insert(plan.path().clone(), handle);
        Ok(handle)
    }

    pub(in crate::device) fn info(
        &self,
        handle: WorldModelMeshHandle,
    ) -> Option<&WorldModelMeshResourceInfo> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|resource| &resource.info)
    }

    pub(in crate::device) fn buffers(
        &self,
        handle: WorldModelMeshHandle,
    ) -> Option<(ash::vk::Buffer, ash::vk::Buffer)> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|resource| resource.buffers.buffers())
    }

    pub(in crate::device) fn destroy(&mut self, allocator: &vk_mem::Allocator) {
        self.handles.clear();
        for mut resource in self.resources.drain(..).rev() {
            resource.buffers.destroy(allocator);
        }
    }
}
