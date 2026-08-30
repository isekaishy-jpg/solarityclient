//! Exact high-water frame-buffer layout and reusable slot ownership.

#![allow(unsafe_code)]

use ash::{Device, vk};
use glam::Mat4;
use vk_mem::Alloc;

use crate::device::VulkanError;
use crate::device::vulkan_m2_draw::M2PreparedDraw;
use crate::model::{M2MaterialUniform, M2SceneUniform};

const BONE_TRANSFORM_BYTES: vk::DeviceSize = 64;

/// Immutable inputs needed to create or grow every frame slot together.
pub(super) struct FrameCreateContext<'a> {
    pub(super) device: &'a Device,
    pub(super) allocator: &'a vk_mem::Allocator,
    pub(super) descriptor_layouts: [vk::DescriptorSetLayout; 3],
    pub(super) graphics_queue_family: u32,
    pub(super) slot_count: usize,
    pub(super) draw_capacity: usize,
    pub(super) bone_capacity: usize,
    pub(super) uniform_alignment: vk::DeviceSize,
    pub(super) storage_alignment: vk::DeviceSize,
    pub(super) extent: (u32, u32),
    pub(super) depth_format: vk::Format,
}

/// Offsets and strides inside one host-visible scene/bone/material buffer.
#[derive(Clone, Copy)]
struct FrameBufferLayout {
    bone_offset: vk::DeviceSize,
    bone_bytes: vk::DeviceSize,
    material_offset: vk::DeviceSize,
    material_stride: vk::DeviceSize,
    total_bytes: vk::DeviceSize,
}

impl FrameBufferLayout {
    /// Derives all capacity from the submitted high-water counts and device ABI.
    fn new(
        draw_capacity: usize,
        bone_capacity: usize,
        uniform_alignment: vk::DeviceSize,
        storage_alignment: vk::DeviceSize,
    ) -> Result<Self, VulkanError> {
        let scene_bytes = u64::try_from(M2SceneUniform::BYTE_SIZE)
            .map_err(|source| VulkanError::operation("convert M2 scene size", source))?;
        let bone_offset = align_up(scene_bytes, storage_alignment.max(16))?;
        // A storage descriptor cannot expose a zero-byte range. If no weighted
        // vertex exists, this inaccessible 64-byte region carries no behavior.
        let stored_bones = bone_capacity.max(1);
        let bone_bytes = u64::try_from(stored_bones)
            .ok()
            .and_then(|count| count.checked_mul(BONE_TRANSFORM_BYTES))
            .ok_or(VulkanError::M2FrameCapacity)?;
        let material_alignment = uniform_alignment.max(1);
        let material_offset = align_up(
            bone_offset
                .checked_add(bone_bytes)
                .ok_or(VulkanError::M2FrameCapacity)?,
            material_alignment,
        )?;
        let material_stride = align_up(
            u64::try_from(M2MaterialUniform::BYTE_SIZE)
                .map_err(|source| VulkanError::operation("convert M2 material size", source))?,
            material_alignment,
        )?;
        let material_bytes = u64::try_from(draw_capacity)
            .ok()
            .and_then(|count| count.checked_mul(material_stride))
            .ok_or(VulkanError::M2FrameCapacity)?;
        let total_bytes = material_offset
            .checked_add(material_bytes)
            .ok_or(VulkanError::M2FrameCapacity)?;
        let last_dynamic_offset = draw_capacity
            .saturating_sub(1)
            .try_into()
            .ok()
            .and_then(|index: u64| index.checked_mul(material_stride))
            .ok_or(VulkanError::M2FrameCapacity)?;
        u32::try_from(last_dynamic_offset).map_err(|_source| VulkanError::M2FrameCapacity)?;
        Ok(Self {
            bone_offset,
            bone_bytes,
            material_offset,
            material_stride,
            total_bytes,
        })
    }
}

