//! Native cloud dome, visible texture, and staging bank per retired world slot.

#![allow(unsafe_code)]

use ash::{Device, vk};
use vk_mem::Alloc;

use crate::{VulkanError, WorldCloudFrame};

mod image;
use image::CloudImage;

const STRIDE: usize = 24;
const INDEX_OFFSET: usize = 177 * STRIDE;
const PIXEL_OFFSET: usize = (INDEX_OFFSET + 374 * 2 + 3) & !3;
const BUFFER_SIZE: usize = PIXEL_OFFSET + 128 * 128 * 4;

/// Each enclosing frame fence owns its geometry, texture, and descriptors.
pub(in crate::device) struct CloudFrameResources {
    buffer: vk::Buffer,
    allocation: Option<vk_mem::Allocation>,
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    sampler: vk::Sampler,
    image: CloudImage,
    pixels: Vec<u8>,
    upload: bool,
}

impl CloudFrameResources {
    pub(in crate::device) const fn empty() -> Self {
        Self {
            buffer: vk::Buffer::null(),
            allocation: None,
            pool: vk::DescriptorPool::null(),
            set: vk::DescriptorSet::null(),
            sampler: vk::Sampler::null(),
            image: CloudImage::empty(),
            pixels: Vec::new(),
            upload: false,
        }
    }

    /// Allocates once per slot; later frames reuse this fixed native-sized bank.
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
            .usage(
                vk::BufferUsageFlags::VERTEX_BUFFER
                    | vk::BufferUsageFlags::INDEX_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_SRC,
            )
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let allocation_info = vk_mem::AllocationCreateInfo {
            flags: vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
            usage: vk_mem::MemoryUsage::AutoPreferHost,
            required_flags: vk::MemoryPropertyFlags::HOST_VISIBLE,
            ..Default::default()
        };
        // SAFETY: VMA binds the fixed, checked host-visible vertex/index extent.
        let (buffer, allocation) = unsafe { allocator.create_buffer(&info, &allocation_info) }
            .map_err(|source| VulkanError::operation("create cloud quad buffer", source))?;
        self.buffer = buffer;
        self.allocation = Some(allocation);
        // 7F1B10 selects sampler row 1: linear min/mag, no mip filtering, clamp.
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
            .map_err(|source| VulkanError::operation("create cloud sampler", source))?;
        let sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)];
        let info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(1)
            .pool_sizes(&sizes);
        // SAFETY: The pool covers the one fixed texture binding.
        self.pool = unsafe { device.create_descriptor_pool(&info, None) }
            .map_err(|source| VulkanError::operation("create cloud descriptor pool", source))?;
        let layouts = [layout];
        let info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.pool)
            .set_layouts(&layouts);
        // SAFETY: The prepared PCT pipeline owns this live layout.
        self.set = unsafe { device.allocate_descriptor_sets(&info) }
            .map_err(|source| VulkanError::operation("allocate cloud descriptor", source))?
            .into_iter()
            .next()
            .ok_or(VulkanError::WorldFrameCapacity)?;
        self.image.create(device, allocator)?;
        let images = [vk::DescriptorImageInfo::default()
            .sampler(self.sampler)
            .image_view(self.image.view())
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let writes = [vk::WriteDescriptorSet::default()
            .dst_set(self.set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&images)];
        // SAFETY: The descriptor and image are owned by this unsubmitted slot.
        unsafe {
            device.update_descriptor_sets(&writes, &[]);
        }
        Ok(())
    }

    /// Writes geometry and changed pixels only after this slot's fence retires.
    pub(in crate::device) fn write(
        &mut self,
        allocator: &vk_mem::Allocator,
        frame: WorldCloudFrame<'_>,
    ) -> Result<(), VulkanError> {
        self.upload |= self.pixels != frame.bgra8();
        let allocation = self
            .allocation
            .as_mut()
            .ok_or(VulkanError::WorldFrameCapacity)?;
        // SAFETY: Slot retirement excludes all GPU readers of this allocation.
        let destination = unsafe { allocator.map_memory(allocation) }
            .map_err(|source| VulkanError::operation("map cloud bank", source))?;
        let mut bytes = vec![
            0_u8;
            if self.upload {
                BUFFER_SIZE
            } else {
                PIXEL_OFFSET
            }
        ];
        for (i, position) in frame.dome().positions().iter().enumerate() {
            let start = i * STRIDE;
            for (component, value) in position.iter().enumerate() {
                bytes[start + component * 4..start + component * 4 + 4]
                    .copy_from_slice(&value.to_le_bytes());
            }
            let color = frame.dome().colors()[i];
            bytes[start + 12..start + 16].copy_from_slice(&[
                (color >> 16) as u8,
                (color >> 8) as u8,
                color as u8,
                (color >> 24) as u8,
            ]);
            for (component, value) in frame.dome().coordinates()[i].iter().enumerate() {
                bytes[start + 16 + component * 4..start + 20 + component * 4]
                    .copy_from_slice(&value.to_le_bytes());
            }
        }
        for (i, value) in frame.dome().indices().iter().enumerate() {
            bytes[INDEX_OFFSET + i * 2..INDEX_OFFSET + i * 2 + 2]
                .copy_from_slice(&value.to_le_bytes());
        }
        if self.upload {
            bytes[PIXEL_OFFSET..].copy_from_slice(frame.bgra8());
        }
        // SAFETY: The fixed geometry and private 128x128 bank fit the allocation.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), destination, bytes.len());
        }
        let result = allocator
            .flush_allocation(allocation, 0, bytes.len() as u64)
            .map_err(|source| VulkanError::operation("flush cloud bank", source));
        // SAFETY: Mapping succeeded; always release it including on flush failure.
        unsafe {
            allocator.unmap_memory(allocation);
        }
        result?;
        // A previous acquisition may fail before submission; retaining the upload
        // request until recording ensures that unchanged pixels still reach the GPU.
        if self.upload {
            self.pixels.clear();
            self.pixels.extend_from_slice(frame.bgra8());
        }
        Ok(())
    }

    pub(in crate::device) fn record_upload(&self, device: &Device, command: vk::CommandBuffer) {
        if self.upload {
            self.image
                .upload(device, command, self.buffer, PIXEL_OFFSET as u64);
        }
    }

    /// Commits the cache only when the queue accepted the recorded upload.
    pub(in crate::device) fn submitted(&mut self) {
        self.upload = false;
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
        self.image.destroy(device, allocator);
        *self = Self::empty();
    }
}
