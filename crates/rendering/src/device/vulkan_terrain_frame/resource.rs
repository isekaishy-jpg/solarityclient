//! Persistently mapped terrain scene buffers and independently fenced slots.

#![allow(unsafe_code)]

use std::ptr;

use ash::{Device, vk};
use vk_mem::Alloc;

use crate::TerrainSceneUniform;
use crate::device::VulkanError;

pub(super) struct FrameCreateContext<'a> {
    pub(super) device: &'a Device,
    pub(super) allocator: &'a vk_mem::Allocator,
    pub(super) scene_layout: vk::DescriptorSetLayout,
    pub(super) graphics_queue_family: u32,
    pub(super) slot_count: usize,
    pub(super) extent: (u32, u32),
    pub(super) depth_format: vk::Format,
}

/// One command/depth/scene allocation protected by its own fence.
pub(super) struct TerrainFrameSlot {
    scene_buffer: vk::Buffer,
    scene_allocation: Option<vk_mem::Allocation>,
    descriptor_pool: vk::DescriptorPool,
    scene_set: vk::DescriptorSet,
    depth_image: vk::Image,
    depth_allocation: Option<vk_mem::Allocation>,
    depth_view: vk::ImageView,
    command_pool: vk::CommandPool,
    command_buffer: vk::CommandBuffer,
    image_available: vk::Semaphore,
    fence: vk::Fence,
}

impl TerrainFrameSlot {
    fn create(context: &FrameCreateContext<'_>) -> Result<Self, VulkanError> {
        let mut slot = Self::empty();
        let result = (|| {
            slot.create_scene(context)?;
            slot.create_depth(context)?;
            slot.create_commands(context)?;
            slot.create_synchronization(context.device)?;
            Ok(())
        })();
        if let Err(error) = result {
            slot.destroy(context.device, context.allocator);
            return Err(error);
        }
        Ok(slot)
    }

    pub(super) const fn command_buffer(&self) -> vk::CommandBuffer {
        self.command_buffer
    }

    pub(super) const fn image_available(&self) -> vk::Semaphore {
        self.image_available
    }

    pub(super) const fn fence(&self) -> vk::Fence {
        self.fence
    }

    pub(super) const fn scene_set(&self) -> vk::DescriptorSet {
        self.scene_set
    }

    pub(super) const fn depth_image(&self) -> vk::Image {
        self.depth_image
    }

    pub(super) const fn depth_view(&self) -> vk::ImageView {
        self.depth_view
    }

    pub(super) fn wait_and_reset(&self, device: &Device) -> Result<(), VulkanError> {
        // SAFETY: This fence was created signaled or submitted once since wait.
        unsafe {
            device
                .wait_for_fences(&[self.fence], true, u64::MAX)
                .map_err(|source| VulkanError::operation("wait for terrain frame slot", source))?;
            device
                .reset_command_pool(self.command_pool, vk::CommandPoolResetFlags::empty())
                .map_err(|source| {
                    VulkanError::operation("reset terrain frame command pool", source)
                })?;
        }
        Ok(())
    }