/// One independently synchronized command/buffer/depth slot.
pub(super) struct M2FrameSlot {
    buffer: vk::Buffer,
    buffer_allocation: Option<vk_mem::Allocation>,
    layout: FrameBufferLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: [vk::DescriptorSet; 3],
    depth_image: vk::Image,
    depth_allocation: Option<vk_mem::Allocation>,
    depth_view: vk::ImageView,
    command_pool: vk::CommandPool,
    command_buffer: vk::CommandBuffer,
    image_available: vk::Semaphore,
    fence: vk::Fence,
}

impl M2FrameSlot {
    /// Creates every persistent child with cleanup on any partial failure.
    fn create(
        context: &FrameCreateContext<'_>,
        layout: FrameBufferLayout,
    ) -> Result<Self, VulkanError> {
        let mut slot = Self::empty(layout);
        let result = (|| {
            slot.create_buffer(context)?;
            slot.create_descriptors(context)?;
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

    /// Returns the acquire semaphore for this slot's next frame.
    pub(super) const fn image_available(&self) -> vk::Semaphore {
        self.image_available
    }

    /// Returns the submission fence used to protect slot reuse.
    pub(super) const fn fence(&self) -> vk::Fence {
        self.fence
    }

    /// Returns the sole reset-and-rerecorded primary command buffer.
    pub(super) const fn command_buffer(&self) -> vk::CommandBuffer {
        self.command_buffer
    }

    /// Returns the scene, bone, and dynamic-material sets in pipeline order.
    pub(super) const fn descriptor_sets(&self) -> [vk::DescriptorSet; 3] {
        self.descriptor_sets
    }

    /// Returns the slot-local depth/stencil attachment view.
    pub(super) const fn depth_view(&self) -> vk::ImageView {
        self.depth_view
    }

    /// Returns the slot-local depth/stencil attachment image.
    pub(super) const fn depth_image(&self) -> vk::Image {
        self.depth_image
    }

    /// Returns the material dynamic-offset stride.
    pub(super) const fn material_stride(&self) -> vk::DeviceSize {
        self.layout.material_stride
    }

    /// Waits for prior GPU use, then resets the command pool for rerecording.
    pub(super) fn wait_and_reset(&self, device: &Device) -> Result<(), VulkanError> {
        // SAFETY: The fence and command pool belong to this live device. The
        // fence was created signaled or submitted exactly once since reset.
        unsafe {
            device
                .wait_for_fences(&[self.fence], true, u64::MAX)
                .map_err(|source| VulkanError::operation("wait for M2 frame slot", source))?;
            device
                .reset_command_pool(self.command_pool, vk::CommandPoolResetFlags::empty())
                .map_err(|source| VulkanError::operation("reset M2 frame command pool", source))?;
        }
        Ok(())
    }

    /// Resets the signaled fence immediately before queue submission.
    pub(super) fn reset_fence(&self, device: &Device) -> Result<(), VulkanError> {
        // SAFETY: Prior use was waited above and no submission currently owns it.
        unsafe { device.reset_fences(&[self.fence]) }
            .map_err(|source| VulkanError::operation("reset M2 frame fence", source))
    }

    /// Replaces a reset fence when queue submission itself fails.
    pub(super) fn restore_signaled_fence(&mut self, device: &Device) -> Result<(), VulkanError> {
        // SAFETY: A failed queue submission did not take ownership of the fence.
        unsafe { device.destroy_fence(self.fence, None) };
        self.fence = vk::Fence::null();
        let info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        // SAFETY: The replacement has no external payload or borrowed state.
        self.fence = unsafe { device.create_fence(&info, None) }
            .map_err(|source| VulkanError::operation("restore M2 frame fence", source))?;
        Ok(())
    }

    /// Writes the exact frame snapshot into one persistently allocated buffer.
    pub(super) fn write(
        &mut self,
        allocator: &vk_mem::Allocator,
        scene: M2SceneUniform,
        bone_transforms: &[Mat4],
        draws: &[M2PreparedDraw],
    ) -> Result<(), VulkanError> {
        let allocation = self.buffer_allocation.as_mut().ok_or_else(|| {
            VulkanError::operation("access M2 frame buffer", "allocation is unavailable")
        })?;
        // SAFETY: The allocation is host-visible and covers `layout.total_bytes`.
        let destination = unsafe { allocator.map_memory(allocation) }
            .map_err(|source| VulkanError::operation("map M2 frame buffer", source))?;
        let write_result = (|| {
            copy_bytes(destination, 0, &scene.to_bytes(), self.layout.total_bytes)?;
            if bone_transforms.is_empty() {
                copy_bytes(
                    destination,
                    self.layout.bone_offset,
                    &[0_u8; BONE_TRANSFORM_BYTES as usize],
                    self.layout.total_bytes,
                )?;
            } else {
                for (index, transform) in bone_transforms.iter().copied().enumerate() {
                    let offset =
                        indexed_offset(self.layout.bone_offset, BONE_TRANSFORM_BYTES, index)?;
                    copy_bytes(
                        destination,
                        offset,
                        &matrix_bytes(transform),
                        self.layout.total_bytes,
                    )?;
                }
            }
            for (index, draw) in draws.iter().copied().enumerate() {
                let offset = indexed_offset(
                    self.layout.material_offset,
                    self.layout.material_stride,
                    index,
                )?;
                copy_bytes(
                    destination,
                    offset,
                    &draw.material().to_bytes(),
                    self.layout.total_bytes,
                )?;
            }
            allocator
                .flush_allocation(allocation, 0, vk::WHOLE_SIZE)
                .map_err(|source| VulkanError::operation("flush M2 frame buffer", source))
        })();
        // SAFETY: This balances the successful map on every write/flush result.
        unsafe { allocator.unmap_memory(allocation) };
        write_result
    }

    /// Releases every child in reverse dependency order.
    pub(super) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        // SAFETY: Non-null handles are uniquely owned by this slot and no longer
        // in flight because the renderer waits idle before destruction/rebuild.
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
                self.descriptor_sets = [vk::DescriptorSet::null(); 3];
            }
            if let Some(mut allocation) = self.buffer_allocation.take() {
                allocator.destroy_buffer(self.buffer, &mut allocation);
                self.buffer = vk::Buffer::null();
            }
        }
    }

