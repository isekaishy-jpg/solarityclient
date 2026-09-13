//! Native WDL face-bank recording in the reserved horizon depth interval.

use ash::vk;

use crate::VulkanError;

use super::{RecordContext, WorldCommandBindings};

/// Draws visible tile fans after sky, before the ordinary world viewport is restored.
pub(super) fn record_low_detail(
    context: &RecordContext<'_>,
    viewport: vk::Viewport,
    world_scissor: vk::Rect2D,
    bindings: &mut WorldCommandBindings,
) -> Result<usize, VulkanError> {
    let Some(frame) = context.low_detail_frame else {
        return Ok(0);
    };
    let Some(scissor) = exterior_scissor(frame.exterior_window(), context.extent, world_scissor)
    else {
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
        context
            .device
            .cmd_set_scissor(context.command_buffer, 0, &[scissor]);
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
    // SAFETY: The original attachment-bounded scissor belongs to the same scope.
    // Ordinary world packets must not inherit the distant terrain portal clip.
    unsafe {
        context
            .device
            .cmd_set_scissor(context.command_buffer, 0, &[world_scissor]);
    }
    Ok(count)
}

/// A 533-unit WDL tile can overlap an exterior portal while most of its pixels
/// lie in sky-only gaps. Keep 790E20's admission window at the raster boundary.
/// Conversion uses the existing 6A38D0 backbuffer edge rounding; projection and
/// the ordinary world scissor remain independent of this coverage restriction.
fn exterior_scissor(
    window: crate::WorldScreenWindow,
    extent: (u32, u32),
    world: vk::Rect2D,
) -> Option<vk::Rect2D> {
    let normalized = |value| (f64::from(value) + 1.) * 0.5;
    let width = f64::from(extent.0 as f32);
    let height = f64::from(extent.1 as f32);
    let left = ((normalized(window.minimum_x()) * width + 0.5) as u32).max(world.offset.x as u32);
    let top =
        (((1. - normalized(window.maximum_y())) * height + 0.5) as u32).max(world.offset.y as u32);
    let right = ((normalized(window.maximum_x()) * width + 1.) as u32)
        .min(world.offset.x as u32 + world.extent.width);
    let bottom = (((1. - normalized(window.minimum_y())) * height + 1.) as u32)
        .min(world.offset.y as u32 + world.extent.height);
    (left < right && top < bottom).then(|| vk::Rect2D {
        offset: vk::Offset2D {
            x: left as i32,
            y: top as i32,
        },
        extent: vk::Extent2D {
            width: right - left,
            height: bottom - top,
        },
    })
}
