//! Exclusive pass command pools share the owning world slot's GPU fence.

#![allow(unsafe_code)]

use crate::VulkanError;
use ash::{Device, vk};

/// Each independent recording range owns an exclusive pool, retired by its world slot.
pub(in super::super) struct RecordingPools<const N: usize = 4, const SECONDARY: bool = false> {
    pools: [vk::CommandPool; N],
    commands: [vk::CommandBuffer; N],
}

impl<const N: usize, const SECONDARY: bool> Default for RecordingPools<N, SECONDARY> {
    fn default() -> Self {
        Self {
            pools: [vk::CommandPool::null(); N],
            commands: [vk::CommandBuffer::null(); N],
        }
    }
}
impl<const N: usize, const SECONDARY: bool> RecordingPools<N, SECONDARY> {
    /// Creates the bounded pass storage after this slot's previous GPU use retires.
    pub(in super::super) fn ensure(
        &mut self,
        device: &Device,
        family: u32,
    ) -> Result<(), VulkanError> {
        self.ensure_count(device, family, N)
    }

    /// Creates only pools reachable by the configured parallel phase, retaining them across frames.
    pub(in super::super) fn ensure_count(
        &mut self,
        device: &Device,
        family: u32,
        count: usize,
    ) -> Result<(), VulkanError> {
        if count > N {
            return Err(VulkanError::WorldFrameCapacity);
        }
        for index in 0..count {
            if self.commands[index] != vk::CommandBuffer::null() {
                continue;
            }
            let info = vk::CommandPoolCreateInfo::default()
                .flags(vk::CommandPoolCreateFlags::TRANSIENT)
                .queue_family_index(family);
            if self.pools[index] == vk::CommandPool::null() {
                // SAFETY: This graphics family belongs to the live device; this slot owns the pool.
                self.pools[index] =
                    unsafe { device.create_command_pool(&info, None) }.map_err(|error| {
                        VulkanError::operation("create world recording pool", error)
                    })?;
            }
            let info = vk::CommandBufferAllocateInfo::default()
                .command_pool(self.pools[index])
                .level(if SECONDARY {
                    vk::CommandBufferLevel::SECONDARY
                } else {
                    vk::CommandBufferLevel::PRIMARY
                })
                .command_buffer_count(1);
            // SAFETY: No worker or GPU use exists during this exclusive slot operation.
            self.commands[index] = unsafe { device.allocate_command_buffers(&info) }
                .map_err(|error| VulkanError::operation("allocate world recording command", error))?
                .first()
                .copied()
                .ok_or(VulkanError::WorldFrameCapacity)?;
        }
        Ok(())
    }

    pub(in super::super) fn commands(&self) -> [vk::CommandBuffer; N] {
        self.commands
    }

    /// Main calls this only after the slot fence and every recording job have completed.
    pub(in super::super) fn reset(&mut self, device: &Device) -> Result<(), VulkanError> {
        for pool in self.pools {
            if pool == vk::CommandPool::null() {
                continue;
            }
            // SAFETY: No command in the owning slot is recording or pending on the GPU.
            unsafe { device.reset_command_pool(pool, vk::CommandPoolResetFlags::empty()) }
                .map_err(|error| VulkanError::operation("reset world recording pool", error))?;
        }
        Ok(())
    }

    /// Renderer teardown has joined CPU work and retired the owning slot's GPU work.
    pub(in super::super) fn destroy(&mut self, device: &Device) {
        for pool in &mut self.pools {
            if *pool != vk::CommandPool::null() {
                // SAFETY: The renderer's teardown barrier covers every command allocated here.
                unsafe { device.destroy_command_pool(*pool, None) };
                *pool = vk::CommandPool::null();
            }
        }
        self.commands.fill(vk::CommandBuffer::null());
    }
}