    /// Creates an inert owner for partial-construction cleanup.
    const fn empty(layout: FrameBufferLayout) -> Self {
        Self {
            buffer: vk::Buffer::null(),
            buffer_allocation: None,
            layout,
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_sets: [vk::DescriptorSet::null(); 3],
            depth_image: vk::Image::null(),
            depth_allocation: None,
            depth_view: vk::ImageView::null(),
            command_pool: vk::CommandPool::null(),
            command_buffer: vk::CommandBuffer::null(),
            image_available: vk::Semaphore::null(),
            fence: vk::Fence::null(),
        }
    }

    /// Allocates the combined host-visible descriptor payload.
    fn create_buffer(&mut self, context: &FrameCreateContext<'_>) -> Result<(), VulkanError> {
        let buffer_info = vk::BufferCreateInfo::default()
            .size(self.layout.total_bytes)
            .usage(vk::BufferUsageFlags::UNIFORM_BUFFER | vk::BufferUsageFlags::STORAGE_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let allocation_info = vk_mem::AllocationCreateInfo {
            flags: vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
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
        .map_err(|source| VulkanError::operation("create M2 frame buffer", source))?;
        self.buffer = buffer;
        self.buffer_allocation = Some(allocation);
        Ok(())
    }

    /// Allocates and writes the three non-texture descriptor sets.
    fn create_descriptors(&mut self, context: &FrameCreateContext<'_>) -> Result<(), VulkanError> {
        let pool_sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
                .descriptor_count(1),
        ];
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(3)
            .pool_sizes(&pool_sizes);
        // SAFETY: Pool counts exactly cover the three compatible layouts.
        self.descriptor_pool = unsafe { context.device.create_descriptor_pool(&pool_info, None) }
            .map_err(|source| {
            VulkanError::operation("create M2 frame descriptor pool", source)
        })?;
        let allocate_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.descriptor_pool)
            .set_layouts(&context.descriptor_layouts);
        // SAFETY: The pool and all three layouts are live.
        let sets = unsafe { context.device.allocate_descriptor_sets(&allocate_info) }
            .map_err(|source| VulkanError::operation("allocate M2 frame descriptors", source))?;
        if sets.len() != self.descriptor_sets.len() {
            return Err(VulkanError::operation(
                "allocate M2 frame descriptors",
                "driver returned an unexpected set count",
            ));
        }
        self.descriptor_sets.copy_from_slice(&sets);

        let scene_info = vk::DescriptorBufferInfo::default()
            .buffer(self.buffer)
            .offset(0)
            .range(M2SceneUniform::BYTE_SIZE as vk::DeviceSize);
        let bone_info = vk::DescriptorBufferInfo::default()
            .buffer(self.buffer)
            .offset(self.layout.bone_offset)
            .range(self.layout.bone_bytes);
        let material_info = vk::DescriptorBufferInfo::default()
            .buffer(self.buffer)
            .offset(self.layout.material_offset)
            .range(M2MaterialUniform::BYTE_SIZE as vk::DeviceSize);
        let scene_infos = [scene_info];
        let bone_infos = [bone_info];
        let material_infos = [material_info];
        let writes = [
            descriptor_write(
                self.descriptor_sets[0],
                vk::DescriptorType::UNIFORM_BUFFER,
                &scene_infos,
            ),
            descriptor_write(
                self.descriptor_sets[1],
                vk::DescriptorType::STORAGE_BUFFER,
                &bone_infos,
            ),
            descriptor_write(
                self.descriptor_sets[2],
                vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC,
                &material_infos,
            ),
        ];
        // SAFETY: Every descriptor range lies within the live combined buffer.
        unsafe { context.device.update_descriptor_sets(&writes, &[]) };
        Ok(())
    }

