//! Plan-identity deduplication and retained terrain buffer ownership.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::TerrainTileMeshPlan;
use crate::device::VulkanError;
use crate::device::vulkan_mesh::{
    DeferredMeshTransfer, GpuMeshBuffers, MeshUploadContext, upload_mesh_buffers_deferred,
};

use super::{TerrainMeshHandle, TerrainMeshResourceInfo};

/// Immutable ADT geometry retained until its last submitted frame completes.
struct GpuTerrainMesh {
    plan_identity: u64,
    buffers: GpuMeshBuffers,
    info: TerrainMeshResourceInfo,
}

/// Owns uploaded ADT geometry until explicit retirement or renderer teardown.
pub(in crate::device) struct TerrainMeshRegistry {
    registry_id: u64,
    handles: HashMap<u64, TerrainMeshHandle>,
    resources: HashMap<u32, GpuTerrainMesh>,
    next_slot: u32,
    pending_transfers: Vec<DeferredMeshTransfer>,
}

impl Default for TerrainMeshRegistry {
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

impl TerrainMeshRegistry {
    /// Publishes geometry behind queue-ordered input barriers without a host wait.
    pub(in crate::device) fn upload(
        &mut self,
        context: MeshUploadContext<'_>,
        plan: &TerrainTileMeshPlan,
    ) -> Result<TerrainMeshHandle, VulkanError> {
        let profile = std::env::var_os("SOLARITY_FRAME_TIMINGS").map(|_| std::time::Instant::now());
        self.retire_completed_transfers(context.device, context.allocator)?;
        let retired = profile.map(|start| start.elapsed());
        if let Some(handle) = self.handles.get(&plan.identity()) {
            return Ok(*handle);
        }
        if plan.vertices().is_empty() {
            return Err(VulkanError::EmptyTerrainMesh {
                buffer_kind: "vertex",
            });
        }
        if plan.indices().is_empty() {
            return Err(VulkanError::EmptyTerrainMesh {
                buffer_kind: "index",
            });
        }
        let slot = self.next_slot;
        let next_slot = slot
            .checked_add(1)
            .ok_or(VulkanError::TerrainMeshCapacity)?;
        let vertex_bytes = plan.vertex_bytes();
        let index_bytes = plan.index_bytes();
        let serialized = profile.map(|start| start.elapsed());
        let info = TerrainMeshResourceInfo::new(
            plan.tile(),
            plan.vertices().len(),
            plan.indices().len(),
            plan.chunks().len(),
            vertex_bytes.len(),
            index_bytes.len(),
        );
        let (buffers, transfer) =
            upload_mesh_buffers_deferred(context, &vertex_bytes, &index_bytes)?;
        let handle = TerrainMeshHandle {
            registry_id: self.registry_id,
            slot,
        };
        self.resources.insert(
            slot,
            GpuTerrainMesh {
                plan_identity: plan.identity(),
                buffers,
                info,
            },
        );
        self.next_slot = next_slot;
        self.handles.insert(plan.identity(), handle);
        self.pending_transfers.push(transfer);
        if let (Some(start), Some(retired), Some(serialized)) = (profile, retired, serialized) {
            tracing::info!(
                target: "solarity_rendering::device::mesh_upload",
                vertex_bytes = vertex_bytes.len(),
                index_bytes = index_bytes.len(),
                retirement_us = retired.as_secs_f64() * 1_000_000.0,
                serialization_us = (serialized - retired).as_secs_f64() * 1_000_000.0,
                upload_us = (start.elapsed() - serialized).as_secs_f64() * 1_000_000.0,
                "profiled terrain geometry admission"
            );
        }
        Ok(handle)
    }

    /// Reclaims staging independently of ADT retirement after its transfer fence signals.
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
        handle: TerrainMeshHandle,
    ) -> Option<TerrainMeshResourceInfo> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(&handle.slot)
            .map(|resource| resource.info)
    }

    pub(in crate::device) fn matches_plan(
        &self,
        handle: TerrainMeshHandle,
        plan: &TerrainTileMeshPlan,
    ) -> bool {
        handle.registry_id == self.registry_id
            && self
                .resources
                .get(&handle.slot)
                .is_some_and(|resource| resource.plan_identity == plan.identity())
    }

    pub(in crate::device) fn buffers(
        &self,
        handle: TerrainMeshHandle,
    ) -> Option<(ash::vk::Buffer, ash::vk::Buffer)> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(&handle.slot)
            .map(|resource| resource.buffers.buffers())
    }

    /// Invalidates the public handle while transferring allocations to a GPU fence owner.
    pub(in crate::device) fn take_plan(&mut self, identity: u64) -> Option<GpuMeshBuffers> {
        let handle = self.handles.remove(&identity)?;
        self.resources
            .remove(&handle.slot)
            .map(|resource| resource.buffers)
    }

    /// Releases staging and geometry after the renderer has waited for device idle.
    pub(in crate::device) fn destroy(
        &mut self,
        device: &ash::Device,
        allocator: &vk_mem::Allocator,
    ) {
        self.handles.clear();
        for mut transfer in self.pending_transfers.drain(..) {
            transfer.destroy(device, allocator);
        }
        for resource in self.resources.values_mut() {
            resource.buffers.destroy(allocator);
        }
        self.resources.clear();
    }
}
