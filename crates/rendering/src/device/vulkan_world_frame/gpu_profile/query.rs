//! One lazily allocated query pool, reused only after its owner's normal fence.

#![allow(unsafe_code)]

use ash::{Device, vk};

use super::QUERY_COUNT;
use crate::device::VulkanError;

/// Distinguishes an unsampled slot from an unavailable submitted sample.
pub(in crate::device) enum GpuTimestampRead<const COUNT: usize = QUERY_COUNT> {
    /// This slot's preceding submission did not write timestamps.
    Idle,
    /// The nonblocking driver read did not return the retired query range.
    Unavailable { generation: u64 },
    /// All completion timestamps belong to the preceding sampled submission.
    Ready {
        generation: u64,
        timestamps: [u64; COUNT],
    },
}

/// Pending means a successfully submitted command buffer wrote every timestamp.
#[derive(Default)]
pub(in crate::device) struct GpuTimestampSlot<const COUNT: usize = QUERY_COUNT> {
    pool: vk::QueryPool,
    pending: u64,
}

impl<const COUNT: usize> GpuTimestampSlot<COUNT> {
    /// Allocates once on a sampled frame; callers have already retired this slot.
    pub(in crate::device) fn ensure(
        &mut self,
        device: &Device,
    ) -> Result<vk::QueryPool, VulkanError> {
        if self.pool == vk::QueryPool::null() {
            let info = vk::QueryPoolCreateInfo::default()
                .query_type(vk::QueryType::TIMESTAMP)
                .query_count(COUNT as u32);
            // SAFETY: The logical device is live; the pool uses no optional feature.
            self.pool = unsafe { device.create_query_pool(&info, None) }
                .map_err(|source| VulkanError::operation("create GPU timestamp pool", source))?;
        }
        Ok(self.pool)
    }

    /// Reads only submitted queries after the existing fence; never uses WAIT.
    pub(in crate::device) fn collect(
        &mut self,
        device: &Device,
    ) -> Result<GpuTimestampRead<COUNT>, VulkanError> {
        let generation = std::mem::take(&mut self.pending);
        if generation == 0 {
            return Ok(GpuTimestampRead::Idle);
        }
        let mut values = [0_u64; COUNT];
        // SAFETY: This pool's submitted work has completed at the slot fence. All
        // queries were reset and written by that submission; storage is 64-bit.
        match unsafe {
            device.get_query_pool_results(self.pool, 0, &mut values, vk::QueryResultFlags::TYPE_64)
        } {
            Ok(()) => Ok(GpuTimestampRead::Ready {
                generation,
                timestamps: values,
            }),
            Err(vk::Result::NOT_READY) => Ok(GpuTimestampRead::Unavailable { generation }),
            Err(source) => Err(VulkanError::operation("read GPU timestamps", source)),
        }
    }

    /// Called immediately after successful queue submission, before presentation.
    pub(in crate::device) fn submitted(&mut self, sampled: bool) {
        self.pending = if sampled {
            solarity_profiling::generation()
        } else {
            0
        };
    }

    /// Destroys the pool under the same retired-device contract as its frame slot.
    pub(in crate::device) fn destroy(&mut self, device: &Device) {
        if self.pool != vk::QueryPool::null() {
            // SAFETY: The caller has retired every submission using this slot.
            unsafe { device.destroy_query_pool(self.pool, None) };
            self.pool = vk::QueryPool::null();
        }
        self.pending = 0;
    }
}