    /// Allocates the slot-local device depth/stencil image and view.
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
        // SAFETY: VMA binds the returned device allocation to this image.
        let (image, allocation) = unsafe {
            context
                .allocator
                .create_image(&image_info, &allocation_info)
        }
        .map_err(|source| VulkanError::operation("create M2 frame depth image", source))?;
        self.depth_image = image;
        self.depth_allocation = Some(allocation);
        let range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL)
            .base_mip_level(0)
            .level_count(1)
            .base_array_layer(0)
            .layer_count(1);
        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(context.depth_format)
            .subresource_range(range);
        // SAFETY: The image is live and the view covers its sole mip/layer.
        self.depth_view = unsafe { context.device.create_image_view(&view_info, None) }
            .map_err(|source| VulkanError::operation("create M2 frame depth view", source))?;
        Ok(())
    }

    /// Creates one resettable pool and primary buffer for this slot.
    fn create_commands(&mut self, context: &FrameCreateContext<'_>) -> Result<(), VulkanError> {
        let pool_info = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::TRANSIENT)
            .queue_family_index(context.graphics_queue_family);
        // SAFETY: The selected family was enabled on this device.
        self.command_pool = unsafe { context.device.create_command_pool(&pool_info, None) }
            .map_err(|source| VulkanError::operation("create M2 frame command pool", source))?;
        let allocate_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(self.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        // SAFETY: The pool is live and uniquely owned by this slot.
        self.command_buffer = unsafe { context.device.allocate_command_buffers(&allocate_info) }
            .map_err(|source| VulkanError::operation("allocate M2 frame command buffer", source))?
            .first()
            .copied()
            .ok_or_else(|| {
                VulkanError::operation("allocate M2 frame command buffer", "driver returned none")
            })?;
        Ok(())
    }

    /// Creates one acquire semaphore and an initially reusable fence.
    fn create_synchronization(&mut self, device: &Device) -> Result<(), VulkanError> {
        let semaphore_info = vk::SemaphoreCreateInfo::default();
        // SAFETY: Default binary semaphore creation has no borrowed state.
        self.image_available = unsafe { device.create_semaphore(&semaphore_info, None) }
            .map_err(|source| VulkanError::operation("create M2 acquire semaphore", source))?;
        let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        // SAFETY: The signaled fence has no external payload or borrowed state.
        self.fence = unsafe { device.create_fence(&fence_info, None) }
            .map_err(|source| VulkanError::operation("create M2 frame fence", source))?;
        Ok(())
    }
}

