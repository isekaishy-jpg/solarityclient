//! Liquid geometry admission and fence-owned transfer/retirement lifetimes.

#![allow(unsafe_code)]

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};

use ash::{Device, vk};

use crate::LiquidRenderVertex;
use crate::device::VulkanError;
use crate::device::vulkan_mesh::{
    DeferredMeshTransfer, GpuMeshBuffers, MeshUploadContext, upload_mesh_buffers_deferred,
};

/// Renderer-local geometry identity, invalidated immediately when retired.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LiquidMeshHandle {
    registry: u64,
    slot: u32,
}

/// Permanent vertex/index storage and the validated unsigned-short draw extent.
struct LiquidMesh {
    buffers: GpuMeshBuffers,
    index_count: u32,
}

/// Detached meshes remain alive behind a submission following their final use.
struct RetiredMeshes {
    fence: vk::Fence,
    meshes: Vec<LiquidMesh>,
}

/// Owns liquid geometry independently of streamed CPU layer and WMO lifetimes.
pub(in crate::device) struct LiquidMeshRegistry {
    identity: u64,
    next_slot: u32,
    meshes: HashMap<u32, LiquidMesh>,
    transfers: Vec<DeferredMeshTransfer>,
    retired: VecDeque<RetiredMeshes>,
}

impl Default for LiquidMeshRegistry {
    /// Assigns renderer locality without allocating GPU resources.
    fn default() -> Self {
        static NEXT_IDENTITY: AtomicU64 = AtomicU64::new(1);
        Self {
            identity: NEXT_IDENTITY.fetch_add(1, Ordering::Relaxed),
            next_slot: 0,
            meshes: HashMap::new(),
            transfers: Vec::new(),
            retired: VecDeque::new(),
        }
    }
}

impl LiquidMeshRegistry {
    /// Validates the complete strip before submitting an asynchronous upload.
    pub(in crate::device) fn upload(
        &mut self,
        context: MeshUploadContext<'_>,
        vertices: &[LiquidRenderVertex],
        indices: &[u16],
    ) -> Result<LiquidMeshHandle, VulkanError> {
        self.collect(context.device, context.allocator)?;
        if vertices.is_empty() || indices.is_empty() {
            return Err(VulkanError::operation(
                "upload liquid mesh",
                "empty geometry",
            ));
        }
        if indices
            .iter()
            .any(|&index| usize::from(index) >= vertices.len())
        {
            return Err(VulkanError::operation(
                "upload liquid mesh",
                "index exceeds vertex storage",
            ));
        }
        let next_slot = self.next_slot.checked_add(1).ok_or_else(|| {
            VulkanError::operation("upload liquid mesh", "mesh identity capacity exhausted")
        })?;
        let index_count = u32::try_from(indices.len())
            .map_err(|source| VulkanError::operation("upload liquid mesh index extent", source))?;
        let vertex_bytes = vertices
            .iter()
            .flat_map(|vertex| vertex.to_bytes())
            .collect::<Vec<_>>();
        let index_bytes = indices
            .iter()
            .flat_map(|index| index.to_le_bytes())
            .collect::<Vec<_>>();
        let (buffers, transfer) =
            upload_mesh_buffers_deferred(context, &vertex_bytes, &index_bytes)?;
        let handle = LiquidMeshHandle {
            registry: self.identity,
            slot: self.next_slot,
        };
        self.meshes.insert(
            handle.slot,
            LiquidMesh {
                buffers,
                index_count,
            },
        );
        self.transfers.push(transfer);
        self.next_slot = next_slot;
        Ok(handle)
    }

    /// Returns live buffers only for a currently admitted identity in this renderer.
    pub(in crate::device) fn raw(
        &self,
        handle: LiquidMeshHandle,
    ) -> Option<(vk::Buffer, vk::Buffer, u32)> {
        if handle.registry != self.identity {
            return None;
        }
        self.meshes.get(&handle.slot).map(|mesh| {
            let (vertices, indices) = mesh.buffers.buffers();
            (vertices, indices, mesh.index_count)
        })
    }

    /// Invalidates handles after a successful ordered fence submission, without waiting.
    pub(in crate::device) fn retire(
        &mut self,
        context: MeshUploadContext<'_>,
        handles: &[LiquidMeshHandle],
    ) -> Result<(), VulkanError> {
        self.collect(context.device, context.allocator)?;
        if !handles.iter().any(|&handle| self.raw(handle).is_some()) {
            return Ok(());
        }
        // SAFETY: The live device owns this newly created, unsignaled fence.
        let fence = unsafe {
            context
                .device
                .create_fence(&vk::FenceCreateInfo::default(), None)
        }
        .map_err(|source| VulkanError::operation("create liquid retirement fence", source))?;
        // SAFETY: Exclusive renderer access serializes this empty submission
        // after all prior uploads and frames that can reference these meshes.
        if let Err(source) = unsafe {
            context
                .device
                .queue_submit(context.graphics_queue, &[], fence)
        } {
            // SAFETY: Failed submission retained no reference to the new fence.
            unsafe { context.device.destroy_fence(fence, None) };
            return Err(VulkanError::operation(
                "submit liquid retirement fence",
                source,
            ));
        }
        let meshes = handles
            .iter()
            .filter_map(|handle| {
                (handle.registry == self.identity)
                    .then(|| self.meshes.remove(&handle.slot))
                    .flatten()
            })
            .collect();
        self.retired.push_back(RetiredMeshes { fence, meshes });
        Ok(())
    }

    /// Reclaims completed uploads and retired meshes without stalling frame production.
    pub(in crate::device) fn collect(
        &mut self,
        device: &Device,
        allocator: &vk_mem::Allocator,
    ) -> Result<(), VulkanError> {
        let mut index = self.transfers.len();
        while index > 0 {
            index -= 1;
            if self.transfers[index].is_complete(device)? {
                self.transfers.swap_remove(index).destroy(device, allocator);
            }
        }
        while let Some(batch) = self.retired.front() {
            // SAFETY: Pending batches uniquely retain their submitted fences.
            let complete = unsafe { device.get_fence_status(batch.fence) }
                .map_err(|source| VulkanError::operation("poll liquid retirement fence", source))?;
            if !complete {
                break;
            }
            if let Some(batch) = self.retired.pop_front() {
                batch.destroy(device, allocator);
            }
        }
        Ok(())
    }

    /// Releases every child after renderer teardown has retired all queue work.
    pub(in crate::device) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        for mut transfer in self.transfers.drain(..) {
            transfer.destroy(device, allocator);
        }
        for batch in self.retired.drain(..) {
            batch.destroy(device, allocator);
        }
        for (_, mut mesh) in self.meshes.drain() {
            mesh.buffers.destroy(allocator);
        }
    }
}

impl RetiredMeshes {
    /// Destroys detached buffers only after the covering fence has completed.
    fn destroy(self, device: &Device, allocator: &vk_mem::Allocator) {
        for mut mesh in self.meshes {
            mesh.buffers.destroy(allocator);
        }
        // SAFETY: The caller has proved completion or device-wide teardown idle.
        unsafe { device.destroy_fence(self.fence, None) };
    }
}
