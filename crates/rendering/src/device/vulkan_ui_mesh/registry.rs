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
    /// Byte ranges awaiting insertion into the next graphics submission.
    pending_vertex_update: Cell<Option<(usize, usize)>>,
    pending_index_update: Cell<Option<(usize, usize)>>,
}

/// Borrowed changed payloads consumed by the next graphics command buffer.
pub(in crate::device) struct UiMeshUpdates<'a> {
    pub(in crate::device) vertex: Option<(vk::Buffer, vk::DeviceSize, &'a [u8])>,
    pub(in crate::device) index: Option<(vk::Buffer, vk::DeviceSize, &'a [u8])>,
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
        let vertex_bytes = validated_update_bytes(plan.vertex_bytes())?;
        let index_bytes = validated_update_bytes(plan.index_bytes())?;
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

        if let Some(range) = replace_payload(&mut resource.vertex_bytes, vertex_bytes) {
            resource
                .pending_vertex_update
                .set(merge_update(resource.pending_vertex_update.get(), range));
        }
        if let Some(range) = replace_payload(&mut resource.index_bytes, index_bytes) {
            resource
                .pending_index_update
                .set(merge_update(resource.pending_index_update.get(), range));
        }
        resource.info = mesh_info(plan);
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
        let (vertex_buffer, index_buffer) = resource.buffers.buffers();
        let vertex = take_update(
            &resource.pending_vertex_update,
            vertex_buffer,
            &resource.vertex_bytes,
        );
        let index = take_update(
            &resource.pending_index_update,
            index_buffer,
            &resource.index_bytes,
        );
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
        pending_vertex_update: Cell::new(None),
        pending_index_update: Cell::new(None),
    })
}

/// Copies only the aligned changed span into retained command-source bytes.
fn replace_payload(current: &mut Vec<u8>, candidate: &[u8]) -> Option<(usize, usize)> {
    let common_length = current.len().min(candidate.len());
    let first_difference = current[..common_length]
        .iter()
        .zip(&candidate[..common_length])
        .position(|(left, right)| left != right)
        .or((current.len() != candidate.len()).then_some(common_length));
    let first_difference = first_difference?;
    let final_difference = current[..common_length]
        .iter()
        .zip(&candidate[..common_length])
        .rposition(|(left, right)| left != right)
        .map_or(candidate.len(), |index| index + 1)
        .max(candidate.len().min(current.len()).min(first_difference));
    let start = first_difference & !3;
    let end = final_difference.checked_add(3).map(|end| end & !3)?;
    current.resize(candidate.len(), 0);
    let end = end.min(candidate.len());
    if start < end {
        current[start..end].copy_from_slice(&candidate[start..end]);
        Some((start, end))
    } else {
        None
    }
}

fn merge_update(
    pending: Option<(usize, usize)>,
    current: (usize, usize),
) -> Option<(usize, usize)> {
    Some(pending.map_or(current, |pending| {
        (pending.0.min(current.0), pending.1.max(current.1))
    }))
}

fn take_update<'a>(
    pending: &Cell<Option<(usize, usize)>>,
    buffer: vk::Buffer,
    bytes: &'a [u8],
) -> Option<(vk::Buffer, vk::DeviceSize, &'a [u8])> {
    let (start, end) = pending.take()?;
    let end = end.min(bytes.len());
    (start < end).then_some((buffer, start as vk::DeviceSize, &bytes[start..end]))
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

/// Validates the inline-update ABI without allocating a duplicate payload.
fn validated_update_bytes(bytes: &[u8]) -> Result<&[u8], VulkanError> {
    if !bytes.len().is_multiple_of(4) {
        return Err(VulkanError::operation(
            "align UI mesh update",
            "payload is not four-byte aligned",
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::replace_payload;

    #[test]
    fn retained_payload_limits_updates_to_aligned_changed_bytes() {
        let mut retained = (0_u8..32).collect::<Vec<_>>();
        let mut candidate = retained.clone();
        candidate[10] = 200;
        candidate[13] = 201;

        assert_eq!(replace_payload(&mut retained, &candidate), Some((8, 16)));
        assert_eq!(retained, candidate);
        assert_eq!(replace_payload(&mut retained, &candidate), None);
    }

    #[test]
    fn retained_payload_handles_growth_and_logical_shrink() {
        let mut retained = vec![1_u8; 8];
        let candidate = vec![1_u8; 16];
        assert_eq!(replace_payload(&mut retained, &candidate), Some((8, 16)));
        assert_eq!(retained, candidate);

        assert_eq!(replace_payload(&mut retained, &[1_u8; 4]), None);
        assert_eq!(retained, [1_u8; 4]);
    }
}
