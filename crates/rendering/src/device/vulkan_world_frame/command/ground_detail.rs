//! Native terrain-detail texture buckets using persistent, fence-pinned buffers.

use ash::vk;

use crate::VulkanError;

use super::{RecordContext, WorldCommandBindings};

/// Records admitted chunks with the shared terrain light/fog descriptor.
pub(super) fn record_ground_detail(
    context: &RecordContext<'_>,
    bindings: &mut WorldCommandBindings,
) -> Result<(), VulkanError> {
    let Some(frame) = context.ground_detail_frame else {
        return Ok(());
    };
    let (pipeline, layout) = context.detail_pipeline.raw();
    for draw in frame
        .draws()
        .iter()
        .filter(|draw| !draw.plan().indices().is_empty())
    {
        let mesh = context
            .ground_detail_registry
            .get(draw)
            .ok_or(VulkanError::WorldFrameCapacity)?;
        bindings.bind_pipeline(context, pipeline);
        bindings.bind_vertex(context, mesh.vertex_buffer());
        bindings.bind_index(context, mesh.index_buffer(), vk::IndexType::UINT16);
        let mut pushes = [0_u8; 16];
        // 7984A0 subtracts the camera before rotating the chunk translation.
        let origin = glam::Vec3::from_array(draw.plan().origin()) - frame.camera_position();
        for (value, target) in origin
            .to_array()
            .into_iter()
            .chain([frame.distance()])
            .zip(pushes.as_chunks_mut::<4>().0)
        {
            *target = value.to_le_bytes();
        }
        // SAFETY: The prepared pipeline owns this 16-byte ABI; every submitted
        // slot pins the immutable vertex/index bank until its fence completes.
        unsafe {
            context.device.cmd_push_constants(
                context.command_buffer,
                layout,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                0,
                &pushes,
            );
            for (batch, set) in draw.plan().batches().iter().zip(mesh.sets()) {
                context.device.cmd_bind_descriptor_sets(
                    context.command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    layout,
                    0,
                    &[context.frame_sets[0], *set],
                    &[],
                );
                let [first, count] = batch.index_range();
                context
                    .device
                    .cmd_draw_indexed(context.command_buffer, count, 1, first, 0, 0);
            }
        }
    }
    Ok(())
}
