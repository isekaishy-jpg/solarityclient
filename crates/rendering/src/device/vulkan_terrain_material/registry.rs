//! Plan-identity deduplication and device-local atlas ownership.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::TerrainTileMeshPlan;
use crate::device::VulkanError;
use crate::device::vulkan_texture::{
    DeferredTextureTransfer, GpuSampledImage, TextureUploadContext, upload_rgba8_image_deferred,
};

use super::{TerrainMaterialHandle, TerrainMaterialResourceInfo};

/// One ADT's blend weights and authored shadow opacity in linear byte space.
struct GpuTerrainMaterial {
    plan_identity: u64,
    image: GpuSampledImage,
    info: TerrainMaterialResourceInfo,
}

/// Owns one shared RGBA atlas for each immutable resident ADT plan.
pub(in crate::device) struct TerrainMaterialRegistry {
    registry_id: u64,
    handles: HashMap<u64, TerrainMaterialHandle>,
    resources: HashMap<u32, GpuTerrainMaterial>,
    next_slot: u32,
    pending_transfers: Vec<DeferredTextureTransfer>,
}

impl Default for TerrainMaterialRegistry {
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

impl TerrainMaterialRegistry {
    /// Publishes an immutable atlas behind queue-ordered image barriers.
    pub(in crate::device) fn upload(
        &mut self,
        context: TextureUploadContext<'_>,
        plan: &TerrainTileMeshPlan,
    ) -> Result<TerrainMaterialHandle, VulkanError> {
        self.retire_completed_transfers(context.device, context.allocator)?;
        if let Some(handle) = self.handles.get(&plan.identity()) {
            return Ok(*handle);
        }
        let slot = self.next_slot;
        let next_slot = slot
            .checked_add(1)
            .ok_or(VulkanError::TerrainMaterialCapacity)?;
        let width = u32::try_from(crate::TERRAIN_MATERIAL_ATLAS_WIDTH)
            .map_err(|source| VulkanError::operation("convert terrain atlas width", source))?;
        let extent = (width, width);
        let (image, transfer) =
            upload_rgba8_image_deferred(context, extent, plan.material_atlas_rgba())?;
        let info =
            TerrainMaterialResourceInfo::new(plan.tile(), extent, plan.material_atlas_rgba().len());
        let handle = TerrainMaterialHandle {
            registry_id: self.registry_id,
            slot,
        };
        self.resources.insert(
            slot,
            GpuTerrainMaterial {
                plan_identity: plan.identity(),
                image,
                info,
            },
        );
        self.next_slot = next_slot;
        self.handles.insert(plan.identity(), handle);
        self.pending_transfers.push(transfer);
        Ok(handle)
    }

    /// Retires completed staging even when its destination ADT has already departed.
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
        handle: TerrainMaterialHandle,
    ) -> Option<TerrainMaterialResourceInfo> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(&handle.slot)
            .map(|resource| resource.info)
    }

    pub(in crate::device) fn view(
        &self,
        handle: TerrainMaterialHandle,
    ) -> Option<ash::vk::ImageView> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(&handle.slot)
            .map(|resource| resource.image.view())
    }

    pub(in crate::device) fn matches_plan(
        &self,
        handle: TerrainMaterialHandle,
        plan: &TerrainTileMeshPlan,
    ) -> bool {
        handle.registry_id == self.registry_id
            && self
                .resources
                .get(&handle.slot)
                .is_some_and(|resource| resource.plan_identity == plan.identity())
    }

    /// Invalidates atlas lookup before its descriptor/image retirement is queued.
    pub(in crate::device) fn take_plan(
        &mut self,
        identity: u64,
    ) -> Option<(TerrainMaterialHandle, GpuSampledImage)> {
        let handle = self.handles.remove(&identity)?;
        self.resources
            .remove(&handle.slot)
            .map(|resource| (handle, resource.image))
    }

    /// Releases staging and images after the renderer has waited for device idle.
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
            resource.image.destroy(device, allocator);
        }
        self.resources.clear();
    }
}
