//! Slot-retained floating depth, depth attachment, and shadow-scene descriptors.

#![allow(unsafe_code)]

use crate::device::VulkanError;
use crate::{WorldEnvironmentShadowFrame, WorldShadowProjection};
use ash::{Device, vk};
use vk_mem::Alloc;

/// GPU state reused only after the containing world's frame-slot fence retires.
#[derive(Default)]
pub(in crate::device) struct ShadowFrameResources {
    size: u32,
    color: Attachment,
    depth: Attachment,
    sampler: vk::Sampler,
    buffer: vk::Buffer,
    allocation: Option<vk_mem::Allocation>,
    receiver_offset: u64,
    caster_stride: u64,
    pool: vk::DescriptorPool,
    sets: [vk::DescriptorSet; 6],
}

/// One uniquely owned attachment and its view/allocation pair.
#[derive(Default)]
pub(super) struct Attachment {
    pub(super) image: vk::Image,
    allocation: Option<vk_mem::Allocation>,
    pub(super) view: vk::ImageView,
}

impl ShadowFrameResources {
    /// Retired-slot descriptors can switch published environment images without
    /// mutating descriptors referenced by other in-flight frames.
    fn bind_receivers(&self, device: &Device, views: [vk::ImageView; 4]) {
        let images = views.map(|view| {
            [vk::DescriptorImageInfo::default()
                .sampler(self.sampler)
                .image_view(view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)]
        });
        let writes: Vec<_> = images
            .iter()
            .enumerate()
            .flat_map(|(index, image)| {
                [
                    vk::WriteDescriptorSet::default()
                        .dst_set(self.sets[1])
                        .dst_binding(index as u32 + 1)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(image),
                    vk::WriteDescriptorSet::default()
                        .dst_set(self.sets[2])
                        .dst_binding(index as u32)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(image),
                ]
            })
            .collect();
        // SAFETY: All images live through submission and the owning slot fence retired.
        unsafe {
            device.update_descriptor_sets(&writes, &[]);
        }
    }

    /// Creates/resizes this idle slot, then serializes the current original projection.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::device) fn ensure(
        &mut self,
        device: &Device,
        allocator: &vk_mem::Allocator,
        layouts: [vk::DescriptorSetLayout; 3],
        depth_format: vk::Format,
        uniform_alignment: u64,
        projection: WorldShadowProjection,
        environment: Option<(WorldEnvironmentShadowFrame<'_>, [vk::ImageView; 3])>,
    ) -> Result<(), VulkanError> {
        if self.size != projection.texture_size() {
            self.destroy(device, allocator);
            let result = self.create(
                device,
                allocator,
                layouts,
                depth_format,
                uniform_alignment,
                projection.texture_size(),
            );
            if result.is_err() {
                self.destroy(device, allocator);
            }
            result?;
        }
        let views = environment.map_or([self.color.view; 3], |(_, views)| views);
        self.bind_receivers(device, [self.color.view, views[0], views[1], views[2]]);
        self.write(allocator, projection, environment.map(|(frame, _)| frame))
    }

