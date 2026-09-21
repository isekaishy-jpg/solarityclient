//! Primary submission scope and independent front/tail secondary recording.

#![allow(unsafe_code)]

use super::super::recording::WorldFrameExecution;
use super::super::recording::scene::{self, ScenePools, SceneRecording};
use super::draws::{record_liquid_queue, record_terrain, record_world_model};
use super::ground_detail;
use super::low_detail::record_low_detail;
use super::order::{LiquidQueue, record_m2_scene_elements, record_sky_models};
use super::sky::{record_celestials, record_clouds, record_sky, record_underwater};
use super::{RecordContext, WorldCommandBindings};
use crate::VulkanError;
use crate::device::vulkan_ui_frame::record_loaded_overlay;
use ash::vk;
pub(in crate::device::vulkan_world_frame) fn record(
    context: &RecordContext<'_>,
    shadows_recorded: bool,
    scene_recording: &mut SceneRecording,
    scene_pools: &ScenePools,
    execution: &mut impl WorldFrameExecution,
) -> Result<(usize, super::super::fog::SubmissionFog), VulkanError> {
    // Bound raw draws before instancing; coalescing can only reduce captured commands.
    let detail_count = context.ground_detail_frame.map_or(0, |frame| {
        frame
            .draws()
            .iter()
            .map(|draw| draw.plan().batches().len())
            .sum::<usize>()
    });
    let command_bound = [
        context.terrain_draws.len(),
        context.world_model_draws.len(),
        context.m2_draws.len(),
        context.particle_draws.len(),
        context.ribbon_draws.len(),
        context.liquid_draws.len(),
        detail_count,
        6,
    ]
    .into_iter()
    .try_fold(0usize, |sum, count| {
        sum.checked_add(count)
            .ok_or(VulkanError::WorldFrameCapacity)
    })?;
    scene_recording.capture(execution.executor(), command_bound)?;
    let begin =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
    // SAFETY: Slot pool was reset and this primary buffer is not pending.
    unsafe {
        context
            .device
            .begin_command_buffer(context.command_buffer, &begin)
    }
    .map_err(|source| VulkanError::operation("begin world command buffer", source))?;
    if let Some(pool) = context.gpu_queries.filter(|_| !shadows_recorded) {
        // SAFETY: The owning slot has retired. Reset is outside rendering and
        // precedes every timestamp write in this submitted command buffer.
        unsafe {
            context.device.cmd_reset_query_pool(
                context.command_buffer,
                pool,
                0,
                super::super::gpu_profile::QUERY_COUNT as u32,
            );
            context.device.cmd_write_timestamp(
                context.command_buffer,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                pool,
                0,
            );
        }
    }
    context
        .glare_slot
        .reset(context.device, context.command_buffer);
    timestamp(context, 1);
    if !context.liquid_draws.is_empty() {
        context
            .liquid_resources
            .record_uploads(context.device, context.command_buffer);
    }
    if context.cloud_frame.is_some() {
        context
            .cloud_resources
            .record_upload(context.device, context.command_buffer);
    }
    transition_attachments(context);
    let color = vk::RenderingAttachmentInfo::default()
        .image_view(context.image_view)
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(vk::ClearValue {
            color: vk::ClearColorValue {
                float32: context.background_color.to_array(),
            },
        });
    let depth = vk::RenderingAttachmentInfo::default()
        .image_view(context.depth_view)
        .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::DONT_CARE)
        .clear_value(vk::ClearValue {
            depth_stencil: vk::ClearDepthStencilValue {
                depth: 1.0,
                stencil: 0,
            },
        });
    let colors = [color];
    let render_area = vk::Rect2D {
        offset: vk::Offset2D::default(),
        extent: vk::Extent2D {
            width: context.extent.0,
            height: context.extent.1,
        },
    };
    let rendering = vk::RenderingInfo::default()
        .flags(vk::RenderingFlags::CONTENTS_SECONDARY_COMMAND_BUFFERS)
        .render_area(render_area)
        .layer_count(1)
        .color_attachments(&colors)
        .depth_attachment(&depth)
        .stencil_attachment(&depth);
    // SAFETY: Formats/layouts agree with every world pipeline.
    unsafe {
        context
            .device
            .cmd_begin_rendering(context.command_buffer, &rendering)
    };
    let window = context.screen_window;
    let width = context.extent.0 as f32;
    let height = context.extent.1 as f32;
    let left = (window.minimum_x() + 1.0) * 0.5 * width;
    let right = (window.maximum_x() + 1.0) * 0.5 * width;
    let bottom = (window.minimum_y() + 1.0) * 0.5 * height;
    let top = (window.maximum_y() + 1.0) * 0.5 * height;
    let viewport = vk::Viewport {
        x: left,
        y: height - bottom,
        width: right - left,
        height: -(top - bottom),
        min_depth: 0.0,
        max_depth: 1.0,
    };
    let scissor = vk::Rect2D {
        offset: vk::Offset2D {
            x: left.floor() as i32,
            y: (height - top).floor() as i32,
        },
        extent: vk::Extent2D {
            width: (right.ceil() - left.floor()) as u32,
            height: (top.ceil() - bottom.floor()) as u32,
        },
    };
    let primary_context = context;
    let commands = scene_pools.commands();
    let front_context = RecordContext {
        command_buffer: commands[0],
        ..*context
    };
    scene::begin_inline(
        context.device,
        commands[0],
        context.color_format,
        context.depth_format,
        viewport,
        scissor,
    )?;
    let context = &front_context;
    let mut bindings = WorldCommandBindings::with_fog(context.submission_fog);
    if let Some(window) = context
        .sky_window
        .and_then(|window| window.clipped(context.screen_window))
    {
        let [left, top, right, bottom] = window.pixel_bounds([context.extent.0, context.extent.1]);
        let sky_scissor = vk::Rect2D {
            offset: vk::Offset2D {
                x: left as i32,
                y: top as i32,
            },
            extent: vk::Extent2D {
                width: right - left,
                height: bottom - top,
            },
        };
        // SAFETY: Every sky pipeline declares a dynamic scissor. The clipped
        // rectangle is bounded by the attachment; projection stays unchanged.
        unsafe {
            context
                .device
                .cmd_set_scissor(context.command_buffer, 0, &[sky_scissor]);
        }
        record_sky_models(context, false, &mut bindings)?;
        record_celestials(context, &mut bindings);
        record_sky(context, &mut bindings);
        record_clouds(context, &mut bindings);
        record_sky_models(context, true, &mut bindings)?;
        // SAFETY: WDL and subsequent world queues share the original scissor.
        unsafe {
            context
                .device
                .cmd_set_scissor(context.command_buffer, 0, &[scissor]);
        }
    }
    let low_detail_draw_count = record_low_detail(context, viewport, scissor, &mut bindings)?;
    timestamp(context, 2);
    // 79A870 restores the ordinary world interval after horizon/sky work.
    let world_viewport = vk::Viewport {
        max_depth: context.depth_maximum,
        ..viewport
    };
    scene::end(context.device, context.command_buffer)?;
    let mut bindings = WorldCommandBindings::capturing(bindings.fog, scene_recording);
    let _capture_profile = solarity_profiling::profile!("rendering.scene.capture");
    for draw in context.terrain_draws.iter().copied() {
        record_terrain(context, draw, &mut bindings)?;
    }
    bindings.timestamp(context, 3)?;
    for (index, draw) in context.world_model_draws.iter().copied().enumerate() {
        record_world_model(context, index, draw, &mut bindings)?;
    }
    bindings.timestamp(context, 4)?;
    // 4F9154 dispatches 7984A0 before the ordinary liquid/M2 scene queues.
    ground_detail::record_ground_detail(context, &mut bindings)?;
    bindings.timestamp(context, 5)?;
    record_liquid_queue(context, LiquidQueue::Opaque, &mut bindings)?;
    bindings.timestamp(context, 6)?;
    record_m2_scene_elements(context, &mut bindings)?;
    let submission_fog = bindings.fog;
    drop(_capture_profile);
    let context = primary_context;
    let pending = scene_recording.begin(
        execution.executor(),
        context,
        scene_pools,
        world_viewport,
        scissor,
    )?;
    // This independent compositor tail overlaps the central recording jobs.
    let tail_context = RecordContext {
        command_buffer: commands[1],
        ..*context
    };
    scene::begin_inline(
        context.device,
        commands[1],
        context.color_format,
        context.depth_format,
        world_viewport,
        scissor,
    )?;
    let mut tail_bindings = WorldCommandBindings::with_fog(submission_fog);
    record_underwater(&tail_context, &mut tail_bindings);
    context
        .glare
        .record(context.glare_slot, context.device, commands[1], viewport);
    scene::end(context.device, commands[1])?;
    record_post(context)?;
    let (recorded, count) = pending.finish(execution)?;
    // SAFETY: Every secondary finished recording and inherits these exact attachment
    // formats. Their exclusive pools and every resource remain pinned by this slot's fence.
    unsafe {
        context
            .device
            .cmd_execute_commands(context.command_buffer, &commands[..1]);
        if count != 0 {
            context
                .device
                .cmd_execute_commands(context.command_buffer, &recorded[..count]);
        }
        context
            .device
            .cmd_execute_commands(context.command_buffer, &commands[1..2]);
    }
    // SAFETY: The single matching world rendering scope is active.
    unsafe { context.device.cmd_end_rendering(context.command_buffer) };
    timestamp(context, 7);
    // SAFETY: Every bound resource outlives slot fence retirement.
    unsafe { context.device.end_command_buffer(context.command_buffer) }
        .map_err(|source| VulkanError::operation("end world command buffer", source))?;
    Ok((low_detail_draw_count, submission_fog))
}

