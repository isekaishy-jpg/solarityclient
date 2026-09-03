//! Typed UI ownership over the common device-local mesh transfer path.

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

use ash::vk;

use crate::UiMeshPlan;
use crate::device::VulkanError;
use crate::device::vulkan_mesh::{GpuMeshBuffers, MeshUploadContext, upload_mesh_buffers};

use super::{UiMeshHandle, UiMeshResourceInfo};

/// One uploaded UI vertex/index pair and its allocation diagnostics.
struct GpuUiMesh {
    buffers: GpuMeshBuffers,
    info: UiMeshResourceInfo,
    /// Four-byte-aligned logical payload within the retained vertex capacity.
    vertex_bytes: Vec<u8>,
    /// Four-byte-aligned logical payload within the retained index capacity.
    index_bytes: Vec<u8>,
    /// Buffer-kind mask awaiting insertion into the next graphics submission.
    pending_updates: Cell<u8>,
}

const VERTEX_UPDATE: u8 = 1;
const INDEX_UPDATE: u8 = 2;

/// Borrowed changed payloads consumed by the next graphics command buffer.
pub(in crate::device) struct UiMeshUpdates<'a> {
    pub(in crate::device) vertex: Option<(vk::Buffer, &'a [u8])>,
    pub(in crate::device) index: Option<(vk::Buffer, &'a [u8])>,
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

    /// Replaces one stable mesh slot without submitting independent queue work.
    ///
    /// Payloads that fit the retained allocation are copied by the next frame's
    /// graphics command buffer. Capacity growth remains rare and uses the
    /// synchronous allocation path before retiring the old buffers.
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
        let vertex_bytes = aligned_update_bytes(plan.vertex_bytes())?;
        let index_bytes = aligned_update_bytes(plan.index_bytes())?;
        let resource = &mut self.resources[slot];
        if vertex_bytes.len() > resource.vertex_bytes.capacity()
            || index_bytes.len() > resource.index_bytes.capacity()
        {
            let allocator = context.allocator;
            let replacement = upload_ui_mesh(context, plan)?;
            // Upload uses the same graphics queue and waits for its fence, so
            // every earlier frame referencing this stable slot has retired.
            let mut previous = std::mem::replace(resource, replacement);
            previous.buffers.destroy(allocator);
            return Ok(());
        }

        let mut pending_updates = 0;
        if resource.vertex_bytes != vertex_bytes {
            resource.vertex_bytes.clear();
            resource.vertex_bytes.extend_from_slice(&vertex_bytes);
            pending_updates |= VERTEX_UPDATE;
        }
        if resource.index_bytes != index_bytes {
            resource.index_bytes.clear();
            resource.index_bytes.extend_from_slice(&index_bytes);
            pending_updates |= INDEX_UPDATE;
        }
        resource.info = mesh_info(plan);
        resource
            .pending_updates
            .set(resource.pending_updates.get() | pending_updates);
        Ok(())
    }

    /// Takes pending host payloads for one ordered graphics command buffer.
    ///
    /// `vkCmdUpdateBuffer` embeds the bytes into command-buffer storage. Queue
    /// order therefore keeps prior draws ahead of the write and current draws
    /// behind it without a CPU fence wait or a staging-allocation lifetime.
    pub(in crate::device) fn take_pending_updates(
        &self,
        handle: UiMeshHandle,
    ) -> Result<UiMeshUpdates<'_>, VulkanError> {
        if handle.registry_id != self.registry_id {
            return Err(VulkanError::UnknownUiMeshHandle);
        }
        let resource = self
            .resources
            .get(handle.slot as usize)
            .ok_or(VulkanError::UnknownUiMeshHandle)?;
        let pending = resource.pending_updates.get();
        let (vertex_buffer, index_buffer) = resource.buffers.buffers();
        let vertex = (pending & VERTEX_UPDATE != 0)
            .then_some((vertex_buffer, resource.vertex_bytes.as_slice()));
        let index = (pending & INDEX_UPDATE != 0)
            .then_some((index_buffer, resource.index_bytes.as_slice()));
        resource.pending_updates.set(0);
        Ok(UiMeshUpdates { vertex, index })
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
    let logical_vertex_bytes = plan.vertex_bytes();
    let logical_index_bytes = plan.index_bytes();
    let info = UiMeshResourceInfo::new(
        plan.identity(),
        plan.vertices().len(),
        plan.indices().len(),
        logical_vertex_bytes.len(),
        logical_index_bytes.len(),
    );
    let mut vertex_bytes = retained_update_bytes(logical_vertex_bytes)?;
    let mut index_bytes = retained_update_bytes(logical_index_bytes)?;
    let buffers = upload_mesh_buffers(context, &vertex_bytes, &index_bytes)?;
    vertex_bytes.clear();
    index_bytes.clear();
    vertex_bytes.extend_from_slice(logical_vertex_bytes);
    index_bytes.extend_from_slice(logical_index_bytes);
    Ok(GpuUiMesh {
        buffers,
        info,
        vertex_bytes,
        index_bytes,
        pending_updates: Cell::new(0),
    })
}

fn mesh_info(plan: &UiMeshPlan) -> UiMeshResourceInfo {
    UiMeshResourceInfo::new(
        plan.identity(),
        plan.vertices().len(),
        plan.indices().len(),
        plan.vertices().len() * crate::UiRenderVertex::BYTE_SIZE,
        size_of_val(plan.indices()),
    )
}

/// Reserves geometric headroom while preserving the exact logical payload.
fn retained_update_bytes(bytes: &[u8]) -> Result<Vec<u8>, VulkanError> {
    let logical = aligned_update_bytes(bytes)?;
    let capacity = logical
        .len()
        .checked_next_power_of_two()
        .ok_or_else(|| VulkanError::operation("reserve UI mesh update", "size overflow"))?;
    let mut retained = Vec::with_capacity(capacity);
    retained.extend_from_slice(&logical);
    retained.resize(capacity, 0);
    Ok(retained)
}

fn aligned_update_bytes(bytes: &[u8]) -> Result<Vec<u8>, VulkanError> {
    let aligned = bytes
        .len()
        .checked_add(3)
        .map(|length| length & !3)
        .ok_or_else(|| VulkanError::operation("align UI mesh update", "size overflow"))?;
    let mut aligned_bytes = Vec::with_capacity(aligned);
    aligned_bytes.extend_from_slice(bytes);
    aligned_bytes.resize(aligned, 0);
    Ok(aligned_bytes)
}
