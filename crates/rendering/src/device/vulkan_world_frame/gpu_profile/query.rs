//! One lazily allocated query pool, reused only after its owner's normal fence.

#![allow(unsafe_code)]

use ash::{Device, vk};

use super::QUERY_COUNT;
use crate::device::VulkanError;

/// Distinguishes an unsampled slot from an unavailable submitted sample.
pub(in crate::device::vulkan_world_frame) enum GpuTimestampRead {
    /// This slot's preceding submission did not write timestamps.
    Idle,
    /// The nonblocking driver read did not return the retired query range.
    Unavailable,
    /// All completion timestamps belong to the preceding sampled submission.
    Ready([u64; QUERY_COUNT]),
}

/// Pending means a successfully submitted command buffer wrote every timestamp.
#[derive(Default)]
pub(in crate::device::vulkan_world_frame) struct GpuTimestampSlot {
    pool: vk::QueryPool,
    pending: bool,
}

impl GpuTimestampSlot {
    /// Allocates once on a sampled frame; callers have already retired this slot.
    pub(in crate::device::vulkan_world_frame) fn ensure(
        &mut self,
        device: &Device,
    ) -> Result<vk::QueryPool, VulkanError> {
        if self.pool == vk::QueryPool::null() {
            let info = vk::QueryPoolCreateInfo::default()
                .query_type(vk::QueryType::TIMESTAMP)
                .query_count(QUERY_COUNT as u32);
            // SAFETY: The logical device is live; the pool uses no optional feature.
            self.pool = unsafe { device.create_query_pool(&info, None) }
                .map_err(|source| VulkanError::operation("create GPU timestamp pool", source))?;
        }
        Ok(self.pool)
    }

    /// Reads only submitted queries after the existing fence; never uses WAIT.
    pub(in crate::device::vulkan_world_frame) fn collect(
        &mut self,
        device: &Device,
    ) -> Result<GpuTimestampRead, VulkanError> {
        if !std::mem::take(&mut self.pending) {
            return Ok(GpuTimestampRead::Idle);
        }
        let mut values = [0_u64; QUERY_COUNT];
        // SAFETY: This pool's submitted work has completed at the slot fence. All
        // queries were reset and written by that submission; storage is 64-bit.
        match unsafe {
            device.get_query_pool_results(self.pool, 0, &mut values, vk::QueryResultFlags::TYPE_64)
        } {
            Ok(()) => Ok(GpuTimestampRead::Ready(values)),
            Err(vk::Result::NOT_READY) => Ok(GpuTimestampRead::Unavailable),
            Err(source) => Err(VulkanError::operation("read GPU timestamps", source)),
        }
    }

    /// Called immediately after successful queue submission, before presentation.
    pub(in crate::device::vulkan_world_frame) fn submitted(&mut self, sampled: bool) {
        self.pending = sampled;
    }

    /// Destroys the pool under the same retired-device contract as its frame slot.
    pub(in crate::device::vulkan_world_frame) fn destroy(&mut self, device: &Device) {
        if self.pool != vk::QueryPool::null() {
            // SAFETY: The caller has retired every submission using this slot.
            unsafe { device.destroy_query_pool(self.pool, None) };
            self.pool = vk::QueryPool::null();
        }
        self.pending = false;
    }
}
