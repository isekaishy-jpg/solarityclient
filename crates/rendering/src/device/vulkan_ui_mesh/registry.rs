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
        let slot =
            u32::try_from(self.resources.len()).map_err(|_source| VulkanError::UiMeshCapacity)?;
        self.resources.push(upload_ui_mesh(context, plan)?);
        Ok(UiMeshHandle {
            registry_id: self.registry_id,
            slot,
        })
    }

    /// Replaces one stable mesh slot after the graphics queue retires prior use.
    pub(in crate::device) fn replace(
        &mut self,
        context: MeshUploadContext<'_>,
        handle: UiMeshHandle,
        plan: &UiMeshPlan,
    ) -> Result<(), VulkanError> {
        if handle.registry_id != self.registry_id {
            return Err(VulkanError::UnknownUiMeshHandle);
        }
        let slot =
            usize::try_from(handle.slot).map_err(|_source| VulkanError::UnknownUiMeshHandle)?;
        if slot >= self.resources.len() {
            return Err(VulkanError::UnknownUiMeshHandle);
        }
        let allocator = context.allocator;
        let replacement = upload_ui_mesh(context, plan)?;
        // Upload uses the same graphics queue and waits for its fence, so every
        // earlier frame referencing this stable slot has retired at this point.
        let mut previous = std::mem::replace(&mut self.resources[slot], replacement);
        previous.buffers.destroy(allocator);
        Ok(())
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

    /// Resolves one renderer-local identity to its live vertex/index buffers.
    pub(in crate::device) fn buffers(
        &self,
        handle: UiMeshHandle,
    ) -> Option<(ash::vk::Buffer, ash::vk::Buffer)> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|resource| resource.buffers.buffers())
    }

    /// Releases every immutable generation before the VMA parent.
    pub(in crate::device) fn destroy(&mut self, allocator: &vk_mem::Allocator) {
        for resource in self.resources.iter_mut().rev() {
            resource.buffers.destroy(allocator);
        }
        self.resources.clear();
    }
}

fn upload_ui_mesh(
    context: MeshUploadContext<'_>,
    plan: &UiMeshPlan,
) -> Result<GpuUiMesh, VulkanError> {
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
    Ok(GpuUiMesh { buffers, info })
}
