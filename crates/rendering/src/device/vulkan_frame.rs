//! One-shot staging and presentation of decoded client pixels.

#![allow(unsafe_code)]

use ash::{Device, vk};
use vk_mem::Alloc;

use crate::device::VulkanError;

/// Borrowed live Vulkan objects needed for one pixel presentation.
pub(super) struct FrameContext<'a> {
    pub(super) device: &'a Device,
    pub(super) allocator: &'a vk_mem::Allocator,
    pub(super) swapchain_loader: &'a ash::khr::swapchain::Device,
    pub(super) swapchain: vk::SwapchainKHR,
    pub(super) swapchain_images: &'a [vk::Image],
    pub(super) graphics_queue: vk::Queue,
    pub(super) present_queue: vk::Queue,
    pub(super) graphics_queue_family: u32,
    pub(super) frame_extent: (u32, u32),
    pub(super) source_extent: (u32, u32),
    pub(super) rgba8: &'a [u8],
}

/// Composes, transfers, presents, and synchronously retires one pixel frame.
pub(super) fn present_rgba8(context: FrameContext<'_>) -> Result<(), VulkanError> {
    let frame = compose_bgra_frame(context.frame_extent, context.source_extent, context.rgba8)?;
    let mut resources = FrameResources::create(context.device, context.allocator, &frame)?;
    let command_buffer = resources.allocate_command_buffer(context.graphics_queue_family)?;

    // SAFETY: The swapchain and synchronization objects are live and owned by
    // the respective renderer/frame owners for the entire call.
    let (image_index, _suboptimal) = unsafe {
        context.swapchain_loader.acquire_next_image(
            context.swapchain,
            u64::MAX,
            resources.image_available,
            vk::Fence::null(),
        )
    }
    .map_err(|source| swapchain_error("acquire swapchain image", source))?;
    let image = context
        .swapchain_images
        .get(image_index as usize)
        .copied()
        .ok_or_else(|| VulkanError::operation("index swapchain image", "index is out of range"))?;
    record_transfer(
        context.device,
        command_buffer,
        resources.staging_buffer,
        image,
        context.frame_extent,
    )?;
    submit_and_present(&context, &resources, command_buffer, image_index)?;
    Ok(())
}

/// Temporary resources whose drop path covers every partial initialization stage.
struct FrameResources<'a> {
    device: &'a Device,
    allocator: &'a vk_mem::Allocator,
    staging_buffer: vk::Buffer,
    staging_allocation: Option<vk_mem::Allocation>,
    command_pool: vk::CommandPool,
    image_available: vk::Semaphore,
    transfer_finished: vk::Semaphore,
    fence: vk::Fence,
}

impl<'a> FrameResources<'a> {
    /// Allocates an exact-size coherent upload buffer and binary synchronization.
    fn create(
        device: &'a Device,
        allocator: &'a vk_mem::Allocator,
        frame: &[u8],
    ) -> Result<Self, VulkanError> {
        let mut resources = Self {
            device,
            allocator,
            staging_buffer: vk::Buffer::null(),
            staging_allocation: None,
            command_pool: vk::CommandPool::null(),
            image_available: vk::Semaphore::null(),
            transfer_finished: vk::Semaphore::null(),
            fence: vk::Fence::null(),
        };
        let size = u64::try_from(frame.len())
            .map_err(|source| VulkanError::operation("convert staging size", source))?;
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let allocation_info = vk_mem::AllocationCreateInfo {
            flags: vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
            usage: vk_mem::MemoryUsage::AutoPreferHost,
            required_flags: vk::MemoryPropertyFlags::HOST_VISIBLE
                | vk::MemoryPropertyFlags::HOST_COHERENT,
            ..Default::default()
        };
        // SAFETY: VMA receives valid create infos and its parent device is live.
        let (buffer, allocation) =
            unsafe { allocator.create_buffer(&buffer_info, &allocation_info) }
                .map_err(|source| VulkanError::operation("create staging buffer", source))?;
        resources.staging_buffer = buffer;
        resources.staging_allocation = Some(allocation);
        let allocation = resources.staging_allocation.as_mut().ok_or_else(|| {
            VulkanError::operation("access staging allocation", "allocation is unavailable")
        })?;
        // SAFETY: The coherent host-visible allocation backs an exact-size
        // buffer, so the destination accepts precisely `frame.len()` bytes.
        unsafe {
            let destination = allocator
                .map_memory(allocation)
                .map_err(|source| VulkanError::operation("map staging buffer", source))?;
            std::ptr::copy_nonoverlapping(frame.as_ptr(), destination, frame.len());
            allocator.unmap_memory(allocation);
        }

        let semaphore_info = vk::SemaphoreCreateInfo::default();
        // SAFETY: Default binary semaphore creation has no borrowed state.
        resources.image_available = unsafe { device.create_semaphore(&semaphore_info, None) }
            .map_err(|source| VulkanError::operation("create acquire semaphore", source))?;
        // SAFETY: Same live device and self-contained create info.
        resources.transfer_finished = unsafe { device.create_semaphore(&semaphore_info, None) }
            .map_err(|source| VulkanError::operation("create transfer semaphore", source))?;
        // SAFETY: Default unsignaled fence creation has no borrowed state.
        resources.fence = unsafe { device.create_fence(&vk::FenceCreateInfo::default(), None) }
            .map_err(|source| VulkanError::operation("create frame fence", source))?;
        Ok(resources)
    }

