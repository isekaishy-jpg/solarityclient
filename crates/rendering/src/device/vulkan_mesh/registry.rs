//! Path/profile deduplication and renderer-lifetime GPU resource ownership.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use solarity_asset::AssetPath;

use crate::device::VulkanError;
use crate::model::M2MeshPlan;

use super::types::{M2MeshHandle, M2MeshResourceInfo};
use super::upload::{DeferredMeshTransfer, GpuM2Mesh, MeshUploadContext, upload_mesh};

/// Identity shared with the decoded M2 cache and explicit view selection.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct M2MeshKey {
    path: AssetPath,
    profile_index: usize,
}

/// Owns every shared M2 buffer until the parent renderer is torn down.
pub(in crate::device) struct M2MeshRegistry {
    registry_id: u64,
    handles: HashMap<M2MeshKey, M2MeshHandle>,
    resources: HashMap<u32, GpuM2Mesh>,
    next_slot: u32,
    pending_transfers: Vec<DeferredMeshTransfer>,
}

impl Default for M2MeshRegistry {
    /// Assigns a process-unique renderer identity for locality-safe handles.
    fn default() -> Self {
        static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);

        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            handles: HashMap::new(),
            resources: HashMap::new(),
            next_slot: 0,
            pending_transfers: Vec::new(),
        }
    }
}

impl M2MeshRegistry {
    /// On-demand content accounting, outside ordinary frame preparation.
    pub(in crate::device) fn usage(&self) -> (usize, usize) {
        (
            self.resources.len(),
            self.resources
                .values()
                .map(|resource| {
                    resource.info().vertex_byte_count() + resource.info().index_byte_count()
                })
                .sum(),
        )
    }

    /// Returns an existing identity or submits one device upload without waiting.
    pub(in crate::device) fn upload(
        &mut self,
        context: MeshUploadContext<'_>,
        plan: &M2MeshPlan,
    ) -> Result<M2MeshHandle, VulkanError> {
        self.retire_completed_transfers(context.device, context.allocator)?;
        let key = M2MeshKey {
            path: plan.path().clone(),
            profile_index: plan.profile_index(),
        };
        if let Some(handle) = self.handles.get(&key) {
            return Ok(*handle);
        }

        // Check capacity before allocating so failure never leaves an orphaned
        // device resource that cannot receive a stable public handle.
        let slot = self.next_slot;
        self.next_slot = slot.checked_add(1).ok_or(VulkanError::M2MeshCapacity)?;
        let (resource, transfer) = upload_mesh(context, plan)?;
        let handle = M2MeshHandle {
            registry_id: self.registry_id,
            slot,
        };
        self.resources.insert(slot, resource);
        self.pending_transfers.push(transfer);
        self.handles.insert(key, handle);
        Ok(handle)
    }

    /// Reclaims completed staging without waiting for unfinished GPU work.
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

    /// Detaches shared geometry when its final resident source disappears.
    pub(in crate::device) fn take(
        &mut self,
        handle: M2MeshHandle,
    ) -> Option<super::upload::GpuMeshBuffers> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        let resource = self.resources.remove(&handle.slot)?;
        self.handles.remove(&M2MeshKey {
            path: resource.info().path().clone(),
            profile_index: resource.info().profile_index(),
        });
        Some(resource.into_buffers())
    }

    /// Resolves renderer-local diagnostics without exposing Vulkan objects.
    pub(in crate::device) fn info(&self, handle: M2MeshHandle) -> Option<&M2MeshResourceInfo> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources.get(&handle.slot).map(GpuM2Mesh::info)
    }

    /// Resolves one renderer-local identity to its vertex and index buffers.
    pub(in crate::device) fn buffers(
        &self,
        handle: M2MeshHandle,
    ) -> Option<(ash::vk::Buffer, ash::vk::Buffer)> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources.get(&handle.slot).map(GpuM2Mesh::buffers)
    }

    /// Releases buffers in reverse upload order before their VMA parent.
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
        for mut resource in self.resources.drain().map(|(_, resource)| resource) {
            resource.destroy(allocator);
        }
    }
}
