//! Resource release batches wait on graphics completion without waiting on CPU.

use super::super::VulkanRenderer;
use super::{GpuResourceLease, ResourceKey};
use crate::device::vulkan_mesh::GpuMeshBuffers;
use crate::device::vulkan_texture::GpuSampledImage;
use crate::device::{
    CharacterAtlasTextureHandle, M2MeshHandle, UiGlyphTextureHandle, UiMeshHandle, VulkanError,
};
use ash::{Device, vk};

/// Invalidated objects remain owned here until their final queue use retires.
pub(super) struct ResourceRetirement {
    fence: vk::Fence,
    images: Vec<GpuSampledImage>,
    meshes: Vec<GpuMeshBuffers>,
    descriptors: Vec<(vk::DescriptorPool, vk::DescriptorSet)>,
    pools: Vec<vk::DescriptorPool>,
}

impl ResourceRetirement {
    /// Fence completion or shutdown establishes that no pending command uses this batch.
    pub(in super::super) fn destroy(mut self, device: &Device, allocator: &vk_mem::Allocator) {
        // SAFETY: All these sets/pools and the fence have retired, and were
        // transferred exclusively out of the live registries.
        unsafe {
            for (pool, set) in self.descriptors {
                let _device_lost = device.free_descriptor_sets(pool, &[set]);
            }
            for pool in self.pools {
                device.destroy_descriptor_pool(pool, None);
            }
            device.destroy_fence(self.fence, None);
        }
        for image in &mut self.images {
            image.destroy(device, allocator);
        }
        for mesh in &mut self.meshes {
            mesh.destroy(allocator);
        }
    }
}

impl VulkanRenderer {
    /// Pins a character body atlas for a resident model source.
    /// # Errors
    /// Returns a foreign or retired resource error.
    pub fn retain_character_atlas(
        &mut self,
        handle: CharacterAtlasTextureHandle,
    ) -> Result<GpuResourceLease, VulkanError> {
        self.character_atlas_textures
            .info(handle)
            .ok_or(VulkanError::UnknownCharacterAtlasTextureHandle)?;
        Ok(self
            .resource_lifetimes
            .pin(ResourceKey::CharacterAtlas(handle)))
    }
    /// Pins a glyph page for a resident UI frame or coverage bank.
    /// # Errors
    /// Returns a foreign or retired resource error.
    pub fn retain_ui_glyph_texture(
        &mut self,
        handle: UiGlyphTextureHandle,
    ) -> Result<GpuResourceLease, VulkanError> {
        self.ui_glyph_textures
            .info(handle)
            .ok_or(VulkanError::UnknownUiGlyphTextureHandle)?;
        Ok(self.resource_lifetimes.pin(ResourceKey::Glyph(handle)))
    }
    /// Pins a retained UI mesh, including future partial replacements.
    /// # Errors
    /// Returns a foreign or retired resource error.
    pub fn retain_ui_mesh(
        &mut self,
        handle: UiMeshHandle,
    ) -> Result<GpuResourceLease, VulkanError> {
        self.ui_meshes
            .info(handle)
            .ok_or(VulkanError::UnknownUiMeshHandle)?;
        Ok(self.resource_lifetimes.pin(ResourceKey::UiMesh(handle)))
    }
    /// Pins shared M2 geometry while any resident source can submit it.
    /// # Errors
    /// Returns a foreign or retired resource error.
    pub fn retain_m2_mesh(
        &mut self,
        handle: M2MeshHandle,
    ) -> Result<GpuResourceLease, VulkanError> {
        self.m2_meshes
            .info(handle)
            .ok_or(VulkanError::UnknownM2MeshHandle)?;
        Ok(self.resource_lifetimes.pin(ResourceKey::M2Mesh(handle)))
    }

    /// Consumes only last-owner events; a resource reacquired before collection
    /// remains resident. The fence is submitted before invalidating any handle.
    pub(in super::super) fn collect_released_resources(&mut self) -> Result<(), VulkanError> {
        let _profile_scope = solarity_profiling::profile!(
            "rendering.device.vulkan_renderer.resource_lifetime.retirement.collect_released_resources"
        );
        let mut released = Vec::new();
        while let Ok(key) = self.resource_lifetimes.receiver.try_recv() {
            if self
                .resource_lifetimes
                .owners
                .get(&key)
                .is_some_and(|owner| owner.strong_count() == 0)
            {
                released.push(key);
            }
        }
        if !released.is_empty() {
            // SAFETY: Exclusive renderer access serializes submissions to this queue.
            let fence = unsafe {
                self.device
                    .create_fence(&vk::FenceCreateInfo::default(), None)
            }
            .map_err(|error| VulkanError::operation("create resource retirement fence", error))?;
            // SAFETY: Empty submission covers every earlier use on the same queue.
            if let Err(error) = unsafe { self.device.queue_submit(self.graphics_queue, &[], fence) }
            {
                // SAFETY: A failed submission did not acquire the fence.
                unsafe {
                    self.device.destroy_fence(fence, None);
                }
                return Err(VulkanError::operation("submit resource retirement", error));
            }
            self.is_idle = false;
            let mut batch = ResourceRetirement {
                fence,
                images: Vec::new(),
                meshes: Vec::new(),
                descriptors: Vec::new(),
                pools: Vec::new(),
            };
            for key in released {
                if self.resource_lifetimes.owners.remove(&key).is_none() {
                    continue;
                }
                match key {
                    ResourceKey::Glyph(handle) => {
                        if let Some(image) = self.ui_glyph_textures.take(handle) {
                            batch.images.push(image);
                        }
                        let (sets, pools) = self.ui_texture_sets.take_glyph(handle);
                        batch.descriptors.extend(sets);
                        batch.pools.extend(pools);
                    }
                    ResourceKey::CharacterAtlas(handle) => {
                        if let Some(image) = self.character_atlas_textures.take(handle) {
                            batch.images.push(image);
                        }
                        batch
                            .descriptors
                            .extend(self.m2_texture_sets.take_atlas(handle));
                    }
                    ResourceKey::UiMesh(handle) => {
                        if let Some(mesh) = self.ui_meshes.take(handle) {
                            batch.meshes.push(mesh);
                        }
                    }
                    ResourceKey::M2Mesh(handle) => {
                        if let Some(mesh) = self.m2_meshes.take(handle) {
                            batch.meshes.push(mesh);
                        }
                    }
                }
            }
            self.resource_lifetimes.pending.push_back(batch);
        }
        while let Some(batch) = self.resource_lifetimes.pending.front() {
            // SAFETY: Pending batches exclusively own their live fences.
            if !unsafe { self.device.get_fence_status(batch.fence) }
                .map_err(|error| VulkanError::operation("poll resource retirement", error))?
            {
                break;
            }
            let allocator = self.allocator.as_ref().ok_or_else(|| {
                VulkanError::operation("access Vulkan allocator", "allocator is unavailable")
            })?;
            if let Some(batch) = self.resource_lifetimes.pending.pop_front() {
                for &(pool, _) in &batch.descriptors {
                    self.m2_texture_sets.release_capacity(pool);
                }
                batch.destroy(&self.device, allocator);
            }
        }
        Ok(())
    }

    /// Shutdown already waited for every queue, so all release batches are safe.
    pub(in super::super) fn destroy_resource_retirements(&mut self) {
        if let Some(allocator) = self.allocator.as_ref() {
            for batch in self.resource_lifetimes.pending.drain(..) {
                for &(pool, _) in &batch.descriptors {
                    self.m2_texture_sets.release_capacity(pool);
                }
                batch.destroy(&self.device, allocator);
            }
        }
    }
}