/// High-water owner rebuilt only when the scene outgrows current capacity.
#[derive(Default)]
pub(super) struct M2FrameResources {
    slots: Vec<M2FrameSlot>,
    present_semaphores: Vec<vk::Semaphore>,
    next_slot: usize,
    draw_capacity: usize,
    bone_capacity: usize,
}

impl M2FrameResources {
    /// Creates or grows all slots together without arbitrary capacity constants.
    pub(super) fn ensure(&mut self, context: FrameCreateContext<'_>) -> Result<(), VulkanError> {
        if !self.slots.is_empty()
            && self.slots.len() == context.slot_count
            && self.draw_capacity >= context.draw_capacity
            && self.bone_capacity >= context.bone_capacity
        {
            return Ok(());
        }
        if context.slot_count == 0 {
            return Err(VulkanError::M2FrameCapacity);
        }
        // SAFETY: Rebuilding invalidates old buffers/descriptors; device idle
        // proves no prior slot or texture descriptor is still referenced.
        unsafe { context.device.device_wait_idle() }
            .map_err(|source| VulkanError::operation("idle before M2 frame growth", source))?;
        let draw_capacity = self.draw_capacity.max(context.draw_capacity);
        let bone_capacity = self.bone_capacity.max(context.bone_capacity);
        self.destroy(context.device, context.allocator);
        let layout = FrameBufferLayout::new(
            draw_capacity,
            bone_capacity,
            context.uniform_alignment,
            context.storage_alignment,
        )?;
        let mut slots = Vec::with_capacity(context.slot_count);
        for _ in 0..context.slot_count {
            match M2FrameSlot::create(&context, layout) {
                Ok(slot) => slots.push(slot),
                Err(error) => {
                    for slot in &mut slots {
                        slot.destroy(context.device, context.allocator);
                    }
                    return Err(error);
                }
            }
        }
        let mut present_semaphores = Vec::with_capacity(context.slot_count);
        let semaphore_info = vk::SemaphoreCreateInfo::default();
        for _ in 0..context.slot_count {
            // SAFETY: Default binary semaphore creation has no borrowed state.
            match unsafe { context.device.create_semaphore(&semaphore_info, None) } {
                Ok(semaphore) => present_semaphores.push(semaphore),
                Err(source) => {
                    // SAFETY: These semaphores have never been submitted.
                    unsafe {
                        for semaphore in present_semaphores.drain(..).rev() {
                            context.device.destroy_semaphore(semaphore, None);
                        }
                    }
                    for slot in &mut slots {
                        slot.destroy(context.device, context.allocator);
                    }
                    return Err(VulkanError::operation(
                        "create M2 presentation semaphore",
                        source,
                    ));
                }
            }
        }
        self.slots = slots;
        self.present_semaphores = present_semaphores;
        self.draw_capacity = draw_capacity;
        self.bone_capacity = bone_capacity;
        self.next_slot = 0;
        Ok(())
    }

    /// Advances round-robin and returns one independently fenced slot index.
    pub(super) fn next_slot_index(&mut self) -> Result<usize, VulkanError> {
        if self.slots.is_empty() {
            return Err(VulkanError::M2FrameCapacity);
        }
        let index = self.next_slot;
        self.next_slot = (self.next_slot + 1) % self.slots.len();
        Ok(index)
    }

