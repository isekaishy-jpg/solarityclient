//! Persistent source allocation and independently fenced presentation slots.

#![allow(unsafe_code)]

use std::ptr;

use ash::{Device, vk};
use vk_mem::Alloc;

use crate::device::VulkanError;

use super::CinematicFrameIdentity;

/// One command buffer whose resources stay live until its fence retires.
pub(super) struct FrameSlot {
    command_pool: vk::CommandPool,
    command_buffer: vk::CommandBuffer,
    image_available: vk::Semaphore,
    fence: vk::Fence,
}

impl FrameSlot {
    /// Creates one resettable command arena and synchronization pair.
    fn create(device: &Device, queue_family: u32) -> Result<Self, VulkanError> {
        let pool_info = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(queue_family);
        // SAFETY: The queue family was enabled on this live device.
        let command_pool = unsafe { device.create_command_pool(&pool_info, None) }
            .map_err(|source| VulkanError::operation("create cinematic command pool", source))?;
        let allocate_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        // SAFETY: The pool is live and uniquely owned here.
        let command_buffer = match unsafe { device.allocate_command_buffers(&allocate_info) } {
            Ok(buffers) => match buffers.first().copied() {
                Some(buffer) => buffer,
                None => {
                    // SAFETY: No command buffer escaped the empty allocation.
                    unsafe { device.destroy_command_pool(command_pool, None) };
                    return Err(VulkanError::operation(
                        "allocate cinematic command buffer",
                        "driver returned none",
                    ));
                }
            },
            Err(source) => {
                // SAFETY: The pool has no submitted children.
                unsafe { device.destroy_command_pool(command_pool, None) };
                return Err(VulkanError::operation(
                    "allocate cinematic command buffer",
                    source,
                ));
            }
        };
        // SAFETY: Default semaphore creation borrows no external state.
        let image_available =
            match unsafe { device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None) } {
                Ok(semaphore) => semaphore,
                Err(source) => {
                    // SAFETY: The command pool transitively releases its buffer.
                    unsafe { device.destroy_command_pool(command_pool, None) };
                    return Err(VulkanError::operation(
                        "create cinematic acquire semaphore",
                        source,
                    ));
                }
            };
        let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        // SAFETY: Fence creation has no borrowed state.
        let fence = match unsafe { device.create_fence(&fence_info, None) } {
            Ok(fence) => fence,
            Err(source) => {
                // SAFETY: Neither child is in flight.
                unsafe {
                    device.destroy_semaphore(image_available, None);
                    device.destroy_command_pool(command_pool, None);
                }
                return Err(VulkanError::operation("create cinematic fence", source));
            }
        };
        Ok(Self {
            command_pool,
            command_buffer,
            image_available,
            fence,
        })
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

    /// Waits for prior use and resets the complete command arena.
    pub(super) fn wait_and_reset(&self, device: &Device) -> Result<(), VulkanError> {
        // SAFETY: The fence begins signaled and is reset only immediately before submit.
        unsafe {
            device
                .wait_for_fences(&[self.fence], true, u64::MAX)
                .map_err(|source| VulkanError::operation("wait for cinematic frame", source))?;
            device
                .reset_command_pool(self.command_pool, vk::CommandPoolResetFlags::empty())
                .map_err(|source| VulkanError::operation("reset cinematic command pool", source))?;
        }
        Ok(())
    }

    pub(super) fn reset_fence(&self, device: &Device) -> Result<(), VulkanError> {
        // SAFETY: Prior use was waited and no submission currently owns it.
        unsafe { device.reset_fences(&[self.fence]) }
            .map_err(|source| VulkanError::operation("reset cinematic fence", source))
    }

    /// Restores the signaled invariant when queue submission fails.
    pub(super) fn restore_signaled_fence(&mut self, device: &Device) -> Result<(), VulkanError> {
        // SAFETY: Failed submission did not take ownership of the old fence.
        unsafe { device.destroy_fence(self.fence, None) };
        let info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        // SAFETY: The replacement has no external payload.
        self.fence = unsafe { device.create_fence(&info, None) }
            .map_err(|source| VulkanError::operation("restore cinematic fence", source))?;
        Ok(())
    }

    fn destroy(&mut self, device: &Device) {
        // SAFETY: Renderer idle guarantees no child remains in flight.
        unsafe {
            device.destroy_fence(self.fence, None);
            device.destroy_semaphore(self.image_available, None);
            device.destroy_command_pool(self.command_pool, None);
        }
    }
}

