//! Typed UI ownership over the common device-local mesh transfer path.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::UiMeshPlan;
use crate::device::VulkanError;
use crate::device::vulkan_mesh::{GpuMeshBuffers, MeshUploadContext, upload_mesh_buffers};

use super::{UiMeshHandle, UiMeshResourceInfo};

/// One uploaded UI vertex/index pair and its allocation diagnostics.
struct GpuUiMesh {
    buffers: GpuMeshBuffers,
    info: UiMeshResourceInfo,
}

/// Owns immutable UI mesh generations until renderer teardown.
pub(in crate::device) struct UiMeshRegistry {
    registry_id: u64,
    resources: Vec<GpuUiMesh>,
}

impl Default for UiMeshRegistry {
    /// Assigns process-unique locality without allocating GPU resources.
    fn default() -> Self {
        static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            resources: Vec::new(),
        }
    }
}

impl UiMeshRegistry {
    /// Uploads one immutable presentation generation to device-local buffers.
    pub(in crate::device) fn upload(
        &mut self,
        context: MeshUploadContext<'_>,
        plan: &UiMeshPlan,
    ) -> Result<UiMeshHandle, VulkanError> {
        if plan.vertices().is_empty() {
            return Err(VulkanError::EmptyUiMesh {
                buffer_kind: "vertex",
            });
        }
        if plan.indices().is_empty() {
            return Err(VulkanError::EmptyUiMesh {
                buffer_kind: "index",
            });
        }
        let slot =
            u32::try_from(self.resources.len()).map_err(|_source| VulkanError::UiMeshCapacity)?;
        let vertex_bytes = plan.vertex_bytes();
        let index_bytes = plan.index_bytes();
        let info = UiMeshResourceInfo::new(
            plan.identity(),
            plan.vertices().len(),
            plan.indices().len(),
            vertex_bytes.len(),
            index_bytes.len(),
        );
        let buffers = upload_mesh_buffers(context, &vertex_bytes, &index_bytes)?;
        self.resources.push(GpuUiMesh { buffers, info });
        Ok(UiMeshHandle {
            registry_id: self.registry_id,
            slot,
        })
    }

    /// Returns diagnostics for one renderer-local mesh generation.
    pub(in crate::device) fn info(&self, handle: UiMeshHandle) -> Option<UiMeshResourceInfo> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|resource| resource.info)
    }

    /// Releases every immutable generation before the VMA parent.
    pub(in crate::device) fn destroy(&mut self, allocator: &vk_mem::Allocator) {
        for resource in self.resources.iter_mut().rev() {
            resource.buffers.destroy(allocator);
        }
        self.resources.clear();
    }
}
