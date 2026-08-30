//! Path/profile deduplication and renderer-lifetime GPU resource ownership.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use solarity_asset::AssetPath;

use crate::device::VulkanError;
use crate::model::M2MeshPlan;

use super::types::{M2MeshHandle, M2MeshResourceInfo};
use super::upload::{GpuM2Mesh, MeshUploadContext, upload_mesh};

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
    resources: Vec<GpuM2Mesh>,
}

impl Default for M2MeshRegistry {
    /// Assigns a process-unique renderer identity for locality-safe handles.
    fn default() -> Self {
        static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);

        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            handles: HashMap::new(),
            resources: Vec::new(),
        }
    }
}

impl M2MeshRegistry {
    /// Returns an existing identity or performs one synchronous device upload.
    pub(in crate::device) fn upload(
        &mut self,
        context: MeshUploadContext<'_>,
        plan: &M2MeshPlan,
    ) -> Result<M2MeshHandle, VulkanError> {
        let key = M2MeshKey {
            path: plan.path().clone(),
            profile_index: plan.profile_index(),
        };
        if let Some(handle) = self.handles.get(&key) {
            return Ok(*handle);
        }

        // Check capacity before allocating so failure never leaves an orphaned
        // device resource that cannot receive a stable public handle.
        let slot =
            u32::try_from(self.resources.len()).map_err(|_source| VulkanError::M2MeshCapacity)?;
        let resource = upload_mesh(context, plan)?;
        let handle = M2MeshHandle {
            registry_id: self.registry_id,
            slot,
        };
        self.resources.push(resource);
        self.handles.insert(key, handle);
        Ok(handle)
    }

    /// Resolves renderer-local diagnostics without exposing Vulkan objects.
    pub(in crate::device) fn info(&self, handle: M2MeshHandle) -> Option<&M2MeshResourceInfo> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(GpuM2Mesh::info)
    }

    /// Resolves one renderer-local identity to its vertex and index buffers.
    pub(in crate::device) fn buffers(
        &self,
        handle: M2MeshHandle,
    ) -> Option<(ash::vk::Buffer, ash::vk::Buffer)> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(GpuM2Mesh::buffers)
    }

    /// Releases buffers in reverse upload order before their VMA parent.
    pub(in crate::device) fn destroy(&mut self, allocator: &vk_mem::Allocator) {
        self.handles.clear();
        for mut resource in self.resources.drain(..).rev() {
            resource.destroy(allocator);
        }
    }
}