/// Device-local decoded pixels plus one persistently mapped upload buffer.
struct FrameSource {
    extent: (u32, u32),
    staging_buffer: vk::Buffer,
    staging_allocation: Option<vk_mem::Allocation>,
    image: vk::Image,
    image_allocation: Option<vk_mem::Allocation>,
    initialized: bool,
}

impl FrameSource {
    /// Allocates exact source-sized resources without resampling the decoder output.
    fn create(allocator: &vk_mem::Allocator, extent: (u32, u32)) -> Result<Self, VulkanError> {
        let byte_count = u64::from(extent.0)
            .checked_mul(u64::from(extent.1))
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(VulkanError::FrameSize)?;
        let buffer_info = vk::BufferCreateInfo::default()
            .size(byte_count)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer_allocation_info = vk_mem::AllocationCreateInfo {
            flags: vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE
                | vk_mem::AllocationCreateFlags::MAPPED,
            usage: vk_mem::MemoryUsage::AutoPreferHost,
            required_flags: vk::MemoryPropertyFlags::HOST_VISIBLE,
            ..Default::default()
        };
        // SAFETY: VMA binds one allocation to the exact upload buffer.
        let (staging_buffer, staging_allocation) =
            unsafe { allocator.create_buffer(&buffer_info, &buffer_allocation_info) }.map_err(
                |source| VulkanError::operation("create cinematic staging buffer", source),
            )?;
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::R8G8B8A8_UNORM)
            .extent(vk::Extent3D {
                width: extent.0,
                height: extent.1,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let image_allocation_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::AutoPreferDevice,
            required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
            ..Default::default()
        };
        // SAFETY: VMA creates and binds the device-local decoded image.
        let (image, image_allocation) =
            match unsafe { allocator.create_image(&image_info, &image_allocation_info) } {
                Ok(result) => result,
                Err(source) => {
                    let mut staging_allocation = staging_allocation;
                    // SAFETY: No command references this newly allocated buffer.
                    unsafe { allocator.destroy_buffer(staging_buffer, &mut staging_allocation) };
                    return Err(VulkanError::operation("create cinematic image", source));
                }
            };
        Ok(Self {
            extent,
            staging_buffer,
            staging_allocation: Some(staging_allocation),
            image,
            image_allocation: Some(image_allocation),
            initialized: false,
        })
    }

    /// Copies a complete decoded frame into the fenced persistent mapping.
    fn write(&self, allocator: &vk_mem::Allocator, rgba8: &[u8]) -> Result<(), VulkanError> {
        let allocation = self.staging_allocation.as_ref().ok_or_else(|| {
            VulkanError::operation(
                "access cinematic staging buffer",
                "allocation is unavailable",
            )
        })?;
        let destination = allocator
            .get_allocation_info(allocation)
            .mapped_data
            .cast::<u8>();
        if destination.is_null() {
            return Err(VulkanError::operation(
                "access cinematic staging mapping",
                "persistent mapping is unavailable",
            ));
        }
        // SAFETY: Pixel validation proves the slice exactly fills this source allocation,
        // and all prior source-reading fences were waited before this write.
        unsafe { ptr::copy_nonoverlapping(rgba8.as_ptr(), destination, rgba8.len()) };
        allocator
            .flush_allocation(allocation, 0, rgba8.len() as vk::DeviceSize)
            .map_err(|source| VulkanError::operation("flush cinematic staging buffer", source))
    }

    fn destroy(&mut self, allocator: &vk_mem::Allocator) {
        // SAFETY: Renderer idle or all frame fences guarantee no command references these.
        unsafe {
            if let Some(mut allocation) = self.image_allocation.take() {
                allocator.destroy_image(self.image, &mut allocation);
            }
            if let Some(mut allocation) = self.staging_allocation.take() {
                allocator.destroy_buffer(self.staging_buffer, &mut allocation);
            }
        }
    }
}

/// Retained source plus a presentation ring matching the current swapchain.
#[derive(Default)]
pub(super) struct FrameResources {
    source: Option<FrameSource>,
    identity: Option<CinematicFrameIdentity>,
    slots: Vec<FrameSlot>,
    present_semaphores: Vec<vk::Semaphore>,
    next_slot: usize,
}

