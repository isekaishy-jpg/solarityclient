//! Fixed sky mesh storage isolated by the enclosing world frame's fence.

#![allow(unsafe_code)]

use ash::vk;
use vk_mem::Alloc;

use crate::{VulkanError, WorldSkyDome};

const STRIDE: usize = 24;
const INDEX_OFFSET: usize = 122 * STRIDE;
const BUFFER_SIZE: usize = INDEX_OFFSET + 300 * 2;

/// One reusable, host-visible vertex and index allocation per frame slot.
pub(in crate::device) struct SkyFrameResources {
    buffer: vk::Buffer,
    allocation: Option<vk_mem::Allocation>,
}

impl SkyFrameResources {
    pub(in crate::device) const fn empty() -> Self {
        Self {
            buffer: vk::Buffer::null(),
            allocation: None,
        }
    }

    /// Allocates once and updates only after the owning slot's fence retires.
    pub(in crate::device) fn write(
        &mut self,
        allocator: &vk_mem::Allocator,
        dome: &WorldSkyDome,
    ) -> Result<(), VulkanError> {
        if self.allocation.is_none() {
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
            // SAFETY: VMA creates and binds the complete fixed-size host-visible bank.
            let (buffer, allocation) = unsafe { allocator.create_buffer(&info, &allocation_info) }
                .map_err(|source| VulkanError::operation("create sky mesh buffer", source))?;
            self.buffer = buffer;
            self.allocation = Some(allocation);
        }
        let allocation = self
            .allocation
            .as_mut()
            .ok_or(VulkanError::WorldFrameCapacity)?;
        // SAFETY: Slot retirement excludes all GPU readers of this allocation.
        let destination = unsafe { allocator.map_memory(allocation) }
            .map_err(|source| VulkanError::operation("map sky mesh", source))?;
        let mut bytes = [0_u8; BUFFER_SIZE];
        for (index, (position, color)) in dome.positions().iter().zip(dome.colors()).enumerate() {
            let start = index * STRIDE;
            for (component, value) in position.iter().enumerate() {
                bytes[start + component * 4..start + component * 4 + 4]
                    .copy_from_slice(&value.to_le_bytes());
            }
            // PCT attributes consume RGBA bytes; the original palette stores ARGB words.
            bytes[start + 12..start + 16].copy_from_slice(&[
                (color >> 16) as u8,
                (color >> 8) as u8,
                *color as u8,
                (color >> 24) as u8,
            ]);
        }
        for (index, value) in dome.indices().iter().enumerate() {
            bytes[INDEX_OFFSET + index * 2..INDEX_OFFSET + index * 2 + 2]
                .copy_from_slice(&value.to_le_bytes());
        }
        // SAFETY: Both fixed-size banks cover exactly BUFFER_SIZE non-overlapping bytes.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), destination, BUFFER_SIZE);
        }
        let result = allocator
            .flush_allocation(allocation, 0, BUFFER_SIZE as u64)
            .map_err(|source| VulkanError::operation("flush sky mesh", source));
        // SAFETY: Mapping succeeded; release it even if flushing failed.
        unsafe {
            allocator.unmap_memory(allocation);
        }
        result
    }

    pub(in crate::device) const fn vertex_buffer(&self) -> (vk::Buffer, vk::DeviceSize) {
        (self.buffer, 0)
    }

    pub(in crate::device) const fn index_buffer(&self) -> (vk::Buffer, vk::DeviceSize) {
        (self.buffer, INDEX_OFFSET as u64)
    }

    /// Retires uniquely owned storage after all submissions have finished.
    pub(in crate::device) fn destroy(&mut self, allocator: &vk_mem::Allocator) {
        if let Some(mut allocation) = self.allocation.take() {
            // SAFETY: Renderer teardown excludes pending GPU use of this slot.
            unsafe {
                allocator.destroy_buffer(self.buffer, &mut allocation);
            }
        }
        *self = Self::empty();
    }
}
