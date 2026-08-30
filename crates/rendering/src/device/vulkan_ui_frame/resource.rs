//! Reusable command and synchronization slots for UI presentation.

#![allow(unsafe_code)]

use ash::{Device, vk};

use crate::device::VulkanError;

/// One independently fenced command slot.
pub(super) struct UiFrameSlot {
    command_pool: vk::CommandPool,
    command_buffer: vk::CommandBuffer,
    image_available: vk::Semaphore,
    fence: vk::Fence,
}

impl UiFrameSlot {
    /// Creates one resettable primary command buffer and synchronization pair.
    fn create(device: &Device, queue_family: u32) -> Result<Self, VulkanError> {
        let pool_info = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(queue_family);
        // SAFETY: The graphics family was enabled on this device.
        let command_pool = unsafe { device.create_command_pool(&pool_info, None) }
            .map_err(|source| VulkanError::operation("create UI frame command pool", source))?;
        let allocate_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        // SAFETY: The pool is live and uniquely owned by the slot.
        let command_buffer = match unsafe { device.allocate_command_buffers(&allocate_info) } {
            Ok(buffers) => match buffers.first().copied() {
                Some(buffer) => buffer,
                None => {
                    // SAFETY: No command buffer escaped this empty allocation.
                    unsafe { device.destroy_command_pool(command_pool, None) };
                    return Err(VulkanError::operation(
                        "allocate UI frame command buffer",
                        "driver returned none",
                    ));
                }
            },
            Err(source) => {
                // SAFETY: The pool is live and has no submitted commands.
                unsafe { device.destroy_command_pool(command_pool, None) };
                return Err(VulkanError::operation(
                    "allocate UI frame command buffer",
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
                        "create UI acquire semaphore",
                        source,
                    ));
                }
            };
        let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        // SAFETY: Fence creation has no borrowed state.
        let fence = match unsafe { device.create_fence(&fence_info, None) } {
            Ok(fence) => fence,
            Err(source) => {
                // SAFETY: Both live children are not in flight.
                unsafe {
                    device.destroy_semaphore(image_available, None);
                    device.destroy_command_pool(command_pool, None);
                }
                return Err(VulkanError::operation("create UI frame fence", source));
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

    /// Waits for prior use and resets this slot's complete command arena.
    pub(super) fn wait_and_reset(&self, device: &Device) -> Result<(), VulkanError> {
        // SAFETY: The fence was created signaled or submitted once since reset.
        unsafe {
            device
                .wait_for_fences(&[self.fence], true, u64::MAX)
                .map_err(|source| VulkanError::operation("wait for UI frame slot", source))?;
            device
                .reset_command_pool(self.command_pool, vk::CommandPoolResetFlags::empty())
                .map_err(|source| VulkanError::operation("reset UI frame command pool", source))?;
        }
        Ok(())
    }

    pub(super) fn reset_fence(&self, device: &Device) -> Result<(), VulkanError> {
        // SAFETY: Prior use was waited and no submission currently owns it.
        unsafe { device.reset_fences(&[self.fence]) }
            .map_err(|source| VulkanError::operation("reset UI frame fence", source))
    }

    /// Replaces a reset fence after queue submission itself fails.
    pub(super) fn restore_signaled_fence(&mut self, device: &Device) -> Result<(), VulkanError> {
        // SAFETY: Failed submission did not take ownership of the old fence.
        unsafe { device.destroy_fence(self.fence, None) };
        let info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        // SAFETY: The replacement has no external payload.
        self.fence = unsafe { device.create_fence(&info, None) }
            .map_err(|source| VulkanError::operation("restore UI frame fence", source))?;
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

/// Slot ring plus one presentation semaphore per swapchain image.
#[derive(Default)]
pub(super) struct UiFrameResources {
    slots: Vec<UiFrameSlot>,
    present_semaphores: Vec<vk::Semaphore>,
    next_slot: usize,
}

impl UiFrameResources {
    /// Lazily creates the exact swapchain-sized resource graph.
    pub(super) fn ensure(
        &mut self,
        device: &Device,
        queue_family: u32,
        slot_count: usize,
    ) -> Result<(), VulkanError> {
        if self.slots.len() == slot_count && self.present_semaphores.len() == slot_count {
            return Ok(());
        }
        if !self.slots.is_empty() || !self.present_semaphores.is_empty() {
            return Err(VulkanError::UiFrameSwapchainChanged);
        }
        for _index in 0..slot_count {
            match UiFrameSlot::create(device, queue_family) {
                Ok(slot) => self.slots.push(slot),
                Err(error) => {
                    self.destroy(device);
                    return Err(error);
                }
            }
            // SAFETY: Default binary semaphore creation has no borrowed state.
            match unsafe { device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None) } {
                Ok(semaphore) => self.present_semaphores.push(semaphore),
                Err(source) => {
                    self.destroy(device);
                    return Err(VulkanError::operation(
                        "create UI present semaphore",
                        source,
                    ));
                }
            }
        }
        Ok(())
    }

    pub(super) fn next_slot_index(&mut self) -> Result<usize, VulkanError> {
        if self.slots.is_empty() {
            return Err(VulkanError::UiFrameCapacity);
        }
        let index = self.next_slot;
        self.next_slot = (self.next_slot + 1) % self.slots.len();
        Ok(index)
    }

    pub(super) fn slot_mut(&mut self, index: usize) -> Result<&mut UiFrameSlot, VulkanError> {
        self.slots
            .get_mut(index)
            .ok_or(VulkanError::UiFrameCapacity)
    }

    pub(super) fn present_semaphore(&self, image: u32) -> Result<vk::Semaphore, VulkanError> {
        self.present_semaphores
            .get(image as usize)
            .copied()
            .ok_or(VulkanError::UiFrameCapacity)
    }

    pub(super) fn destroy(&mut self, device: &Device) {
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