/// Postprocessing and UI commands overlap scene workers; submission preserves world-before-UI.
fn record_post(context: &RecordContext<'_>) -> Result<(), VulkanError> {
    let context = &RecordContext {
        command_buffer: context.post_command_buffer,
        ..*context
    };
    let begin =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
    let _profile = solarity_profiling::profile!("rendering.scene.compositor");
    // SAFETY: Main alone records both primaries serially through their retired slot pool.
    unsafe {
        context
            .device
            .begin_command_buffer(context.command_buffer, &begin)
    }
    .map_err(|error| VulkanError::operation("begin world compositor", error))?;
    if let Some((glow, settings)) = context.glow {
        glow.record(
            context.device,
            context.command_buffer,
            context.image,
            context.image_view,
            context.image_index,
            context.screen_window,
            settings,
        )?;
    }
    timestamp(context, 8);
    if let Some(ui) = context.ui {
        transition_to_ui_overlay(context);
        record_loaded_overlay(ui)?;
    }
    timestamp(context, 9);
    transition_to_present(context);
    timestamp(context, 10);
    // SAFETY: Every postprocess/rendering scope is closed; the slot fence pins resources.
    unsafe { context.device.end_command_buffer(context.command_buffer) }
        .map_err(|error| VulkanError::operation("end world compositor", error))
}

