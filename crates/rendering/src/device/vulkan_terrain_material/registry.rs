//! Plan-identity deduplication and device-local atlas ownership.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::TerrainTileMeshPlan;
use crate::device::VulkanError;
use crate::device::vulkan_texture::{GpuSampledImage, TextureUploadContext, upload_rgba8_image};

use super::{TerrainMaterialHandle, TerrainMaterialResourceInfo};

struct GpuTerrainMaterial {
    plan_identity: u64,
    image: GpuSampledImage,
    info: TerrainMaterialResourceInfo,
}

/// Owns one shared RGBA atlas for each immutable resident ADT plan.
pub(in crate::device) struct TerrainMaterialRegistry {
    registry_id: u64,
    handles: HashMap<u64, TerrainMaterialHandle>,
    resources: Vec<GpuTerrainMaterial>,
}

impl Default for TerrainMaterialRegistry {
    fn default() -> Self {
        static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            handles: HashMap::new(),
            resources: Vec::new(),
        }
    }
}

impl TerrainMaterialRegistry {
    pub(in crate::device) fn upload(
        &mut self,
        context: TextureUploadContext<'_>,
        plan: &TerrainTileMeshPlan,
    ) -> Result<TerrainMaterialHandle, VulkanError> {
        if let Some(handle) = self.handles.get(&plan.identity()) {
            return Ok(*handle);
        }
        let slot = u32::try_from(self.resources.len())
            .map_err(|_source| VulkanError::TerrainMaterialCapacity)?;
        let width = u32::try_from(crate::TERRAIN_MATERIAL_ATLAS_WIDTH)
            .map_err(|source| VulkanError::operation("convert terrain atlas width", source))?;
        let extent = (width, width);
        let image = upload_rgba8_image(context, extent, plan.material_atlas_rgba())?;
        let info =
            TerrainMaterialResourceInfo::new(plan.tile(), extent, plan.material_atlas_rgba().len());
        let handle = TerrainMaterialHandle {
            registry_id: self.registry_id,
            slot,
        };
        self.resources.push(GpuTerrainMaterial {
            plan_identity: plan.identity(),
            image,
            info,
        });
        self.handles.insert(plan.identity(), handle);
        Ok(handle)
    }

    pub(in crate::device) fn info(
        &self,
        handle: TerrainMaterialHandle,
    ) -> Option<TerrainMaterialResourceInfo> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
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
            .get(handle.slot as usize)
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
                .get(handle.slot as usize)
                .is_some_and(|resource| resource.plan_identity == plan.identity())
    }

    pub(in crate::device) fn destroy(
        &mut self,
        device: &ash::Device,
        allocator: &vk_mem::Allocator,
    ) {
        self.handles.clear();
        for resource in self.resources.iter_mut().rev() {
            resource.image.destroy(device, allocator);
        }
        self.resources.clear();
    }
}