    /// Allocates every child transactionally through the caller's cleanup guard.
    fn create(
        &mut self,
        device: &Device,
        allocator: &vk_mem::Allocator,
        layouts: [vk::DescriptorSetLayout; 3],
        depth_format: vk::Format,
        uniform_alignment: u64,
        size: u32,
    ) -> Result<(), VulkanError> {
        self.color.create(
            device,
            allocator,
            size,
            vk::Format::R32_SFLOAT,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            vk::ImageAspectFlags::COLOR,
        )?;
        self.depth.create(
            device,
            allocator,
            size,
            depth_format,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL,
        )?;
        // 875D30 clears filtering/wrap bits for the direct-depth backend; PCF sets bit 0.
        let sampler = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::NEAREST)
            .min_filter(vk::Filter::NEAREST)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .max_lod(0.0);
        // SAFETY: The sampler is immutable and owned by this slot.
        self.sampler = unsafe { device.create_sampler(&sampler, None) }
            .map_err(|source| VulkanError::operation("create primary shadow sampler", source))?;
        let alignment = uniform_alignment.max(16);
        self.caster_stride = 144_u64.div_ceil(alignment) * alignment;
        self.receiver_offset = self.caster_stride * 4;
        let buffer = vk::BufferCreateInfo::default()
            .size(self.receiver_offset + 256)
            .usage(vk::BufferUsageFlags::UNIFORM_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let allocation_info = vk_mem::AllocationCreateInfo {
            flags: vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE
                | vk_mem::AllocationCreateFlags::MAPPED,
            usage: vk_mem::MemoryUsage::AutoPreferHost,
            required_flags: vk::MemoryPropertyFlags::HOST_VISIBLE,
            ..Default::default()
        };
        // SAFETY: VMA binds a persistently mapped allocation covering the declared buffer.
        let (buffer, allocation) = unsafe { allocator.create_buffer(&buffer, &allocation_info) }
            .map_err(|source| VulkanError::operation("create shadow scene buffer", source))?;
        self.buffer = buffer;
        self.allocation = Some(allocation);
        let sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(5),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(8),
        ];
        let info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(6)
            .pool_sizes(&sizes);
        // SAFETY: Sizes cover caster, terrain receiver, and four-sampler M2 sets.
        self.pool = unsafe { device.create_descriptor_pool(&info, None) }.map_err(|source| {
            VulkanError::operation("create shadow scene descriptor pool", source)
        })?;
        let layouts = [
            layouts[0], layouts[1], layouts[2], layouts[0], layouts[0], layouts[0],
        ];
        let info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.pool)
            .set_layouts(&layouts);
        // SAFETY: Layouts and pool remain live until this idle slot is destroyed.
        let sets = unsafe { device.allocate_descriptor_sets(&info) }.map_err(|source| {
            VulkanError::operation("allocate shadow scene descriptors", source)
        })?;
        self.sets = sets.try_into().map_err(|_| {
            VulkanError::operation(
                "allocate shadow scene descriptors",
                "driver returned an unexpected count",
            )
        })?;
        let caster = [vk::DescriptorBufferInfo::default()
            .buffer(self.buffer)
            .offset(0)
            .range(144)];
        let receiver = [vk::DescriptorBufferInfo::default()
            .buffer(self.buffer)
            .offset(self.receiver_offset)
            .range(256)];
        let environment_casters = [1, 2, 3].map(|index| {
            [vk::DescriptorBufferInfo::default()
                .buffer(self.buffer)
                .offset(self.caster_stride * index)
                .range(144)]
        });
        let mut writes = vec![
            vk::WriteDescriptorSet::default()
                .dst_set(self.sets[0])
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .buffer_info(&caster),
            vk::WriteDescriptorSet::default()
                .dst_set(self.sets[1])
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .buffer_info(&receiver),
        ];
        for (index, caster) in environment_casters.iter().enumerate() {
            writes.push(
                vk::WriteDescriptorSet::default()
                    .dst_set(self.sets[3 + index])
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(caster),
            );
        }
        // SAFETY: All descriptor ranges and child handles belong to this idle slot.
        unsafe { device.update_descriptor_sets(&writes, &[]) };
        self.size = size;
        Ok(())
    }

    /// Serializes both shader ABIs without depending on Rust or glam struct layout.
    fn write(
        &self,
        allocator: &vk_mem::Allocator,
        projection: WorldShadowProjection,
        environment: Option<WorldEnvironmentShadowFrame<'_>>,
    ) -> Result<(), VulkanError> {
        let allocation = self.allocation.as_ref().ok_or_else(|| {
            VulkanError::operation("write shadow scene", "allocation is unavailable")
        })?;
        let pointer = allocator
            .get_allocation_info(allocation)
            .mapped_data
            .cast::<u8>();
        if pointer.is_null() {
            return Err(VulkanError::operation(
                "write shadow scene",
                "mapping is unavailable",
            ));
        }
        for index in 0..4 {
            let caster_projection = if index == 0 {
                projection
            } else {
                environment
                    .and_then(|frame| frame.passes()[index - 1])
                    .map_or(projection, |pass| pass.projection())
            };
            let mut caster = [0_u8; 144];
            for (destination, value) in caster.as_chunks_mut::<4>().0.iter_mut().zip(
                caster_projection
                    .caster_projection()
                    .to_cols_array()
                    .into_iter()
                    .chain(caster_projection.caster_view().to_cols_array())
                    .chain(caster_projection.origin().extend(0.0).to_array()),
            ) {
                destination.copy_from_slice(&value.to_le_bytes());
            }
            // SAFETY: The retired slot owns four aligned, nonoverlapping caster ranges.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    caster.as_ptr(),
                    pointer.add((self.caster_stride * index as u64) as usize),
                    caster.len(),
                );
            }
        }
        let mut receiver = [0_u8; 256];
        let environment_rows = environment.map_or([glam::Vec4::ZERO; 9], |frame| {
            let receivers = frame.receivers();
            std::array::from_fn(|index| receivers[index / 3].receiver_rows()[index % 3])
        });
        for (destination, value) in receiver.as_chunks_mut::<4>().0.iter_mut().zip(
            projection
                .origin()
                .extend(1.0 / projection.texture_size() as f32)
                .to_array()
                .into_iter()
                .chain(
                    projection
                        .receiver_rows()
                        .into_iter()
                        .flat_map(|row| row.to_array()),
                )
                .chain(projection.light_direction().extend(0.0).to_array())
                .chain(environment_rows.into_iter().flat_map(|row| row.to_array()))
                .chain([0.; 4])
                .chain([
                    environment.map_or(1., |frame| f32::from(frame.quality().shader_mode())),
                    0.,
                    0.,
                    0.,
                ]),
        ) {
            destination.copy_from_slice(&value.to_le_bytes());
        }
        // SAFETY: The slot fence retired; both nonoverlapping ranges lie in the persistent allocation.
        unsafe {
            std::ptr::copy_nonoverlapping(
                receiver.as_ptr(),
                pointer.add(self.receiver_offset as usize),
                receiver.len(),
            );
        }
        allocator
            .flush_allocation(allocation, 0, self.receiver_offset + 256)
            .map_err(|source| VulkanError::operation("flush shadow scene buffer", source))
    }

    pub(in crate::device) const fn size(&self) -> u32 {
        self.size
    }
    pub(in crate::device) const fn color_image(&self) -> vk::Image {
        self.color.image
    }
    pub(in crate::device) const fn color_view(&self) -> vk::ImageView {
        self.color.view
    }
    pub(in crate::device) const fn depth_image(&self) -> vk::Image {
        self.depth.image
    }
    pub(in crate::device) const fn depth_view(&self) -> vk::ImageView {
        self.depth.view
    }
    pub(in crate::device) const fn caster_set(&self) -> vk::DescriptorSet {
        self.sets[0]
    }
    pub(in crate::device) fn environment_caster_set(&self, index: usize) -> vk::DescriptorSet {
        self.sets[3 + index]
    }
    pub(in crate::device) const fn receiver_set(&self) -> vk::DescriptorSet {
        self.sets[1]
    }

    pub(in crate::device) const fn m2_receiver_set(&self) -> vk::DescriptorSet {
        self.sets[2]
    }

    /// Releases children only after the owning frame-slot fence is idle.
    pub(in crate::device) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        // SAFETY: The owner guarantees no submitted command can access these resources.
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
        self.color.destroy(device, allocator);
        self.depth.destroy(device, allocator);
        *self = Self::default();
    }
}