impl FrameResources {
    /// Lazily creates the exact swapchain-sized synchronization graph.
    pub(super) fn ensure_slots(
        &mut self,
        device: &Device,
        queue_family: u32,
        count: usize,
    ) -> Result<(), VulkanError> {
        if self.slots.len() == count && self.present_semaphores.len() == count {
            return Ok(());
        }
        if !self.slots.is_empty() || !self.present_semaphores.is_empty() {
            return Err(VulkanError::FrameSwapchainChanged);
        }
        for _index in 0..count {
            match FrameSlot::create(device, queue_family) {
                Ok(slot) => self.slots.push(slot),
                Err(error) => {
                    self.destroy_slots(device);
                    return Err(error);
                }
            }
            // SAFETY: Default binary semaphore creation borrows no external state.
            match unsafe { device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None) } {
                Ok(semaphore) => self.present_semaphores.push(semaphore),
                Err(source) => {
                    self.destroy_slots(device);
                    return Err(VulkanError::operation(
                        "create cinematic present semaphore",
                        source,
                    ));
                }
            }
        }
        Ok(())
    }

    pub(super) fn requires_upload(
        &self,
        extent: (u32, u32),
        identity: Option<CinematicFrameIdentity>,
    ) -> bool {
        self.source
            .as_ref()
            .is_none_or(|source| source.extent != extent)
            || identity.is_none()
            || self.identity != identity
    }

    /// Waits every source reader before decoded pixels may be replaced.
    pub(super) fn wait_all(&self, device: &Device) -> Result<(), VulkanError> {
        let fences = self.slots.iter().map(FrameSlot::fence).collect::<Vec<_>>();
        if fences.is_empty() {
            return Ok(());
        }
        // SAFETY: Every fence is live and owned by this resource graph.
        unsafe { device.wait_for_fences(&fences, true, u64::MAX) }
            .map_err(|source| VulkanError::operation("wait for cinematic source readers", source))
    }

    pub(super) fn ensure_source(
        &mut self,
        _device: &Device,
        allocator: &vk_mem::Allocator,
        extent: (u32, u32),
    ) -> Result<(), VulkanError> {
        if self
            .source
            .as_ref()
            .is_some_and(|source| source.extent == extent)
        {
            return Ok(());
        }
        if let Some(mut source) = self.source.take() {
            source.destroy(allocator);
        }
        self.identity = None;
        self.source = Some(FrameSource::create(allocator, extent)?);
        Ok(())
    }

    pub(super) fn write_source(
        &self,
        allocator: &vk_mem::Allocator,
        rgba8: &[u8],
    ) -> Result<(), VulkanError> {
        self.source
            .as_ref()
            .ok_or(VulkanError::FrameCapacity)?
            .write(allocator, rgba8)
    }

    pub(super) fn source_image(&self) -> Result<vk::Image, VulkanError> {
        self.source
            .as_ref()
            .map(|source| source.image)
            .ok_or(VulkanError::FrameCapacity)
    }

    pub(super) fn source_buffer(&self) -> Result<vk::Buffer, VulkanError> {
        self.source
            .as_ref()
            .map(|source| source.staging_buffer)
            .ok_or(VulkanError::FrameCapacity)
    }

    pub(super) fn source_initialized(&self) -> Result<bool, VulkanError> {
        self.source
            .as_ref()
            .map(|source| source.initialized)
            .ok_or(VulkanError::FrameCapacity)
    }

    pub(super) fn commit_upload(
        &mut self,
        identity: Option<CinematicFrameIdentity>,
    ) -> Result<(), VulkanError> {
        let source = self.source.as_mut().ok_or(VulkanError::FrameCapacity)?;
        source.initialized = true;
        self.identity = identity;
        Ok(())
    }

    pub(super) fn next_slot_index(&mut self) -> Result<usize, VulkanError> {
        if self.slots.is_empty() {
            return Err(VulkanError::FrameCapacity);
        }
        let index = self.next_slot;
        self.next_slot = (self.next_slot + 1) % self.slots.len();
        Ok(index)
    }

    pub(super) fn slot_mut(&mut self, index: usize) -> Result<&mut FrameSlot, VulkanError> {
        self.slots.get_mut(index).ok_or(VulkanError::FrameCapacity)
    }

    pub(super) fn present_semaphore(&self, image: u32) -> Result<vk::Semaphore, VulkanError> {
        self.present_semaphores
            .get(image as usize)
            .copied()
            .ok_or(VulkanError::FrameCapacity)
    }

    pub(super) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        if let Some(mut source) = self.source.take() {
            source.destroy(allocator);
        }
        self.identity = None;
        self.destroy_slots(device);
    }

    fn destroy_slots(&mut self, device: &Device) {
        // SAFETY: Renderer idle guarantees no present wait uses these semaphores.
        unsafe {
            for semaphore in self.present_semaphores.drain(..).rev() {
                device.destroy_semaphore(semaphore, None);
            }
        }
        for slot in self.slots.iter_mut().rev() {
            slot.destroy(device);
        }
        self.slots.clear();
        self.next_slot = 0;
    }
}