    /// Creates a transient graphics-family command pool and one primary buffer.
    fn allocate_command_buffer(
        &mut self,
        queue_family: u32,
    ) -> Result<vk::CommandBuffer, VulkanError> {
        let pool_info = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::TRANSIENT)
            .queue_family_index(queue_family);
        // SAFETY: The family was enabled on this live logical device.
        self.command_pool = unsafe { self.device.create_command_pool(&pool_info, None) }
            .map_err(|source| VulkanError::operation("create command pool", source))?;
        let allocate_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(self.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        // SAFETY: The pool is live and owned until frame completion.
        let buffers = unsafe { self.device.allocate_command_buffers(&allocate_info) }
            .map_err(|source| VulkanError::operation("allocate command buffer", source))?;
        buffers.first().copied().ok_or_else(|| {
            VulkanError::operation(
                "allocate command buffer",
                "driver returned no command buffer",
            )
        })
    }

    /// Waits for the submission fence before temporary resources can be destroyed.
    fn wait(&self) -> Result<(), VulkanError> {
        // SAFETY: The fence is live and associated with this frame submission.
        unsafe { self.device.wait_for_fences(&[self.fence], true, u64::MAX) }
            .map_err(|source| VulkanError::operation("wait for bootstrap frame", source))
    }
}

impl Drop for FrameResources<'_> {
    /// Releases transient objects after the caller has retired the frame.
    fn drop(&mut self) {
        // SAFETY: Each non-null handle was created by this device and is owned
        // exactly once. The normal path waits for its fence before drop.
        unsafe {
            if self.command_pool != vk::CommandPool::null() {
                self.device.destroy_command_pool(self.command_pool, None);
            }
            if self.fence != vk::Fence::null() {
                self.device.destroy_fence(self.fence, None);
            }
            if self.transfer_finished != vk::Semaphore::null() {
                self.device.destroy_semaphore(self.transfer_finished, None);
            }
            if self.image_available != vk::Semaphore::null() {
                self.device.destroy_semaphore(self.image_available, None);
            }
            if let Some(mut allocation) = self.staging_allocation.take() {
                self.allocator
                    .destroy_buffer(self.staging_buffer, &mut allocation);
            }
        }
    }
}

/// Records layout transitions and the tightly packed full-frame buffer copy.
fn record_transfer(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    staging_buffer: vk::Buffer,
    image: vk::Image,
    extent: (u32, u32),
) -> Result<(), VulkanError> {
    let begin_info =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
    // SAFETY: The primary command buffer is newly allocated and not pending.
    unsafe { device.begin_command_buffer(command_buffer, &begin_info) }
        .map_err(|source| VulkanError::operation("begin transfer command buffer", source))?;
    let subresource_range = vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1);
    let to_transfer = vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::NONE)
        .src_access_mask(vk::AccessFlags2::NONE)
        .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
        .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .image(image)
        .subresource_range(subresource_range);
    let transfer_barriers = [to_transfer];
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&transfer_barriers);
    // SAFETY: Synchronization2 is enabled and the image belongs to the acquired
    // swapchain index, whose prior contents are intentionally discarded.
    unsafe { device.cmd_pipeline_barrier2(command_buffer, &dependency) };

    let image_layers = vk::ImageSubresourceLayers::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .mip_level(0)
        .base_array_layer(0)
        .layer_count(1);
    let copy_region = vk::BufferImageCopy::default()
        .buffer_offset(0)
        .buffer_row_length(0)
        .buffer_image_height(0)
        .image_subresource(image_layers)
        .image_offset(vk::Offset3D::default())
        .image_extent(vk::Extent3D {
            width: extent.0,
            height: extent.1,
            depth: 1,
        });
    // SAFETY: The staging buffer contains one tightly packed BGRA8 pixel for
    // every target pixel, and the acquired image is in transfer-destination layout.
    unsafe {
        device.cmd_copy_buffer_to_image(
            command_buffer,
            staging_buffer,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[copy_region],
        );
    }
    let to_present = vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
        .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::NONE)
        .dst_access_mask(vk::AccessFlags2::NONE)
        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
        .image(image)
        .subresource_range(subresource_range);
    let present_barriers = [to_present];
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&present_barriers);
    // SAFETY: The transfer write precedes the presentation layout transition
    // in the same command buffer.
    unsafe { device.cmd_pipeline_barrier2(command_buffer, &dependency) };
    // SAFETY: All recorded commands reference resources alive through fence retirement.
    unsafe { device.end_command_buffer(command_buffer) }
        .map_err(|source| VulkanError::operation("end transfer command buffer", source))
}

