//! Root-path deduplication and renderer-lifetime WMO buffer ownership.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use solarity_asset::AssetPath;

use crate::device::VulkanError;
use crate::device::vulkan_mesh::{
    DeferredMeshTransfer, GpuMeshBuffers, MeshUploadContext, upload_mesh_buffers_deferred,
};
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
    pending_transfers: Vec<DeferredMeshTransfer>,
}

impl Default for WorldModelMeshRegistry {
    fn default() -> Self {
        static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);

        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            handles: HashMap::new(),
            resources: Vec::new(),
            pending_transfers: Vec::new(),
        }
    }
}

impl WorldModelMeshRegistry {
    pub(in crate::device) fn upload(
        &mut self,
        context: MeshUploadContext<'_>,
        plan: &WorldModelMeshPlan,
    ) -> Result<WorldModelMeshHandle, VulkanError> {
        self.retire_completed_transfers(context.device, context.allocator)?;
        if let Some(handle) = self.handles.get(plan.path()) {
            return Ok(*handle);
        }
        let vertex_bytes = plan.vertex_upload_bytes();
        let index_bytes = plan.index_upload_bytes();
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
        let (buffers, transfer) =
            upload_mesh_buffers_deferred(context, &vertex_bytes, &index_bytes)?;
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
        self.pending_transfers.push(transfer);
        self.handles.insert(plan.path().clone(), handle);
        Ok(handle)
    }

    /// The transfer barriers precede draws on the same graphics queue. Polling
    /// the fence only controls staging lifetime and never waits for GPU work.
    pub(in crate::device) fn retire_completed_transfers(
        &mut self,
        device: &ash::Device,
        allocator: &vk_mem::Allocator,
    ) -> Result<(), VulkanError> {
        let mut index = self.pending_transfers.len();
        while index > 0 {
            index -= 1;
            if self.pending_transfers[index].is_complete(device)? {
                let mut transfer = self.pending_transfers.swap_remove(index);
                transfer.destroy(device, allocator);
            }
        }
        Ok(())
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

    /// The renderer has waited for device idle before releasing this registry.
    pub(in crate::device) fn destroy(
        &mut self,
        device: &ash::Device,
        allocator: &vk_mem::Allocator,
    ) {
        self.handles.clear();
        for transfer in self.pending_transfers.iter_mut().rev() {
            transfer.destroy(device, allocator);
        }
        self.pending_transfers.clear();
        for mut resource in self.resources.drain(..).rev() {
            resource.buffers.destroy(allocator);
        }
    }
}