/// Completion-stage boundaries expose elapsed GPU intervals without new barriers.
/// Overlapping pipeline work prevents interpreting these as isolated shader costs.
pub(super) fn timestamp(context: &RecordContext<'_>, index: u32) {
    if let Some(pool) = context.gpu_queries {
        // SAFETY: This command buffer owns the reset query range; each boundary
        // writes a distinct index and the selected graphics queue supports it.
        unsafe {
            context.device.cmd_write_timestamp(
                context.command_buffer,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                pool,
                index,
            )
        };
    }
}

pub(super) fn transition_attachments(context: &RecordContext<'_>) {
    let color_range = vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1);
    let depth_range = vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL)
        .level_count(1)
        .layer_count(1);
    let barriers = [
        vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::NONE)
            .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .image(context.image)
            .subresource_range(color_range),
        vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::NONE)
            .dst_stage_mask(
                vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS
                    | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
            )
            .dst_access_mask(vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .image(context.depth_image)
            .subresource_range(depth_range),
    ];
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&barriers);
    // SAFETY: Synchronization2 is enabled and old contents are discarded.
    unsafe {
        context
            .device
            .cmd_pipeline_barrier2(context.command_buffer, &dependency)
    };
}

/// Makes the completed world color writes available to the blending UI pass.
///
/// Dynamic rendering scopes do not create an implicit attachment dependency.
/// The overlay keeps the same optimal layout, but its blend operations read
/// the destination color produced by the preceding world/M2 scope.
pub(super) fn transition_to_ui_overlay(context: &RecordContext<'_>) {
    let range = vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1);
    let barriers = [vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        .dst_access_mask(
            vk::AccessFlags2::COLOR_ATTACHMENT_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        )
        .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .image(context.image)
        .subresource_range(range)];
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&barriers);
    // SAFETY: Both rendering scopes use this live swapchain image on the same
    // graphics queue and the first scope has ended before this dependency.
    unsafe {
        context
            .device
            .cmd_pipeline_barrier2(context.command_buffer, &dependency)
    };
}

pub(super) fn transition_to_present(context: &RecordContext<'_>) {
    if let Some(capture) = context.capture {
        capture.record(
            context.device,
            context.command_buffer,
            context.image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );
        return;
    }
    let range = vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1);
    let barriers = [vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::NONE)
        .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
        .image(context.image)
        .subresource_range(range)];
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&barriers);
    // SAFETY: Barrier follows the completed rendering scope.
    unsafe {
        context
            .device
            .cmd_pipeline_barrier2(context.command_buffer, &dependency)
    };
}
