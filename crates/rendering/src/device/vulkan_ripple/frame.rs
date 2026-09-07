//! Reusable mapped vertices and two sampled descriptors owned by a retired slot.

#![allow(unsafe_code)]

use ash::{Device, vk};
use vk_mem::Alloc;

use crate::device::capacity::geometric_capacity;
use crate::device::vulkan_texture::BlpTextureRegistry;
use crate::{VulkanError, WaterRippleRenderVertex};

use super::WaterRippleFrame;

/// Mutable ripple data is isolated by the enclosing world frame's fence.
pub(in crate::device) struct RippleFrameResources {
    buffer: vk::Buffer,
    allocation: Option<vk_mem::Allocation>,
    capacity: usize,
    pool: vk::DescriptorPool,
    sets: [vk::DescriptorSet; 2],
    sampler: vk::Sampler,
    views: [Option<vk::ImageView>; 2],
}

impl RippleFrameResources {
    pub(in crate::device) const fn empty() -> Self {
        Self {
            buffer: vk::Buffer::null(),
            allocation: None,
            capacity: 0,
            pool: vk::DescriptorPool::null(),
            sets: [vk::DescriptorSet::null(); 2],
            sampler: vk::Sampler::null(),
            views: [None; 2],
        }
    }

    /// Grows only a retired slot, preserving the old resources on allocation failure.
    pub(in crate::device) fn ensure(
        &mut self,
        device: &Device,
        allocator: &vk_mem::Allocator,
        layout: vk::DescriptorSetLayout,
        vertex_count: usize,
    ) -> Result<(), VulkanError> {
        if self.capacity >= vertex_count {
            return Ok(());
        }
        let capacity = geometric_capacity(self.capacity, vertex_count);
        let mut replacement = Self::empty();
        if let Err(error) = replacement.create(device, allocator, layout, capacity) {
            replacement.destroy(device, allocator);
            return Err(error);
        }
        self.destroy(device, allocator);
        *self = replacement;
        Ok(())
    }

    fn create(
        &mut self,
        device: &Device,
        allocator: &vk_mem::Allocator,
        layout: vk::DescriptorSetLayout,
        capacity: usize,
    ) -> Result<(), VulkanError> {
        let size = capacity
            .checked_mul(WaterRippleRenderVertex::BYTE_SIZE)
            .ok_or(VulkanError::WorldFrameCapacity)? as u64;
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(vk::BufferUsageFlags::VERTEX_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let allocation_info = vk_mem::AllocationCreateInfo {
            flags: vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
            usage: vk_mem::MemoryUsage::AutoPreferHost,
            required_flags: vk::MemoryPropertyFlags::HOST_VISIBLE,
            ..Default::default()
        };
        // SAFETY: VMA binds host-visible storage covering the checked vertex extent.
        let (buffer, allocation) =
            unsafe { allocator.create_buffer(&buffer_info, &allocation_info) }
                .map_err(|source| VulkanError::operation("create ripple vertex buffer", source))?;
        self.buffer = buffer;
        self.allocation = Some(allocation);
        // Native texture flags 0x201 select AD8F40/AD8FE0 row 1: min/mag
        // linear, mip none, anisotropy one. Address bits zero select clamp.
        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .max_lod(0.0)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE);
        // SAFETY: This sampler requires no optional device feature.
        self.sampler = unsafe { device.create_sampler(&sampler_info, None) }
            .map_err(|source| VulkanError::operation("create ripple sampler", source))?;
        let sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(2)];
        let info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(2)
            .pool_sizes(&sizes);
        // SAFETY: The pool covers both one-texture pass descriptors.
        self.pool = unsafe { device.create_descriptor_pool(&info, None) }
            .map_err(|source| VulkanError::operation("create ripple descriptor pool", source))?;
        let layouts = [layout; 2];
        let info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.pool)
            .set_layouts(&layouts);
        // SAFETY: Pipeline preparation owns the live compatible layouts.
        let sets = unsafe { device.allocate_descriptor_sets(&info) }
            .map_err(|source| VulkanError::operation("allocate ripple descriptors", source))?;
        self.sets = sets
            .try_into()
            .map_err(|_| VulkanError::WorldFrameCapacity)?;
        self.capacity = capacity;
        Ok(())
    }

    /// Refreshes only this retired slot; steady frames allocate no Vulkan objects.
    pub(in crate::device) fn write(
        &mut self,
        device: &Device,
        allocator: &vk_mem::Allocator,
        textures: &BlpTextureRegistry,
        frame: WaterRippleFrame<'_>,
    ) -> Result<(), VulkanError> {
        if frame.draw_count() == 0 {
            return Ok(());
        }
        if frame.vertex_count() > self.capacity {
            return Err(VulkanError::WorldFrameCapacity);
        }
        let mut views = [None; 2];
        for (index, pass) in frame.passes().into_iter().enumerate() {
            if let Some(pass) = pass.filter(|pass| pass.draw_vertex_count() != 0) {
                views[index] = Some(
                    textures
                        .view(pass.texture())
                        .ok_or(VulkanError::UnknownBlpTextureHandle)?,
                );
            }
        }
        let allocation = self
            .allocation
            .as_mut()
            .ok_or(VulkanError::WorldFrameCapacity)?;
        // SAFETY: The enclosing slot fence retired every reader of this host-visible buffer.
        let destination = unsafe { allocator.map_memory(allocation) }
            .map_err(|source| VulkanError::operation("map ripple vertices", source))?;
        let mut offset = 0;
        for pass in frame.passes().into_iter().flatten() {
            for vertex in pass.vertices() {
                let bytes = vertex.to_bytes();
                // SAFETY: The frame count check covers each complete nonoverlapping vertex.
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        bytes.as_ptr(),
                        destination.add(offset),
                        bytes.len(),
                    )
                };
                offset += bytes.len();
            }
        }
        let flushed = allocator
            .flush_allocation(allocation, 0, offset as u64)
            .map_err(|source| VulkanError::operation("flush ripple vertices", source));
        // SAFETY: This allocation was mapped successfully, including on flush failure.
        unsafe { allocator.unmap_memory(allocation) };
        flushed?;
        for (index, view) in views.into_iter().enumerate() {
            if let Some(view) = view.filter(|view| self.views[index] != Some(*view)) {
                let images = [vk::DescriptorImageInfo::default()
                    .sampler(self.sampler)
                    .image_view(view)
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
                let writes = [vk::WriteDescriptorSet::default()
                    .dst_set(self.sets[index])
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&images)];
                // SAFETY: Slot retirement excludes descriptor readers; this live view is validated above.
                unsafe { device.update_descriptor_sets(&writes, &[]) };
                self.views[index] = Some(view);
            }
        }
        Ok(())
    }

    pub(in crate::device) const fn buffer(&self) -> vk::Buffer {
        self.buffer
    }
    pub(in crate::device) const fn sets(&self) -> [vk::DescriptorSet; 2] {
        self.sets
    }

    /// Releases all resources after this slot's GPU use has finished.
    pub(in crate::device) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        // SAFETY: These handles have unique slot ownership and no pending readers.
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
