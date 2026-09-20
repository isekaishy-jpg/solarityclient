//! Exclusive pass command pools share the owning world slot's GPU fence.

#![allow(unsafe_code)]

use crate::VulkanError;
use ash::{Device, vk};

/// Primary and three environment passes never record through the same pool.
#[derive(Default)]
pub(in super::super) struct RecordingPools {
    pools: [vk::CommandPool; 4],
    commands: [vk::CommandBuffer; 4],
}

impl RecordingPools {
    /// Creates the bounded pass storage after this slot's previous GPU use retires.
    pub(in super::super) fn ensure(
        &mut self,
        device: &Device,
        family: u32,
    ) -> Result<(), VulkanError> {
        for index in 0..self.pools.len() {
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
                        VulkanError::operation("create shadow recording pool", error)
                    })?;
            }
            let info = vk::CommandBufferAllocateInfo::default()
                .command_pool(self.pools[index])
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1);
            // SAFETY: No worker or GPU use exists during this exclusive slot operation.
            self.commands[index] = unsafe { device.allocate_command_buffers(&info) }
                .map_err(|error| {
                    VulkanError::operation("allocate shadow recording command", error)
                })?
                .first()
                .copied()
                .ok_or(VulkanError::WorldFrameCapacity)?;
        }
        Ok(())
    }

    pub(in super::super) fn commands(&self) -> [vk::CommandBuffer; 4] {
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
                .map_err(|error| VulkanError::operation("reset shadow recording pool", error))?;
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
