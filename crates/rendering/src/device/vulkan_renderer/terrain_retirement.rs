//! Nonblocking retirement of terrain resources after their final queue use.

use ash::{Device, vk};

use crate::TerrainTileMeshPlan;
use crate::device::VulkanError;
use crate::device::vulkan_mesh::GpuMeshBuffers;
use crate::device::vulkan_texture::GpuSampledImage;

use super::VulkanRenderer;

/// Owns invalidated resources behind a submission following their last use.
pub(super) struct TerrainRetirement {
    fence: vk::Fence,
    meshes: Vec<GpuMeshBuffers>,
    materials: Vec<GpuSampledImage>,
    descriptor_pools: Vec<vk::DescriptorPool>,
}

impl TerrainRetirement {
    /// Destroys a batch only after its fence has signaled or device teardown waited.
    pub(super) fn destroy(mut self, device: &Device, allocator: &vk_mem::Allocator) {
        // SAFETY: The caller proves all earlier graphics submissions completed.
        // These pools and this fence were removed from their original owners.
        unsafe {
            for pool in self.descriptor_pools {
                device.destroy_descriptor_pool(pool, None);
            }
            device.destroy_fence(self.fence, None);
        }
        for image in &mut self.materials {
            image.destroy(device, allocator);
        }
        for mesh in &mut self.meshes {
            mesh.destroy(allocator);
        }
    }
}

impl VulkanRenderer {
    /// Retires uploaded ADT buffers, atlases, and their terrain descriptors.
    ///
    /// Existing handles become invalid immediately. Already submitted frames
    /// finish before destruction; this method does not wait for the GPU. Shared
    /// diffuse textures and shader pipelines keep their separate cache lifetime.
    /// The next upload of a retired plan creates new handles.
    ///
    /// # Errors
    /// Returns [`VulkanError`] if fence creation or queue submission fails.
    pub fn retire_terrain_plans<'a>(
        &mut self,
        plans: impl Iterator<Item = &'a TerrainTileMeshPlan>,
    ) -> Result<(), VulkanError> {
        let identities = plans.map(TerrainTileMeshPlan::identity).collect::<Vec<_>>();
        if identities.is_empty() {
            return Ok(());
        }
        // SAFETY: This exclusive renderer owner has a live logical device.
        let fence = unsafe {
            self.device
                .create_fence(&vk::FenceCreateInfo::default(), None)
        }
        .map_err(|source| VulkanError::operation("create terrain retirement fence", source))?;
        // An empty submission's fence covers all earlier work on this queue,
        // including every swapchain slot that might still reference these ADTs.
        // SAFETY: Queue and unsignaled fence belong to this device; renderer
        // mutation serializes queue submissions and retirement publication.
        if let Err(source) = unsafe { self.device.queue_submit(self.graphics_queue, &[], fence) } {
            // SAFETY: Failed submission left the fence unowned by queue work.
            unsafe { self.device.destroy_fence(fence, None) };
            return Err(VulkanError::operation(
                "submit terrain retirement fence",
                source,
            ));
        }
        self.is_idle = false;
        let mut batch = TerrainRetirement {
            fence,
            meshes: Vec::with_capacity(identities.len()),
            materials: Vec::with_capacity(identities.len()),
            descriptor_pools: Vec::new(),
        };
        let mut material_handles = Vec::with_capacity(identities.len());
        for identity in identities {
            if let Some(mesh) = self.terrain_meshes.take_plan(identity) {
                batch.meshes.push(mesh);
            }
            if let Some((handle, image)) = self.terrain_materials.take_plan(identity) {
                material_handles.push(handle);
                batch.materials.push(image);
            }
        }
        batch.descriptor_pools = self.terrain_texture_sets.take_materials(&material_handles);
        self.terrain_retirements.push_back(batch);
        Ok(())
    }

    /// Polls submission completion before presentation, with no GPU idle wait.
    pub(super) fn collect_retired_terrain(&mut self) -> Result<(), VulkanError> {
        while let Some(batch) = self.terrain_retirements.front() {
            // SAFETY: Each pending fence is live and uniquely owned by this queue.
            let completed =
                unsafe { self.device.get_fence_status(batch.fence) }.map_err(|source| {
                    VulkanError::operation("poll terrain retirement fence", source)
                })?;
            if !completed {
                break;
            }
            let allocator = self.allocator.as_ref().ok_or_else(|| {
                VulkanError::operation("access Vulkan allocator", "allocator is unavailable")
            })?;
            if let Some(batch) = self.terrain_retirements.pop_front() {
                batch.destroy(&self.device, allocator);
            }
        }
        Ok(())
    }
}