    /// Returns one mutable frame slot by a previously selected index.
    pub(super) fn slot_mut(&mut self, index: usize) -> Result<&mut M2FrameSlot, VulkanError> {
        self.slots
            .get_mut(index)
            .ok_or_else(|| VulkanError::operation("select M2 frame slot", "slot is unavailable"))
    }

    /// Returns the semaphore safe to signal after this image is reacquired.
    pub(super) fn present_semaphore(&self, image_index: u32) -> Result<vk::Semaphore, VulkanError> {
        self.present_semaphores
            .get(image_index as usize)
            .copied()
            .ok_or_else(|| {
                VulkanError::operation(
                    "select M2 presentation semaphore",
                    "image index is out of range",
                )
            })
    }

    /// Releases all slots after caller-provided GPU retirement.
    pub(super) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        // SAFETY: Device idle is guaranteed before rebuild/renderer teardown,
        // so no presentation wait can still reference these semaphores.
        unsafe {
            for semaphore in self.present_semaphores.drain(..).rev() {
                device.destroy_semaphore(semaphore, None);
            }
        }
        for mut slot in self.slots.drain(..).rev() {
            slot.destroy(device, allocator);
        }
        self.next_slot = 0;
        self.draw_capacity = 0;
        self.bone_capacity = 0;
    }
}

/// Builds one single-buffer descriptor write.
fn descriptor_write<'a>(
    set: vk::DescriptorSet,
    descriptor_type: vk::DescriptorType,
    infos: &'a [vk::DescriptorBufferInfo],
) -> vk::WriteDescriptorSet<'a> {
    vk::WriteDescriptorSet::default()
        .dst_set(set)
        .dst_binding(0)
        .descriptor_type(descriptor_type)
        .buffer_info(infos)
}

/// Rounds upward to a nonzero device alignment with overflow checks.
fn align_up(
    value: vk::DeviceSize,
    alignment: vk::DeviceSize,
) -> Result<vk::DeviceSize, VulkanError> {
    let alignment = alignment.max(1);
    value
        .checked_add(alignment - 1)
        .map(|sum| sum / alignment * alignment)
        .ok_or(VulkanError::M2FrameCapacity)
}

/// Computes one indexed byte offset without overflowing host/Vulkan sizes.
fn indexed_offset(
    base: vk::DeviceSize,
    stride: vk::DeviceSize,
    index: usize,
) -> Result<vk::DeviceSize, VulkanError> {
    u64::try_from(index)
        .ok()
        .and_then(|index| index.checked_mul(stride))
        .and_then(|offset| base.checked_add(offset))
        .ok_or(VulkanError::M2FrameCapacity)
}

/// Copies one fixed block into mapped memory after exact bounds validation.
fn copy_bytes(
    destination: *mut u8,
    offset: vk::DeviceSize,
    bytes: &[u8],
    total: vk::DeviceSize,
) -> Result<(), VulkanError> {
    let byte_count = u64::try_from(bytes.len()).map_err(|_source| VulkanError::M2FrameCapacity)?;
    let end = offset
        .checked_add(byte_count)
        .ok_or(VulkanError::M2FrameCapacity)?;
    if end > total {
        return Err(VulkanError::M2FrameCapacity);
    }
    let offset = usize::try_from(offset).map_err(|_source| VulkanError::M2FrameCapacity)?;
    // SAFETY: Bounds above prove the destination subrange lies inside the live
    // mapped allocation, and source/destination do not overlap.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), destination.add(offset), bytes.len()) };
    Ok(())
}

/// Serializes one column-major model-bone matrix without allocating.
fn matrix_bytes(matrix: Mat4) -> [u8; BONE_TRANSFORM_BYTES as usize] {
    let mut bytes = [0_u8; BONE_TRANSFORM_BYTES as usize];
    for (index, value) in matrix.to_cols_array().into_iter().enumerate() {
        let offset = index * size_of::<f32>();
        bytes[offset..offset + size_of::<f32>()].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}
