//! Six-vertex celestial strip and texture descriptor per retired world slot.

#![allow(unsafe_code)]

use ash::{Device, vk};
use vk_mem::Alloc;

use crate::device::vulkan_texture::BlpTextureRegistry;
use crate::{VulkanError, WorldCelestialDraw};

const STRIDE: usize = 24;
const INDEX_OFFSET: usize = 6 * STRIDE;
const BUFFER_SIZE: usize = INDEX_OFFSET + 6 * 2;

/// Each enclosing frame fence owns one native body strip and its descriptor.
pub(in crate::device) struct CelestialFrameResources {
    buffer: vk::Buffer,
    allocation: Option<vk_mem::Allocation>,
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    sampler: vk::Sampler,
    view: Option<vk::ImageView>,
}

impl CelestialFrameResources {
    pub(in crate::device) const fn empty() -> Self {
        Self {
            buffer: vk::Buffer::null(),
            allocation: None,
            pool: vk::DescriptorPool::null(),
            set: vk::DescriptorSet::null(),
            sampler: vk::Sampler::null(),
            view: None,
        }
    }

    /// Allocates once per slot; celestial frames reuse this six-vertex bank.
    pub(in crate::device) fn ensure(
        &mut self,
        device: &Device,
        allocator: &vk_mem::Allocator,
        layout: vk::DescriptorSetLayout,
    ) -> Result<(), VulkanError> {
        if self.buffer != vk::Buffer::null() {
            return Ok(());
        }
        let mut replacement = Self::empty();
        if let Err(error) = replacement.create(device, allocator, layout) {
            replacement.destroy(device, allocator);
            return Err(error);
        }
        *self = replacement;
        Ok(())
    }

    fn create(
        &mut self,
        device: &Device,
        allocator: &vk_mem::Allocator,
        layout: vk::DescriptorSetLayout,
    ) -> Result<(), VulkanError> {
        let info = vk::BufferCreateInfo::default()
            .size(BUFFER_SIZE as u64)
            .usage(vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::INDEX_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let allocation_info = vk_mem::AllocationCreateInfo {
            flags: vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
            usage: vk_mem::MemoryUsage::AutoPreferHost,
            required_flags: vk::MemoryPropertyFlags::HOST_VISIBLE,
            ..Default::default()
        };
        // SAFETY: VMA binds the fixed, checked host-visible vertex/index extent.
        let (buffer, allocation) = unsafe { allocator.create_buffer(&info, &allocation_info) }
            .map_err(|source| VulkanError::operation("create celestial quad buffer", source))?;
        self.buffer = buffer;
        self.allocation = Some(allocation);
        // 9AD0B0 requests sampler row 1: linear min/mag, no mip filtering, clamp.
        let info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .max_lod(0.0)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE);
        // SAFETY: No optional feature or anisotropy is requested.
        self.sampler = unsafe { device.create_sampler(&info, None) }
            .map_err(|source| VulkanError::operation("create celestial sampler", source))?;
        let sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)];
        let info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(1)
            .pool_sizes(&sizes);
        // SAFETY: The pool covers the one fixed texture binding.
        self.pool = unsafe { device.create_descriptor_pool(&info, None) }
            .map_err(|source| VulkanError::operation("create celestial descriptor pool", source))?;
        let layouts = [layout];
        let info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.pool)
            .set_layouts(&layouts);
        // SAFETY: The prepared PCT pipeline owns this live layout.
        self.set = unsafe { device.allocate_descriptor_sets(&info) }
            .map_err(|source| VulkanError::operation("allocate celestial descriptor", source))?
            .into_iter()
            .next()
            .ok_or(VulkanError::WorldFrameCapacity)?;
        Ok(())
    }

    /// Writes geometry and changes descriptors only after this slot's fence retires.
    pub(in crate::device) fn write(
        &mut self,
        device: &Device,
        allocator: &vk_mem::Allocator,
        textures: &BlpTextureRegistry,
        frame: WorldCelestialDraw<'_>,
    ) -> Result<(), VulkanError> {
        if frame.mesh().indices().is_empty() {
            return Ok(());
        }
        let view = textures
            .view(frame.texture())
            .ok_or(VulkanError::UnknownBlpTextureHandle)?;
        let allocation = self
            .allocation
            .as_mut()
            .ok_or(VulkanError::WorldFrameCapacity)?;
        // SAFETY: The retired slot excludes readers of this host-visible allocation.
        let destination = unsafe { allocator.map_memory(allocation) }
            .map_err(|source| VulkanError::operation("map celestial quads", source))?;
        for index in 0..6 {
            let mut bytes = [0_u8; STRIDE];
            for (i, value) in frame.mesh().positions()[index].iter().enumerate() {
                bytes[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
            }
            let color = frame.mesh().colors()[index];
            bytes[12..16].copy_from_slice(&[
                (color >> 16) as u8,
                (color >> 8) as u8,
                color as u8,
                (color >> 24) as u8,
            ]);
            for (i, value) in frame.mesh().uv()[index].iter().enumerate() {
                bytes[16 + i * 4..20 + i * 4].copy_from_slice(&value.to_le_bytes());
            }
            // SAFETY: All six packed vertices fit the fixed allocation.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    bytes.as_ptr(),
                    destination.add(index * STRIDE),
                    STRIDE,
                );
            }
        }
        for (index, value) in frame.mesh().indices().iter().enumerate() {
            let bytes = value.to_le_bytes();
            // SAFETY: Frame construction limits indices to the buffer's remaining extent.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    bytes.as_ptr(),
                    destination.add(INDEX_OFFSET + index * 2),
                    2,
                );
            }
        }
        let result = allocator
            .flush_allocation(allocation, 0, BUFFER_SIZE as u64)
            .map_err(|source| VulkanError::operation("flush celestial quads", source));
        // SAFETY: Mapping succeeded; unmapping is required even after a failed flush.
        unsafe {
            allocator.unmap_memory(allocation);
        }
        result?;
        if self.view != Some(view) {
            let images = [vk::DescriptorImageInfo::default()
                .sampler(self.sampler)
                .image_view(view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let writes = [vk::WriteDescriptorSet::default()
                .dst_set(self.set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&images)];
            // SAFETY: The image is live and slot retirement excludes descriptor readers.
            unsafe {
                device.update_descriptor_sets(&writes, &[]);
            }
            self.view = Some(view);
        }
        Ok(())
    }

    pub(in crate::device) const fn vertex_buffer(&self) -> (vk::Buffer, vk::DeviceSize) {
        (self.buffer, 0)
    }
    pub(in crate::device) const fn index_buffer(&self) -> (vk::Buffer, vk::DeviceSize) {
        (self.buffer, INDEX_OFFSET as u64)
    }
    pub(in crate::device) const fn descriptor(&self) -> vk::DescriptorSet {
        self.set
    }

    /// Releases the slot's exclusive resources after GPU use finishes.
    pub(in crate::device) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        // SAFETY: All handles have unique ownership and no pending GPU readers.
        unsafe {
            if self.pool != vk::DescriptorPool::null() {
                device.destroy_descriptor_pool(self.pool, None);
            }
            if self.sampler != vk::Sampler::null() {
                device.destroy_sampler(self.sampler, None);
            }
            if let Some(mut allocation) = self.allocation.take() {
                allocator.destroy_buffer(self.buffer, &mut allocation);
            }
        }
        *self = Self::empty();
    }
}
