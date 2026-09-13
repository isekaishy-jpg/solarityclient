//! Typed UI ownership over the common device-local mesh transfer path.

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use ash::vk;

use crate::UiMeshPlan;
use crate::device::VulkanError;
use crate::device::vulkan_mesh::{
    DeferredMeshTransfer, GpuMeshBuffers, MeshUploadContext, upload_mesh_buffers_deferred,
};

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

/// Owns retained UI meshes and queue-fenced replaced allocations.
pub(in crate::device) struct UiMeshRegistry {
    registry_id: u64,
    resources: HashMap<u32, GpuUiMesh>,
    next_slot: u32,
    transfers: Vec<(DeferredMeshTransfer, Option<GpuMeshBuffers>)>,
}

impl Default for UiMeshRegistry {
    /// Assigns process-unique locality without allocating GPU resources.
    fn default() -> Self {
        static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            resources: HashMap::new(),
            next_slot: 0,
            transfers: Vec::new(),
        }
    }
}

impl UiMeshRegistry {
    /// On-demand content accounting, outside ordinary frame preparation.
    pub(in crate::device) fn usage(&self) -> (usize, usize) {
        (
            self.resources.len(),
            self.resources
                .values()
                .map(|resource| resource.vertex_bytes.capacity() + resource.index_bytes.capacity())
                .sum(),
        )
    }

    /// Uploads one immutable presentation generation to device-local buffers.
    pub(in crate::device) fn upload(
        &mut self,
        context: MeshUploadContext<'_>,
        plan: &UiMeshPlan,
    ) -> Result<UiMeshHandle, VulkanError> {
        let slot = self.next_slot;
        self.next_slot = slot.checked_add(1).ok_or(VulkanError::UiMeshCapacity)?;
        self.retire_transfers(context)?;
        let (resource, transfer) = upload_ui_mesh(context, plan)?;
        self.resources.insert(slot, resource);
        self.transfers.push((transfer, None));
        Ok(UiMeshHandle {
            registry_id: self.registry_id,
            slot,
        })
    }

    /// Replaces one stable mesh slot, queuing a transfer only for capacity growth.
    ///
    /// Payloads that fit the retained allocation are copied by the next frame's
    /// graphics command buffer. Capacity growth remains rare and uses the
    /// queue-ordered allocation path, retiring old buffers at its transfer fence.
    pub(in crate::device) fn replace(
        &mut self,
        context: MeshUploadContext<'_>,
        handle: UiMeshHandle,
        plan: &UiMeshPlan,
    ) -> Result<(), VulkanError> {
        self.retire_transfers(context)?;
        if handle.registry_id != self.registry_id {
            return Err(VulkanError::UnknownUiMeshHandle);
        }
        let vertex_bytes = validated_update_bytes(plan.vertex_bytes())?;
        let index_bytes = validated_update_bytes(plan.index_bytes())?;
        let resource = self
            .resources
            .get_mut(&handle.slot)
            .ok_or(VulkanError::UnknownUiMeshHandle)?;
        if vertex_bytes.len() > resource.vertex_bytes.capacity()
            || index_bytes.len() > resource.index_bytes.capacity()
        {
            let (replacement, transfer) = upload_ui_mesh(context, plan)?;
            let previous = std::mem::replace(resource, replacement);
            self.transfers.push((transfer, Some(previous.buffers)));
            return Ok(());
        }

        let retained_identity = resource.info.plan_identity();
        let vertex_update = if resource.vertex_bytes.len() == vertex_bytes.len() {
            plan.vertex_update_range_since(retained_identity)
                .and_then(|range| {
                    replace_payload_range(&mut resource.vertex_bytes, vertex_bytes, range)
                })
                .or_else(|| replace_payload(&mut resource.vertex_bytes, vertex_bytes))
        } else {
            replace_payload(&mut resource.vertex_bytes, vertex_bytes)
        };
        if let Some(range) = vertex_update {
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

    /// Nonblocking staging/old-allocation retirement after queue completion.
    pub(in crate::device) fn retire_transfers(
        &mut self,
        context: MeshUploadContext<'_>,
    ) -> Result<(), VulkanError> {
        for index in (0..self.transfers.len()).rev() {
            if self.transfers[index].0.is_complete(context.device)? {
                let (mut transfer, buffers) = self.transfers.swap_remove(index);
                transfer.destroy(context.device, context.allocator);
                if let Some(mut buffers) = buffers {
                    buffers.destroy(context.allocator);
                }
            }
        }
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
            .get(&handle.slot)
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

    /// Removes the stable slot only after its final prepared CPU frame departs.
    pub(in crate::device) fn take(&mut self, handle: UiMeshHandle) -> Option<GpuMeshBuffers> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .remove(&handle.slot)
            .map(|resource| resource.buffers)
    }

    /// Returns diagnostics for one renderer-local mesh generation.
    pub(in crate::device) fn info(&self, handle: UiMeshHandle) -> Option<UiMeshResourceInfo> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(&handle.slot)
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
            .get(&handle.slot)
            .map(|resource| resource.buffers.buffers())
    }

    /// Releases every immutable generation before the VMA parent.
    pub(in crate::device) fn destroy(
        &mut self,
        device: &ash::Device,
        allocator: &vk_mem::Allocator,
    ) {
        for (mut transfer, buffers) in self.transfers.drain(..) {
            transfer.destroy(device, allocator);
            if let Some(mut buffers) = buffers {
                buffers.destroy(allocator);
            }
        }
        for resource in self.resources.values_mut() {
            resource.buffers.destroy(allocator);
        }
        self.resources.clear();
    }
}

fn upload_ui_mesh(
    context: MeshUploadContext<'_>,
    plan: &UiMeshPlan,
) -> Result<(GpuUiMesh, DeferredMeshTransfer), VulkanError> {
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
    let (buffers, transfer) = upload_mesh_buffers_deferred(context, &vertex_bytes, &index_bytes)?;
    vertex_bytes.clear();
    index_bytes.clear();
    vertex_bytes.extend_from_slice(logical_vertex_bytes);
    index_bytes.extend_from_slice(logical_index_bytes);
    Ok((
        GpuUiMesh {
            buffers,
            info,
            vertex_bytes,
            index_bytes,
            pending_vertex_update: Cell::new(None),
            pending_index_update: Cell::new(None),
        },
        transfer,
    ))
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
    let final_difference = if candidate.len() > current.len() {
        // A changed prefix and a new tail belong to the same pending upload.
        // Searching only the old length would leave the appended bytes zeroed.
        candidate.len()
    } else {
        current[..common_length]
            .iter()
            .zip(&candidate[..common_length])
            .rposition(|(left, right)| left != right)
            .map_or(candidate.len(), |index| index + 1)
            .max(first_difference.min(candidate.len()))
    };
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

/// Copies one journal-proven changed span without scanning the full mesh.
fn replace_payload_range(
    current: &mut [u8],
    candidate: &[u8],
    range: (usize, usize),
) -> Option<(usize, usize)> {
    let (start, end) = range;
    if start >= end || end > current.len() || end > candidate.len() {
        return None;
    }
    current[start..end].copy_from_slice(&candidate[start..end]);
    Some(range)
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
#[path = "../../../tests/unit/ui_mesh_upload.rs"]
mod tests;
