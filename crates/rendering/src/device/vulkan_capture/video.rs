//! A reusable readback slot with its own completion fence; no device-idle polling.

#![allow(unsafe_code)]

use std::time::Duration;

use ash::{Device, vk};

use super::{FrameReadback, VulkanError, scale};

/// Single reusable copy owner; an unsignaled marker forbids storage reuse and teardown.
pub(in crate::device) struct VideoReadback {
    frame: FrameReadback,
    source: (u32, u32),
    fence: vk::Fence,
    requested: Option<Duration>,
    submitted: bool,
    presented: bool,
    fence_failed: bool,
}

impl VideoReadback {
    /// Allocate fixed recording storage and an independent queue completion marker.
    pub(in crate::device) fn create(
        device: &Device,
        allocator: &vk_mem::Allocator,
        source: (u32, u32),
    ) -> Result<Self, VulkanError> {
        let output = scale::recording_extent(source);
        let frame = Self::frame(allocator, source, output)?;
        // SAFETY: Unsignaled fence belongs exclusively to this capture slot.
        let fence = match unsafe { device.create_fence(&vk::FenceCreateInfo::default(), None) } {
            Ok(fence) => fence,
            Err(error) => {
                frame.destroy(allocator);
                return Err(VulkanError::operation("create recording fence", error));
            }
        };
        Ok(Self {
            frame,
            source,
            fence,
            requested: None,
            submitted: false,
            presented: false,
            fence_failed: false,
        })
    }

    /// Couple readback allocation with an optional aspect-preserving scale image.
    fn frame(
        allocator: &vk_mem::Allocator,
        source: (u32, u32),
        output: (u32, u32),
    ) -> Result<FrameReadback, VulkanError> {
        let mut frame = FrameReadback::create(allocator, output)?;
        if source != output {
            match scale::CaptureScale::create(allocator, source, output) {
                Ok(scale) => frame.scale = Some(scale),
                Err(error) => {
                    frame.destroy(allocator);
                    return Err(error);
                }
            }
        }
        Ok(frame)
    }

    pub(in crate::device) fn extent(&self) -> (u32, u32) {
        self.frame.extent
    }

    /// Caller has retired the slot or waited idle during swapchain recreation.
    pub(in crate::device) fn resize(
        &mut self,
        allocator: &vk_mem::Allocator,
        source: (u32, u32),
    ) -> Result<(), VulkanError> {
        if self.source != source && !self.submitted {
            let replacement = Self::frame(allocator, source, self.extent())?;
            let old = std::mem::replace(&mut self.frame, replacement);
            old.destroy(allocator);
            self.source = source;
        }
        Ok(())
    }

    /// Reserve an idle slot; its timestamp remains owned until collection.
    pub(in crate::device) fn request(&mut self, timestamp: Duration) -> bool {
        if self.requested.is_some() {
            return false;
        }
        self.requested = Some(timestamp);
        true
    }

    /// Expose storage only while a requested copy has not entered the queue.
    pub(in crate::device) fn pending(&self) -> Option<&FrameReadback> {
        (self.requested.is_some() && !self.submitted).then_some(&self.frame)
    }

    /// Fence earlier graphics work even when presentation reports failure.
    pub(in crate::device) fn submitted(
        &mut self,
        device: &Device,
        queue: vk::Queue,
        presented: bool,
    ) -> Result<(), VulkanError> {
        if self.pending().is_none() {
            return Ok(());
        }
        // A rendering submission may already reference this buffer. If the
        // completion marker fails, only device-idle teardown may release it.
        self.submitted = true;
        self.fence_failed = true;
        // SAFETY: No submission currently owns this fence. An empty queue submit
        // fences all earlier graphics submissions, including the framebuffer copy.
        unsafe {
            device
                .reset_fences(&[self.fence])
                .map_err(|error| VulkanError::operation("reset recording fence", error))?;
            device
                .queue_submit2(queue, &[], self.fence)
                .map_err(|error| VulkanError::operation("submit recording fence", error))?;
        }
        self.fence_failed = false;
        self.presented = presented;
        Ok(())
    }

    /// Return completed BGRA storage without blocking the rendering thread.
    pub(in crate::device) fn poll(
        &mut self,
        device: &Device,
        allocator: &vk_mem::Allocator,
        output: &mut [u8],
    ) -> Result<Option<Duration>, VulkanError> {
        if !self.submitted || !self.complete(device)? {
            return Ok(None);
        }
        if output.len() != self.frame.byte_count {
            return Err(VulkanError::FrameSize);
        }
        let timestamp = self.requested.take();
        self.submitted = false;
        if !self.presented {
            return Ok(None);
        }
        allocator
            .invalidate_allocation(&self.frame.allocation, 0, self.frame.byte_count as u64)
            .map_err(|error| VulkanError::operation("invalidate recording readback", error))?;
        let source = allocator
            .get_allocation_info(&self.frame.allocation)
            .mapped_data
            .cast::<u8>();
        if source.is_null() {
            return Err(VulkanError::operation(
                "read recording pixels",
                "allocation is not mapped",
            ));
        }
        // SAFETY: Fence completion and invalidation precede this exact-size copy.
        // BGRA is retained; color conversion belongs to the encoder worker.
        output.copy_from_slice(unsafe { std::slice::from_raw_parts(source, output.len()) });
        Ok(timestamp)
    }

    /// Query completion without waiting; marker failures require device-idle teardown.
    pub(in crate::device) fn complete(&self, device: &Device) -> Result<bool, VulkanError> {
        if self.fence_failed {
            return Err(VulkanError::operation(
                "poll recording fence",
                "completion marker failed; device-idle teardown is required",
            ));
        }
        if !self.submitted {
            return Ok(true);
        }
        // SAFETY: Fence stays live until capture teardown and is never reset in flight.
        unsafe { device.get_fence_status(self.fence) }
            .map_err(|error| VulkanError::operation("poll recording fence", error))
    }

    /// Release storage only after the caller proves completion or device idle.
    pub(in crate::device) fn destroy(self, device: &Device, allocator: &vk_mem::Allocator) {
        self.frame.destroy(allocator);
        // SAFETY: Caller has proved capture completion or waited for device idle.
        unsafe { device.destroy_fence(self.fence, None) };
    }
}