impl Attachment {
    /// Allocates an attachment and its sole mip/layer view, retaining partial state for cleanup.
    pub(super) fn create(
        &mut self,
        device: &Device,
        allocator: &vk_mem::Allocator,
        size: u32,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        aspects: vk::ImageAspectFlags,
    ) -> Result<(), VulkanError> {
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width: size,
                height: size,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let allocation_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::AutoPreferDevice,
            required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
            ..Default::default()
        };
        // SAFETY: VMA creates and binds the complete device-local image.
        let (image, allocation) = unsafe { allocator.create_image(&info, &allocation_info) }
            .map_err(|source| VulkanError::operation("create primary shadow attachment", source))?;
        self.image = image;
        self.allocation = Some(allocation);
        let range = vk::ImageSubresourceRange::default()
            .aspect_mask(aspects)
            .level_count(1)
            .layer_count(1);
        let view = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(range);
        // SAFETY: This view covers the live image's only mip and layer.
        self.view = unsafe { device.create_image_view(&view, None) }.map_err(|source| {
            VulkanError::operation("create primary shadow attachment view", source)
        })?;
        Ok(())
    }

    /// Releases a uniquely owned idle image after its view.
    pub(super) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        // SAFETY: The slot fence guarantees that neither view nor allocation is in use.
        unsafe {
            if self.view != vk::ImageView::null() {
                device.destroy_image_view(self.view, None);
            }
            if let Some(mut allocation) = self.allocation.take() {
                allocator.destroy_image(self.image, &mut allocation);
            }
        }
        *self = Self::default();
    }
}