/// Submits the transfer and queues presentation with explicit binary semaphore flow.
fn submit_and_present(
    context: &FrameContext<'_>,
    resources: &FrameResources<'_>,
    command_buffer: vk::CommandBuffer,
    image_index: u32,
) -> Result<(), VulkanError> {
    let wait_info = vk::SemaphoreSubmitInfo::default()
        .semaphore(resources.image_available)
        .stage_mask(vk::PipelineStageFlags2::TRANSFER);
    let command_info = vk::CommandBufferSubmitInfo::default().command_buffer(command_buffer);
    let signal_info = vk::SemaphoreSubmitInfo::default()
        .semaphore(resources.transfer_finished)
        .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS);
    let wait_infos = [wait_info];
    let command_infos = [command_info];
    let signal_infos = [signal_info];
    let submit_info = vk::SubmitInfo2::default()
        .wait_semaphore_infos(&wait_infos)
        .command_buffer_infos(&command_infos)
        .signal_semaphore_infos(&signal_infos);
    // SAFETY: Queue submission references live synchronization objects and a
    // completed primary command buffer from the queue's family.
    unsafe {
        context
            .device
            .queue_submit2(context.graphics_queue, &[submit_info], resources.fence)
    }
    .map_err(|source| VulkanError::operation("submit bootstrap frame", source))?;

    let present_wait = [resources.transfer_finished];
    let swapchains = [context.swapchain];
    let image_indices = [image_index];
    let present_info = vk::PresentInfoKHR::default()
        .wait_semaphores(&present_wait)
        .swapchains(&swapchains)
        .image_indices(&image_indices);
    // SAFETY: Presentation waits on the transfer signal and uses the acquired
    // image index from this live swapchain.
    let present_result = unsafe {
        context
            .swapchain_loader
            .queue_present(context.present_queue, &present_info)
    }
    .map_err(|source| swapchain_error("present bootstrap frame", source));
    // Submission succeeded, so the fence must retire before any error path can
    // release its command pool, semaphores, or staging allocation.
    let wait_result = resources.wait();
    present_result?;
    wait_result
}

pub(super) fn swapchain_error(operation: &'static str, source: vk::Result) -> VulkanError {
    if source == vk::Result::ERROR_OUT_OF_DATE_KHR {
        VulkanError::SwapchainOutOfDate
    } else {
        VulkanError::operation(operation, source)
    }
}

/// Fits RGBA8 source pixels inside the frame and converts them to BGRA8.
fn compose_bgra_frame(
    frame_extent: (u32, u32),
    texture_extent: (u32, u32),
    rgba8: &[u8],
) -> Result<Vec<u8>, VulkanError> {
    let (frame_width, frame_height) = frame_extent;
    let (texture_width, texture_height) = texture_extent;
    if frame_width == 0 || frame_height == 0 || texture_width == 0 || texture_height == 0 {
        return Err(VulkanError::FrameSize);
    }
    let expected_source = usize::try_from(texture_width)
        .ok()
        .and_then(|width| {
            usize::try_from(texture_height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(VulkanError::FrameSize)?;
    if rgba8.len() != expected_source {
        return Err(VulkanError::FrameSize);
    }
    let frame_len = usize::try_from(frame_width)
        .ok()
        .and_then(|width| {
            usize::try_from(frame_height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(VulkanError::FrameSize)?;
    let mut frame = vec![0_u8; frame_len];
    for pixel in frame.as_chunks_mut::<4>().0 {
        pixel[3] = 0xFF;
    }

    let frame_aspect = u64::from(frame_width) * u64::from(texture_height);
    let texture_aspect = u64::from(texture_width) * u64::from(frame_height);
    let (draw_width, draw_height) = if texture_aspect > frame_aspect {
        let height =
            (u64::from(texture_height) * u64::from(frame_width) / u64::from(texture_width)) as u32;
        (frame_width, height.max(1))
    } else {
        let width =
            (u64::from(texture_width) * u64::from(frame_height) / u64::from(texture_height)) as u32;
        (width.max(1), frame_height)
    };
    let origin_x = (frame_width - draw_width) / 2;
    let origin_y = (frame_height - draw_height) / 2;
    for y in 0..draw_height {
        let source_y = u64::from(y) * u64::from(texture_height) / u64::from(draw_height);
        for x in 0..draw_width {
            let source_x = u64::from(x) * u64::from(texture_width) / u64::from(draw_width);
            let source_pixel = source_y * u64::from(texture_width) + source_x;
            let target_pixel =
                u64::from(origin_y + y) * u64::from(frame_width) + u64::from(origin_x + x);
            let source =
                usize::try_from(source_pixel * 4).map_err(|_source| VulkanError::FrameSize)?;
            let target =
                usize::try_from(target_pixel * 4).map_err(|_source| VulkanError::FrameSize)?;
            frame[target] = rgba8[source + 2];
            frame[target + 1] = rgba8[source + 1];
            frame[target + 2] = rgba8[source];
            frame[target + 3] = rgba8[source + 3];
        }
    }
    Ok(frame)
}
