//! Reusable descriptor capacity for the common two-binding M2 texture layout.

#![allow(unsafe_code)]

use ash::{Device, vk};

use crate::device::VulkanError;

/// Owns one pool until renderer teardown; retired sets return capacity only
/// after their final submitted GPU use. Live material descriptors stay stable.
pub(super) struct M2DescriptorPool {
    handle: vk::DescriptorPool,
    /// Set slots, each charged for two combined image/sampler descriptors.
    capacity: u32,
    /// Unallocated set slots; failed allocations leave this count unchanged.
    remaining: u32,
}

impl M2DescriptorPool {
    /// Reserves a nonzero number of sets, each charged for both layout bindings
    /// even when its shader statically consumes only one sampled texture.
    pub(super) fn new(device: &Device, capacity: u32) -> Result<Self, VulkanError> {
        let descriptor_count = capacity
            .checked_mul(2)
            .filter(|count| *count != 0)
            .ok_or(VulkanError::M2TextureSetCapacity)?;
        let sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(descriptor_count)];
        let info = vk::DescriptorPoolCreateInfo::default()
            .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET)
            .max_sets(capacity)
            .pool_sizes(&sizes);
        // SAFETY: Both counts cover the common M2 layout. Individual frees are
        // accounted for only after their resource retirement fence completes.
        let handle = unsafe { device.create_descriptor_pool(&info, None) }.map_err(|source| {
            VulkanError::operation("create M2 texture descriptor pool", source)
        })?;
        Ok(Self {
            handle,
            capacity,
            remaining: capacity,
        })
    }

    pub(super) const fn handle(&self) -> vk::DescriptorPool {
        self.handle
    }
    /// Returns a set slot only after its GPU retirement fence completes.
    pub(super) fn release_capacity(&mut self) {
        self.remaining += 1;
    }

    pub(super) const fn capacity(&self) -> u32 {
        self.capacity
    }

    pub(super) const fn can_allocate(&self, count: u32) -> bool {
        count != 0 && count <= self.remaining
    }

    /// Consumes capacity only after the complete Vulkan allocation succeeds.
    /// All allocations use the same layout and no sets are individually freed;
    /// VkDescriptorPoolCreateInfo's fragmentation guarantees therefore apply.
    pub(super) fn allocate(
        &mut self,
        device: &Device,
        layout: vk::DescriptorSetLayout,
        count: u32,
    ) -> Result<Vec<vk::DescriptorSet>, VulkanError> {
        if !self.can_allocate(count) {
            return Err(VulkanError::M2TextureSetCapacity);
        }
        let layouts = vec![layout; count as usize];
        let info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.handle)
            .set_layouts(&layouts);
        // SAFETY: This pool has capacity for every repeated compatible layout.
        // Vulkan rolls a failed batch back, leaving existing allocations live.
        let sets = unsafe { device.allocate_descriptor_sets(&info) }.map_err(|source| {
            VulkanError::operation("allocate M2 texture descriptor sets", source)
        })?;
        self.remaining -= count;
        Ok(sets)
    }

    /// Destroys a failed unused pool or a pool whose draws have all retired.
    pub(super) fn destroy(self, device: &Device) {
        // SAFETY: The caller owns this device's pool and has retired all uses.
        unsafe { device.destroy_descriptor_pool(self.handle, None) };
    }
}
