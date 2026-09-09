//! Native WDL face-bank recording in the reserved horizon depth interval.

use ash::vk;

use crate::VulkanError;

use super::{RecordContext, WorldCommandBindings};

/// Draws visible tile fans after sky, before the ordinary world viewport is restored.
pub(super) fn record_low_detail(
    context: &RecordContext<'_>,
    viewport: vk::Viewport,
    bindings: &mut WorldCommandBindings,
) -> Result<usize, VulkanError> {
    let Some(frame) = context.low_detail_frame else {
        return Ok(0);
    };
    let map = context
        .low_detail_map
        .ok_or(VulkanError::WorldFrameCapacity)?;
    // 795F80 passes ADEEE8/ADEEEC to GX viewport setup.
    let viewport = vk::Viewport {
        min_depth: 0.998_046_9,
        max_depth: 0.999_023_44,
        ..viewport
    };
    let pushes = frame.push_bytes();
    // SAFETY: The active scope uses the same extent and dynamic viewport ABI.
    unsafe {
        context
            .device
            .cmd_set_viewport(context.command_buffer, 0, &[viewport]);
    }
    let mut count = 0;
    for (index, tile) in frame.map().tiles().iter().enumerate() {
        if !frame
            .is_visible(tile)
            .map_err(|source| VulkanError::operation("select horizon tile", source))?
        {
            continue;
        }
        bindings.bind_vertex(context, map.vertex_buffer(index));
        bindings.bind_index(context, map.index_buffer(index), vk::IndexType::UINT16);
        let unculled = tile.unculled_index_count();
        for (bank, first, indices) in [(0, 0, unculled), (1, unculled, 3072 - unculled)] {
            if indices == 0 {
                continue;
            }
            let (pipeline, layout) = context.low_detail_pipelines.raw(bank);
            bindings.bind_pipeline(context, pipeline);
            // SAFETY: Both pipelines share the 96-byte push ABI. Validated native
            // tile banks bound the indices, and the slot pins their immutable map.
            unsafe {
                context.device.cmd_push_constants(
                    context.command_buffer,
                    layout,
                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                    0,
                    &pushes,
                );
                context
                    .device
                    .cmd_draw_indexed(context.command_buffer, indices, 1, first, 0, 0);
            }
            count += 1;
        }
    }
    Ok(count)
}