    pub(super) fn write_scene(
        &self,
        allocator: &vk_mem::Allocator,
        scene: TerrainSceneUniform,
    ) -> Result<(), VulkanError> {
        let allocation = self.scene_allocation.as_ref().ok_or_else(|| {
            VulkanError::operation("access terrain scene buffer", "allocation is unavailable")
        })?;
        let destination = allocator
            .get_allocation_info(allocation)
            .mapped_data
            .cast::<u8>();
        if destination.is_null() {
            return Err(VulkanError::operation(
                "access terrain scene mapping",
                "persistent mapping is unavailable",
            ));
        }
        let bytes = scene.to_bytes();
        // SAFETY: VMA reports a persistent mapping covering the 112-byte buffer;
        // the slot fence was waited before this CPU write.
        unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), destination, bytes.len()) };
        allocator
            .flush_allocation(allocation, 0, bytes.len() as vk::DeviceSize)
            .map_err(|source| VulkanError::operation("flush terrain scene buffer", source))
    }

    pub(super) fn reset_fence(&self, device: &Device) -> Result<(), VulkanError> {
        // SAFETY: Prior use was waited and no submission currently owns it.
        unsafe { device.reset_fences(&[self.fence]) }
            .map_err(|source| VulkanError::operation("reset terrain frame fence", source))
    }

    pub(super) fn restore_signaled_fence(&mut self, device: &Device) -> Result<(), VulkanError> {
        // SAFETY: Failed submission did not take ownership of the reset fence.
        unsafe { device.destroy_fence(self.fence, None) };
        let info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        // SAFETY: The replacement fence has no external payload.
        self.fence = unsafe { device.create_fence(&info, None) }
            .map_err(|source| VulkanError::operation("restore terrain frame fence", source))?;
        Ok(())
    }

    fn create_scene(&mut self, context: &FrameCreateContext<'_>) -> Result<(), VulkanError> {
        let buffer_info = vk::BufferCreateInfo::default()
            .size(TerrainSceneUniform::BYTE_SIZE as vk::DeviceSize)
            .usage(vk::BufferUsageFlags::UNIFORM_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let allocation_info = vk_mem::AllocationCreateInfo {
            flags: vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE
                | vk_mem::AllocationCreateFlags::MAPPED,
            usage: vk_mem::MemoryUsage::AutoPreferHost,
            required_flags: vk::MemoryPropertyFlags::HOST_VISIBLE,
            ..Default::default()
        };
        // SAFETY: VMA binds the returned allocation to this exact buffer.
        let (buffer, allocation) = unsafe {
            context
                .allocator
                .create_buffer(&buffer_info, &allocation_info)
        }
        .map_err(|source| VulkanError::operation("create terrain scene buffer", source))?;
        self.scene_buffer = buffer;
        self.scene_allocation = Some(allocation);

        let pool_sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)];
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(1)
            .pool_sizes(&pool_sizes);
        // SAFETY: Pool counts cover the sole terrain scene descriptor.
        self.descriptor_pool = unsafe { context.device.create_descriptor_pool(&pool_info, None) }
            .map_err(|source| {
            VulkanError::operation("create terrain scene descriptor pool", source)
        })?;
        let layouts = [context.scene_layout];
        let allocation_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.descriptor_pool)
            .set_layouts(&layouts);
        // SAFETY: The pool and compatible layout remain live.
        self.scene_set = unsafe { context.device.allocate_descriptor_sets(&allocation_info) }
            .map_err(|source| VulkanError::operation("allocate terrain scene descriptor", source))?
            .first()
            .copied()
            .ok_or_else(|| {
                VulkanError::operation("allocate terrain scene descriptor", "driver returned none")
            })?;
        let buffer_infos = [vk::DescriptorBufferInfo::default()
            .buffer(self.scene_buffer)
            .offset(0)
            .range(TerrainSceneUniform::BYTE_SIZE as vk::DeviceSize)];
        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.scene_set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .buffer_info(&buffer_infos);
        // SAFETY: The descriptor range covers the complete live buffer.
        unsafe { context.device.update_descriptor_sets(&[write], &[]) };
        Ok(())
    }

    fn create_depth(&mut self, context: &FrameCreateContext<'_>) -> Result<(), VulkanError> {
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(context.depth_format)
            .extent(vk::Extent3D {
                width: context.extent.0,
                height: context.extent.1,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let allocation_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::AutoPreferDevice,
            required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
            ..Default::default()
        };
        // SAFETY: VMA binds the returned device-local allocation to the image.
        let (image, allocation) = unsafe {
            context
                .allocator
                .create_image(&image_info, &allocation_info)
        }
        .map_err(|source| VulkanError::operation("create terrain depth image", source))?;
        self.depth_image = image;
        self.depth_allocation = Some(allocation);
        let range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL)
            .level_count(1)
            .layer_count(1);
        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(context.depth_format)
            .subresource_range(range);
        // SAFETY: The view covers the image's sole mip and layer.
        self.depth_view = unsafe { context.device.create_image_view(&view_info, None) }
            .map_err(|source| VulkanError::operation("create terrain depth view", source))?;
        Ok(())
    }

    fn create_commands(&mut self, context: &FrameCreateContext<'_>) -> Result<(), VulkanError> {
        let pool_info = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::TRANSIENT)
            .queue_family_index(context.graphics_queue_family);
        // SAFETY: The selected graphics family was enabled on this device.
        self.command_pool = unsafe { context.device.create_command_pool(&pool_info, None) }
            .map_err(|source| VulkanError::operation("create terrain command pool", source))?;
        let allocation_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(self.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        // SAFETY: The command pool is live and uniquely owned by this slot.
        self.command_buffer = unsafe { context.device.allocate_command_buffers(&allocation_info) }
            .map_err(|source| VulkanError::operation("allocate terrain command buffer", source))?
            .first()
            .copied()
            .ok_or_else(|| {
                VulkanError::operation("allocate terrain command buffer", "driver returned none")
            })?;
        Ok(())
    }

    fn create_synchronization(&mut self, device: &Device) -> Result<(), VulkanError> {
        // SAFETY: Default binary semaphore creation has no borrowed state.
        self.image_available =
            unsafe { device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None) }.map_err(
                |source| VulkanError::operation("create terrain acquire semaphore", source),
            )?;
        let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        // SAFETY: Fence creation has no borrowed state.
        self.fence = unsafe { device.create_fence(&fence_info, None) }
            .map_err(|source| VulkanError::operation("create terrain frame fence", source))?;
        Ok(())
    }

    fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        // SAFETY: Renderer idle guarantees no child remains in flight.
        unsafe {
            if self.fence != vk::Fence::null() {
                device.destroy_fence(self.fence, None);
                self.fence = vk::Fence::null();
            }
            if self.image_available != vk::Semaphore::null() {
                device.destroy_semaphore(self.image_available, None);
                self.image_available = vk::Semaphore::null();
            }
            if self.command_pool != vk::CommandPool::null() {
                device.destroy_command_pool(self.command_pool, None);
                self.command_pool = vk::CommandPool::null();
                self.command_buffer = vk::CommandBuffer::null();
            }
            if self.depth_view != vk::ImageView::null() {
                device.destroy_image_view(self.depth_view, None);
                self.depth_view = vk::ImageView::null();
            }
            if let Some(mut allocation) = self.depth_allocation.take() {
                allocator.destroy_image(self.depth_image, &mut allocation);
                self.depth_image = vk::Image::null();
            }
            if self.descriptor_pool != vk::DescriptorPool::null() {
                device.destroy_descriptor_pool(self.descriptor_pool, None);
                self.descriptor_pool = vk::DescriptorPool::null();
                self.scene_set = vk::DescriptorSet::null();
            }
            if let Some(mut allocation) = self.scene_allocation.take() {
                allocator.destroy_buffer(self.scene_buffer, &mut allocation);
                self.scene_buffer = vk::Buffer::null();
            }
        }
    }

    const fn empty() -> Self {
        Self {
            scene_buffer: vk::Buffer::null(),
            scene_allocation: None,
            descriptor_pool: vk::DescriptorPool::null(),
            scene_set: vk::DescriptorSet::null(),
            depth_image: vk::Image::null(),
            depth_allocation: None,
            depth_view: vk::ImageView::null(),
            command_pool: vk::CommandPool::null(),
            command_buffer: vk::CommandBuffer::null(),
            image_available: vk::Semaphore::null(),
            fence: vk::Fence::null(),
        }
    }
}

