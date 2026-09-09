//! One immutable WDL upload shared by every fenced world-frame slot.

#![allow(unsafe_code)]

use std::sync::Arc;

use ash::vk;
use vk_mem::Alloc;

use crate::{TerrainLowDetailMap, VulkanError};

const VERTEX_BYTES: usize = 545 * 12;
const TILE_BYTES: usize = VERTEX_BYTES + 3072 * 2;

/// The map identity doubles as a retirement token held by every submitted slot.
pub(in crate::device) struct LowDetailGpuMap {
    map: Arc<TerrainLowDetailMap>,
    buffer: vk::Buffer,
    allocation: Option<vk_mem::Allocation>,
}

impl LowDetailGpuMap {
    /// Uploads the fixed native vertex/index banks before any GPU reader exists.
    fn create(
        allocator: &vk_mem::Allocator,
        map: &Arc<TerrainLowDetailMap>,
    ) -> Result<Self, VulkanError> {
        let mut result = Self {
            map: Arc::clone(map),
            buffer: vk::Buffer::null(),
            allocation: None,
        };
        let size = map.tiles().len() * TILE_BYTES;
        if size == 0 {
            return Ok(result);
        }
        let info = vk::BufferCreateInfo::default()
            .size(size as u64)
            .usage(vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::INDEX_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let allocation_info = vk_mem::AllocationCreateInfo {
            flags: vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
            usage: vk_mem::MemoryUsage::AutoPreferDevice,
            required_flags: vk::MemoryPropertyFlags::HOST_VISIBLE,
            ..Default::default()
        };
        // SAFETY: VMA owns creation and binding of this complete immutable buffer.
        let (buffer, allocation) = unsafe { allocator.create_buffer(&info, &allocation_info) }
            .map_err(|source| VulkanError::operation("create horizon map buffer", source))?;
        result.buffer = buffer;
        result.allocation = Some(allocation);
        if let Err(error) = result.write(allocator, size) {
            result.destroy(allocator);
            return Err(error);
        }
        Ok(result)
    }

    /// Serializes native scalar arrays directly into disjoint, aligned tile banks.
    fn write(&mut self, allocator: &vk_mem::Allocator, size: usize) -> Result<(), VulkanError> {
        let allocation = self
            .allocation
            .as_mut()
            .ok_or(VulkanError::WorldFrameCapacity)?;
        // SAFETY: This newly allocated map has never been submitted.
        let destination = unsafe { allocator.map_memory(allocation) }
            .map_err(|source| VulkanError::operation("map horizon buffer", source))?;
        for (index, tile) in self.map.tiles().iter().enumerate() {
            // SAFETY: Each tile has 545 packed float triples and 3072 u16 indices.
            // TILE_BYTES bounds both non-overlapping copies inside the allocation.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    tile.positions().as_ptr().cast::<u8>(),
                    destination.add(index * TILE_BYTES),
                    VERTEX_BYTES,
                );
                std::ptr::copy_nonoverlapping(
                    tile.indices().as_ptr().cast::<u8>(),
                    destination.add(index * TILE_BYTES + VERTEX_BYTES),
                    3072 * 2,
                );
            }
        }
        let result = allocator
            .flush_allocation(allocation, 0, size as u64)
            .map_err(|source| VulkanError::operation("flush horizon map", source));
        // SAFETY: Mapping succeeded and must be released even when flushing fails.
        unsafe {
            allocator.unmap_memory(allocation);
        }
        result
    }

    pub(in crate::device) fn vertex_buffer(&self, tile: usize) -> (vk::Buffer, vk::DeviceSize) {
        (self.buffer, (tile * TILE_BYTES) as u64)
    }

    pub(in crate::device) fn index_buffer(&self, tile: usize) -> (vk::Buffer, vk::DeviceSize) {
        (self.buffer, (tile * TILE_BYTES + VERTEX_BYTES) as u64)
    }

    /// Called only once no submitted slot retains the map, or at device-idle teardown.
    fn destroy(&mut self, allocator: &vk_mem::Allocator) {
        if let Some(mut allocation) = self.allocation.take() {
            // SAFETY: Registry retirement or renderer teardown excludes GPU readers.
            unsafe {
                allocator.destroy_buffer(self.buffer, &mut allocation);
            }
        }
        self.buffer = vk::Buffer::null();
    }
}

/// Shares map uploads across frame slots and retires abandoned map generations.
#[derive(Default)]
pub(in crate::device) struct LowDetailRegistry {
    maps: Vec<LowDetailGpuMap>,
}

impl LowDetailRegistry {
    /// Retires unreferenced maps, then uploads a newly encountered map once.
    pub(in crate::device) fn ensure(
        &mut self,
        allocator: &vk_mem::Allocator,
        map: Option<&Arc<TerrainLowDetailMap>>,
    ) -> Result<(), VulkanError> {
        self.maps.retain_mut(|entry| {
            if Arc::strong_count(&entry.map) != 1 {
                return true;
            }
            entry.destroy(allocator);
            false
        });
        if let Some(map) = map
            && self.get(map).is_none()
        {
            self.maps.push(LowDetailGpuMap::create(allocator, map)?);
        }
        Ok(())
    }

    pub(in crate::device) fn get(
        &self,
        map: &Arc<TerrainLowDetailMap>,
    ) -> Option<&LowDetailGpuMap> {
        self.maps.iter().find(|entry| Arc::ptr_eq(&entry.map, map))
    }

    /// Renderer teardown waits for all slots before releasing shared buffers.
    pub(in crate::device) fn destroy(&mut self, allocator: &vk_mem::Allocator) {
        for map in &mut self.maps {
            map.destroy(allocator);
        }
        self.maps.clear();
    }
}
