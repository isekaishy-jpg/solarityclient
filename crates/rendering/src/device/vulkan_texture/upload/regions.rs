//! Queue-ordered updates of newly admitted coverage in a resident image.

use super::{
    DeferredTextureTransfer, GpuSampledImage, TextureTransfer, TextureUploadContext, VulkanError,
};
use ash::vk;

/// Copies only requested RGBA rectangles. The graphics queue orders earlier
/// samples, this transfer, and subsequent samples without a host-side wait.
pub(in crate::device) fn update_rgba8_regions(
    context: TextureUploadContext<'_>,
    image: &GpuSampledImage,
    extent: (u32, u32),
    rgba8: &[u8],
    rectangles: &[[u32; 4]],
) -> Result<DeferredTextureTransfer, VulkanError> {
    let invalid = || {
        VulkanError::operation(
            "validate coverage update",
            "invalid RGBA rectangle or image byte count",
        )
    };
    let expected = u64::from(extent.0)
        .checked_mul(u64::from(extent.1))
        .and_then(|value| value.checked_mul(4));
    if expected != Some(rgba8.len() as u64) || rectangles.is_empty() {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    let mut regions = Vec::with_capacity(rectangles.len());
    for &[x, y, width, height] in rectangles {
        if width == 0
            || height == 0
            || x.checked_add(width).is_none_or(|end| end > extent.0)
            || y.checked_add(height).is_none_or(|end| end > extent.1)
        {
            return Err(invalid());
        }
        let offset = bytes.len() as u64;
        for row in y..y + height {
            let begin = ((u64::from(row) * u64::from(extent.0) + u64::from(x)) * 4) as usize;
            bytes.extend_from_slice(&rgba8[begin..begin + width as usize * 4]);
        }
        regions.push(
            vk::BufferImageCopy::default()
                .buffer_offset(offset)
                .image_subresource(
                    vk::ImageSubresourceLayers::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .layer_count(1),
                )
                .image_offset(vk::Offset3D {
                    x: i32::try_from(x).map_err(|_| invalid())?,
                    y: i32::try_from(y).map_err(|_| invalid())?,
                    z: 0,
                })
                .image_extent(vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                }),
        );
    }
    let transfer = TextureTransfer::create(context, &bytes)?;
    let command = transfer.command_buffer()?;
    let begin =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
    let range = vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1);
    let before = [vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
        .src_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
        .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
        .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .image(image.image)
        .subresource_range(range)];
    let after = [vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
        .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
        .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .image(image.image)
        .subresource_range(range)];
    // SAFETY: New command buffer, validated staging spans, and a resident RGBA
    // image. Same-queue barriers order its previous and future fragment uses.
    unsafe {
        context
            .device
            .begin_command_buffer(command, &begin)
            .map_err(|error| VulkanError::operation("begin coverage update", error))?;
        context.device.cmd_pipeline_barrier2(
            command,
            &vk::DependencyInfo::default().image_memory_barriers(&before),
        );
        context.device.cmd_copy_buffer_to_image(
            command,
            transfer.staging_buffer,
            image.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &regions,
        );
        context.device.cmd_pipeline_barrier2(
            command,
            &vk::DependencyInfo::default().image_memory_barriers(&after),
        );
        context
            .device
            .end_command_buffer(command)
            .map_err(|error| VulkanError::operation("end coverage update", error))?;
    }
    transfer.submit(command)?;
    Ok(transfer.defer())
}