#[derive(Default)]
pub(super) struct TerrainFrameResources {
    slots: Vec<TerrainFrameSlot>,
    present_semaphores: Vec<vk::Semaphore>,
    next_slot: usize,
    extent: Option<(u32, u32)>,
}

impl TerrainFrameResources {
    pub(super) fn ensure(&mut self, context: FrameCreateContext<'_>) -> Result<(), VulkanError> {
        if !self.slots.is_empty() {
            if self.slots.len() == context.slot_count && self.extent == Some(context.extent) {
                return Ok(());
            }
            return Err(VulkanError::TerrainFrameSwapchainChanged);
        }
        if context.slot_count == 0 {
            return Err(VulkanError::TerrainFrameCapacity);
        }
        for _index in 0..context.slot_count {
            match TerrainFrameSlot::create(&context) {
                Ok(slot) => self.slots.push(slot),
                Err(error) => {
                    self.destroy(context.device, context.allocator);
                    return Err(error);
                }
            }
            // SAFETY: Default binary semaphore creation has no borrowed state.
            match unsafe {
                context
                    .device
                    .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
            } {
                Ok(semaphore) => self.present_semaphores.push(semaphore),
                Err(source) => {
                    self.destroy(context.device, context.allocator);
                    return Err(VulkanError::operation(
                        "create terrain present semaphore",
                        source,
                    ));
                }
            }
        }
        self.extent = Some(context.extent);
        Ok(())
    }

    pub(super) fn next_slot_index(&mut self) -> Result<usize, VulkanError> {
        if self.slots.is_empty() {
            return Err(VulkanError::TerrainFrameCapacity);
        }
        let index = self.next_slot;
        self.next_slot = (self.next_slot + 1) % self.slots.len();
        Ok(index)
    }

    pub(super) fn slot_mut(&mut self, index: usize) -> Result<&mut TerrainFrameSlot, VulkanError> {
        self.slots
            .get_mut(index)
            .ok_or(VulkanError::TerrainFrameCapacity)
    }

    pub(super) fn present_semaphore(&self, image_index: u32) -> Result<vk::Semaphore, VulkanError> {
        self.present_semaphores
            .get(image_index as usize)
            .copied()
            .ok_or(VulkanError::TerrainFrameCapacity)
    }

    pub(super) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        // SAFETY: Device idle guarantees no presentation wait remains live.
        unsafe {
            for semaphore in self.present_semaphores.drain(..).rev() {
                device.destroy_semaphore(semaphore, None);
            }
        }
        for slot in self.slots.iter_mut().rev() {
            slot.destroy(device, allocator);
        }
        self.slots.clear();
        self.next_slot = 0;
        self.extent = None;
    }
}
