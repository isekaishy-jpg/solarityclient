//! One dynamic-rendering scope for terrain, WMO, and M2 world draws.

#![allow(unsafe_code)]

use ash::{Device, vk};

use crate::device::VulkanError;
use crate::device::vulkan_capture::FrameReadback;
use crate::device::vulkan_celestial::CelestialFrameResources;
use crate::device::vulkan_cloud::CloudFrameResources;
use crate::device::vulkan_frame::swapchain_error;
use crate::device::vulkan_glow::{VulkanGlowRenderer, WorldFrameScreenEffect};
use crate::device::vulkan_liquid::{
    LiquidDrawMaterial, LiquidFrameResources, LiquidMeshRegistry, LiquidPipelines,
    LiquidPreparedDraw,
};
use crate::device::vulkan_m2_draw::M2PreparedDraw;
use crate::device::vulkan_m2_particle_draw::M2ParticlePreparedDraw;
use crate::device::vulkan_m2_particle_pipeline::M2ParticlePipelineRegistry;
use crate::device::vulkan_m2_pipeline::M2PipelineRegistry;
use crate::device::vulkan_m2_ribbon_draw::M2RibbonPreparedDraw;
use crate::device::vulkan_m2_ribbon_pipeline::M2RibbonPipelineRegistry;
use crate::device::vulkan_m2_texture_set::M2TextureSetRegistry;
use crate::device::vulkan_mesh::M2MeshRegistry;
use crate::device::vulkan_pct_pipeline::PctPipeline;
use crate::device::vulkan_ripple::{RippleFrameResources, WaterRippleFrame};
use crate::device::vulkan_sky::SkyFrameResources;
use crate::device::vulkan_terrain_draw::TerrainPreparedDraw;
use crate::device::vulkan_terrain_mesh::TerrainMeshRegistry;
use crate::device::vulkan_terrain_pipeline::TerrainPipelineRegistry;
use crate::device::vulkan_terrain_texture_set::TerrainTextureSetRegistry;
use crate::device::vulkan_ui_frame::{UiOverlayRecordContext, record_loaded_overlay};
use crate::device::vulkan_underwater::{UnderwaterFrameResources, UnderwaterParticleFrame};
use crate::device::vulkan_world_model_draw::WorldModelPreparedDraw;
use crate::device::vulkan_world_model_mesh::WorldModelMeshRegistry;
use crate::device::vulkan_world_model_pipeline::WorldModelPipelineRegistry;
use crate::device::vulkan_world_model_texture_set::WorldModelTextureSetRegistry;

mod bindings;
mod ground_detail;
mod low_detail;
mod shadow;
use bindings::WorldCommandBindings;
use low_detail::record_low_detail;

use super::WorldFrameContext;
use super::resource::WorldFrameSlot;

/// Optional CPU timings around the two queue calls at the submission boundary.
pub(super) struct WorldSubmitTimings {
    pub(super) queue_submit: std::time::Duration,
    pub(super) queue_present: std::time::Duration,
}

pub(super) struct RecordContext<'a> {
    pub(super) shadow_pipeline: &'a crate::device::vulkan_shadow::ShadowPipelines,
    pub(super) shadow_resources: &'a crate::device::vulkan_shadow::ShadowFrameResources,
    pub(super) shadow_frame: Option<crate::WorldPrimaryShadowFrame<'a>>,
    pub(super) environment_frame: Option<crate::WorldEnvironmentShadowFrame<'a>>,
    pub(super) environment_images: &'a crate::device::vulkan_shadow::EnvironmentShadowImages,
    pub(super) device: &'a Device,
    pub(super) capture: Option<&'a FrameReadback>,
    pub(super) command_buffer: vk::CommandBuffer,
    pub(super) image: vk::Image,
    pub(super) image_view: vk::ImageView,
    pub(super) depth_image: vk::Image,
    pub(super) depth_view: vk::ImageView,
    pub(super) extent: (u32, u32),
    pub(super) screen_window: crate::WorldScreenWindow,
    pub(super) sky_window: Option<crate::WorldSkyWindow>,
    pub(super) background_color: glam::Vec4,
    pub(super) frame_sets: [vk::DescriptorSet; 13],
    pub(super) world_model_material_stride: vk::DeviceSize,
    pub(super) m2_material_stride: vk::DeviceSize,
    pub(super) m2_scene_stride: vk::DeviceSize,
    pub(super) terrain_pipelines: &'a TerrainPipelineRegistry,
    pub(super) terrain_meshes: &'a TerrainMeshRegistry,
    pub(super) terrain_texture_sets: &'a TerrainTextureSetRegistry,
    pub(super) liquid_pipelines: &'a LiquidPipelines,
    pub(super) liquid_meshes: &'a LiquidMeshRegistry,
    pub(super) liquid_resources: &'a LiquidFrameResources,
    pub(super) ripple_pipeline: &'a PctPipeline,
    pub(super) ripple_resources: &'a RippleFrameResources,
    pub(super) ripple_frame: Option<WaterRippleFrame<'a>>,
    pub(super) underwater_pipeline: &'a PctPipeline,
    pub(super) underwater_resources: &'a UnderwaterFrameResources,
    pub(super) underwater_frame: Option<UnderwaterParticleFrame<'a>>,
    pub(super) celestial_pipeline: &'a PctPipeline,
    pub(super) celestial_resources: &'a [CelestialFrameResources; 3],
    pub(super) celestial_frame: Option<crate::WorldCelestialFrame<'a>>,
    pub(super) cloud_pipeline: &'a PctPipeline,
    pub(super) cloud_resources: &'a CloudFrameResources,
    pub(super) cloud_frame: Option<crate::WorldCloudFrame<'a>>,
    pub(super) sky_pipeline: &'a PctPipeline,
    pub(super) low_detail_pipelines: &'a crate::device::vulkan_low_detail::LowDetailPipelines,
    pub(super) low_detail_map: Option<&'a crate::device::vulkan_low_detail::LowDetailGpuMap>,
    pub(super) low_detail_frame: Option<crate::WorldLowDetailFrame<'a>>,
    pub(super) detail_pipeline: &'a crate::device::vulkan_detail::DetailPipeline,
    pub(super) ground_detail_registry: &'a crate::device::vulkan_detail::DetailRegistry,
    pub(super) ground_detail_frame: Option<crate::GroundDetailFrame<'a>>,
    pub(super) depth_maximum: f32,
    pub(super) sky_resources: &'a SkyFrameResources,
    pub(super) sky_frame: Option<crate::WorldSkyFrame<'a>>,
    pub(super) liquid_draws: &'a [LiquidPreparedDraw],
    pub(super) liquid_scene_order: u32,
    pub(super) world_model_pipelines: &'a WorldModelPipelineRegistry,
    pub(super) world_model_meshes: &'a WorldModelMeshRegistry,
    pub(super) world_model_texture_sets: &'a WorldModelTextureSetRegistry,
    pub(super) m2_pipelines: &'a M2PipelineRegistry,
    pub(super) m2_meshes: &'a M2MeshRegistry,
    pub(super) m2_texture_sets: &'a M2TextureSetRegistry,
    pub(super) m2_particle_pipelines: &'a M2ParticlePipelineRegistry,
    pub(super) m2_ribbon_pipelines: &'a M2RibbonPipelineRegistry,
    pub(super) terrain_draws: &'a [TerrainPreparedDraw],
    pub(super) world_model_draws: &'a [WorldModelPreparedDraw],
    pub(super) m2_draws: &'a [M2PreparedDraw],
    pub(super) sky_models: Option<super::WorldSkyModelFrame<'a>>,
    pub(super) particle_draws: &'a [M2ParticlePreparedDraw],
    pub(super) ribbon_draws: &'a [M2RibbonPreparedDraw],
    pub(super) particle_vertex_buffer: (vk::Buffer, vk::DeviceSize),
    pub(super) particle_index_buffer: (vk::Buffer, vk::DeviceSize),
    pub(super) ribbon_vertex_buffer: (vk::Buffer, vk::DeviceSize),
    pub(super) ui: Option<UiOverlayRecordContext<'a>>,
    pub(super) glow: Option<(&'a VulkanGlowRenderer, WorldFrameScreenEffect)>,
    pub(super) image_index: u32,
}

pub(super) fn record(context: RecordContext<'_>) -> Result<usize, VulkanError> {
    let begin =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
    // SAFETY: Slot pool was reset and this primary buffer is not pending.
    unsafe {
        context
            .device
            .begin_command_buffer(context.command_buffer, &begin)
    }
    .map_err(|source| VulkanError::operation("begin world command buffer", source))?;
    shadow::record_primary(&context)?;
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
    transition_attachments(&context);
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
    // SAFETY: Every terrain, WMO, and M2 pipeline declares these dynamic.
    unsafe {
        context
            .device
            .cmd_set_viewport(context.command_buffer, 0, &[viewport]);
        context
            .device
            .cmd_set_scissor(context.command_buffer, 0, &[scissor]);
    }
    let mut bindings = WorldCommandBindings::default();
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
        record_sky_models(&context, false, &mut bindings)?;
        record_celestials(&context, &mut bindings);
        record_sky(&context, &mut bindings);
        record_clouds(&context, &mut bindings);
        record_sky_models(&context, true, &mut bindings)?;
        // SAFETY: WDL and subsequent world queues share the original scissor.
        unsafe {
            context
                .device
                .cmd_set_scissor(context.command_buffer, 0, &[scissor]);
        }
    }
    let low_detail_draw_count = record_low_detail(&context, viewport, &mut bindings)?;
    // 79A870 restores the ordinary world interval after horizon/sky work.
    let world_viewport = vk::Viewport {
        max_depth: context.depth_maximum,
        ..viewport
    };
    // SAFETY: All subsequent world pipelines declare a dynamic viewport.
    unsafe {
        context
            .device
            .cmd_set_viewport(context.command_buffer, 0, &[world_viewport]);
    }
    for draw in context.terrain_draws.iter().copied() {
        record_terrain(&context, draw, &mut bindings)?;
    }
    for (index, draw) in context.world_model_draws.iter().copied().enumerate() {
        record_world_model(&context, index, draw, &mut bindings)?;
    }
    // 4F9154 dispatches 7984A0 before the ordinary liquid/M2 scene queues.
    ground_detail::record_ground_detail(&context, &mut bindings)?;
    record_liquid_queue(&context, LiquidQueue::Opaque, &mut bindings)?;
    record_m2_scene_elements(&context, &mut bindings)?;
    record_underwater(&context, &mut bindings);
    // SAFETY: The single matching world rendering scope is active.
    unsafe { context.device.cmd_end_rendering(context.command_buffer) };
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
    if let Some(ui) = context.ui {
        transition_to_ui_overlay(&context);
        record_loaded_overlay(ui)?;
    }
    transition_to_present(&context);
    // SAFETY: Every bound resource outlives slot fence retirement.
    unsafe { context.device.end_command_buffer(context.command_buffer) }
        .map_err(|source| VulkanError::operation("end world command buffer", source))?;
    Ok(low_detail_draw_count)
}

/// Dispatches the typed streams in their one stock scene-element order.
fn record_m2_scene_elements(
    context: &RecordContext<'_>,
    bindings: &mut WorldCommandBindings,
) -> Result<(), VulkanError> {
    let mut next_m2 = 0;
    let mut next_particle = 0;
    let mut next_ribbon = 0;
    let mut water_pending = !context.liquid_draws.is_empty()
        || context
            .ripple_frame
            .is_some_and(|frame| frame.draw_count() != 0);
    loop {
        let m2_key = context
            .m2_draws
            .get(next_m2)
            .map(|draw| (draw.scene_order(), 0_u8));
        let particle_key = context
            .particle_draws
            .get(next_particle)
            .map(|draw| (draw.scene_order(), 4_u8));
        let ribbon_key = context
            .ribbon_draws
            .get(next_ribbon)
            .map(|draw| (draw.scene_order(), 3_u8));
        let next = [
            m2_key.map(|key| (key, 0_u8)),
            particle_key.map(|key| (key, 1_u8)),
            ribbon_key.map(|key| (key, 2_u8)),
        ]
        .into_iter()
        .flatten()
        .min_by_key(|(key, _kind)| *key);
        if water_pending
            && next.is_none_or(|((order, _kind), _)| order >= context.liquid_scene_order)
        {
            record_liquid_queue(context, LiquidQueue::Transparent, bindings)?;
            record_ripples(context, bindings);
            water_pending = false;
        }
        match next.map(|(_key, kind)| kind) {
            Some(0) => {
                let draw = context
                    .m2_draws
                    .get(next_m2)
                    .copied()
                    .ok_or(VulkanError::WorldFrameCapacity)?;
                record_m2(
                    context,
                    next_m2,
                    draw,
                    m2_scene_set(context, draw.light_bank()),
                    bindings,
                )?;
                next_m2 += 1;
            }
            Some(1) => {
                let draw = context
                    .particle_draws
                    .get(next_particle)
                    .copied()
                    .ok_or(VulkanError::WorldFrameCapacity)?;
                record_particle(context, draw, bindings)?;
                next_particle += 1;
            }
            Some(2) => {
                let draw = context
                    .ribbon_draws
                    .get(next_ribbon)
                    .copied()
                    .ok_or(VulkanError::WorldFrameCapacity)?;
                record_ribbon(context, draw, bindings)?;
                next_ribbon += 1;
            }
            Some(_) => return Err(VulkanError::WorldFrameCapacity),
            None => return Ok(()),
        }
    }
}

/// The material flag selected by native 8A27C0/8A20C0 admits two liquid queues.
#[derive(Clone, Copy, Eq, PartialEq)]
enum LiquidQueue {
    Opaque,
    Transparent,
}

/// The sky occupies the reserved far depth range.
fn record_sky(context: &RecordContext<'_>, bindings: &mut WorldCommandBindings) {
    let Some(frame) = context.sky_frame else {
        return;
    };
    let (pipeline, layout) = context.sky_pipeline.raw();
    bindings.bind_pipeline(context, pipeline);
    bindings.bind_vertex(context, context.sky_resources.vertex_buffer());
    bindings.bind_index(
        context,
        context.sky_resources.index_buffer(),
        vk::IndexType::UINT16,
    );
    let matrix = frame.view_projection().to_cols_array();
    let mut pushes = [0_u8; 64];
    for (word, value) in pushes.as_chunks_mut::<4>().0.iter_mut().zip(matrix) {
        *word = value.to_le_bytes();
    }
    // SAFETY: The retired slot owns all 122 vertices and six 50-index strips;
    // the prepared pipeline has the matching 64-byte push range and no sampled inputs.
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
            .cmd_draw_indexed(context.command_buffer, 300, 1, 0, 0, 0);
    }
}

fn record_celestials(context: &RecordContext<'_>, bindings: &mut WorldCommandBindings) {
    let Some(frame) = context.celestial_frame else {
        return;
    };
    let (pipeline, layout) = context.celestial_pipeline.raw();
    for (draw, resources) in frame.draws().into_iter().zip(context.celestial_resources) {
        if draw.mesh().indices().is_empty() {
            continue;
        }
        bindings.bind_pipeline(context, pipeline);
        bindings.bind_vertex(context, resources.vertex_buffer());
        bindings.bind_index(context, resources.index_buffer(), vk::IndexType::UINT16);
        let mut pushes = [0_u8; 64];
        for (word, value) in pushes
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(draw.view_projection().to_cols_array())
        {
            *word = value.to_le_bytes();
        }
        // SAFETY: Slot retirement precedes the six-vertex write and descriptor update;
        // the matching PCT pipeline consumes the complete 64-byte matrix range.
        unsafe {
            context.device.cmd_bind_descriptor_sets(
                context.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                layout,
                0,
                &[resources.descriptor()],
                &[],
            );
            context.device.cmd_push_constants(
                context.command_buffer,
                layout,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                0,
                &pushes,
            );
            context.device.cmd_draw_indexed(
                context.command_buffer,
                draw.mesh().indices().len() as u32,
                1,
                0,
                0,
                0,
            );
        }
    }
}

fn record_clouds(context: &RecordContext<'_>, bindings: &mut WorldCommandBindings) {
    let Some(frame) = context.cloud_frame else {
        return;
    };
    let (pipeline, layout) = context.cloud_pipeline.raw();
    bindings.bind_pipeline(context, pipeline);
    bindings.bind_vertex(context, context.cloud_resources.vertex_buffer());
    bindings.bind_index(
        context,
        context.cloud_resources.index_buffer(),
        vk::IndexType::UINT16,
    );
    let matrix = frame.view_projection().to_cols_array();
    let mut pushes = [0_u8; 64];
    for (word, value) in pushes.as_chunks_mut::<4>().0.iter_mut().zip(matrix) {
        *word = value.to_le_bytes();
    }
    // SAFETY: The retired slot owns all 177 vertices and one 374-index strip;
    // the prepared pipeline has the matching 64-byte push range and a resident procedural texture.
    unsafe {
        context.device.cmd_bind_descriptor_sets(
            context.command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            layout,
            0,
            &[context.cloud_resources.descriptor()],
            &[],
        );
        context.device.cmd_push_constants(
            context.command_buffer,
            layout,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            &pushes,
        );
        context
            .device
            .cmd_draw_indexed(context.command_buffer, 374, 1, 0, 0, 0);
    }
}

fn record_ripples(context: &RecordContext<'_>, bindings: &mut WorldCommandBindings) {
    let Some(frame) = context.ripple_frame.filter(|frame| frame.draw_count() != 0) else {
        return;
    };
    let (pipeline, layout) = context.ripple_pipeline.raw();
    bindings.bind_pipeline(context, pipeline);
    let sets = context.ripple_resources.sets();
    let mut offset = 0;
    // SAFETY: Pipeline preparation and retired-slot writes validate this complete ABI.
    unsafe {
        context.device.cmd_push_constants(
            context.command_buffer,
            layout,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            &frame.push_bytes(),
        );
    }
    for (index, pass) in frame.passes().into_iter().enumerate() {
        let Some(pass) = pass else {
            continue;
        };
        let count = pass.draw_vertex_count();
        if count != 0 {
            bindings.bind_vertex(context, (context.ripple_resources.buffer(), offset));
            // SAFETY: The active pass descriptor was validated/written before acquisition.
            // Sequential native indices address precisely these first low-16-bit vertices.
            unsafe {
                context.device.cmd_bind_descriptor_sets(
                    context.command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    layout,
                    0,
                    &[sets[index]],
                    &[],
                );
                context
                    .device
                    .cmd_draw(context.command_buffer, count, 1, 0, 0);
            }
        }
        offset += (pass.vertices().len() * crate::WaterRippleRenderVertex::BYTE_SIZE) as u64;
    }
}

/// Native 77F9D0 draws camera-relative billboards after the world effect queues.
fn record_underwater(context: &RecordContext<'_>, bindings: &mut WorldCommandBindings) {
    let Some(frame) = context
        .underwater_frame
        .filter(|frame| frame.draw_count() != 0)
    else {
        return;
    };
    let (pipeline, layout) = context.underwater_pipeline.raw();
    bindings.bind_pipeline(context, pipeline);
    bindings.bind_vertex(context, context.underwater_resources.vertex_buffer());
    let (buffer, offset) = context.underwater_resources.index_buffer();
    bindings.bind_index(context, (buffer, offset), vk::IndexType::UINT16);
    // SAFETY: Retired-slot writes validate both complete geometry banks and this
    // texture descriptor; pipeline preparation fixes the 96-byte push ABI.
    unsafe {
        context.device.cmd_push_constants(
            context.command_buffer,
            layout,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            &frame.push_bytes(),
        );
        context.device.cmd_bind_descriptor_sets(
            context.command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            layout,
            0,
            &[context.underwater_resources.descriptor()],
            &[],
        );
        context.device.cmd_draw_indexed(
            context.command_buffer,
            frame.indices().len() as u32,
            1,
            0,
            0,
            0,
        );
    }
}

/// Records one stock queue at its world/M2 stage, retaining the prepared order.
fn record_liquid_queue(
    context: &RecordContext<'_>,
    queue: LiquidQueue,
    bindings: &mut WorldCommandBindings,
) -> Result<(), VulkanError> {
    for (index, draw) in context.liquid_draws.iter().copied().enumerate() {
        let draw_queue = if draw.material() == LiquidDrawMaterial::Magma {
            LiquidQueue::Opaque
        } else {
            LiquidQueue::Transparent
        };
        if draw_queue != queue {
            continue;
        }
        let (pipeline, layout) = context.liquid_pipelines.raw(draw.material().shader());
        let (vertices, indices, count) =
            context.liquid_meshes.raw(draw.mesh()).ok_or_else(|| {
                VulkanError::operation("record liquid draw", "unknown liquid mesh handle")
            })?;
        let (sets, offset) = context.liquid_resources.draw_sets(index)?;
        // SAFETY: Preparation validates handles; the slot fence protects uniform,
        // descriptor, and image storage until this submission has completed.
        unsafe {
            bindings.bind_pipeline(context, pipeline);
            bindings.bind_vertex(context, (vertices, 0));
            bindings.bind_index(context, (indices, 0), vk::IndexType::UINT16);
            context.device.cmd_bind_descriptor_sets(
                context.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                layout,
                0,
                &sets,
                &[offset],
            );
            context
                .device
                .cmd_draw_indexed(context.command_buffer, count, 1, 0, 0, 0);
        }
    }
    Ok(())
}

/// Makes the completed world color writes available to the blending UI pass.
///
/// Dynamic rendering scopes do not create an implicit attachment dependency.
/// The overlay keeps the same optimal layout, but its blend operations read
/// the destination color produced by the preceding world/M2 scope.
fn transition_to_ui_overlay(context: &RecordContext<'_>) {
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

fn record_particle(
    context: &RecordContext<'_>,
    draw: M2ParticlePreparedDraw,
    bindings: &mut WorldCommandBindings,
) -> Result<(), VulkanError> {
    let (pipeline, layout) = context
        .m2_particle_pipelines
        .raw(draw.pipeline())
        .ok_or(VulkanError::UnknownM2ParticlePipelineHandle)?;
    let texture = context
        .m2_texture_sets
        .raw(draw.texture_set())
        .ok_or(VulkanError::UnknownM2TextureSetHandle)?;
    let (scene_set, scene_offset) = instance_scene(
        context,
        draw.scene_index(),
        m2_scene_set(context, draw.light_bank()),
    )?;
    let sets = [scene_set, texture];
    // SAFETY: The prepared packet proves compatible renderer-local handles;
    // frame validation proves every UINT32 index addresses the PNC0T0 stream.
    unsafe {
        bindings.bind_pipeline(context, pipeline);
        bindings.bind_vertex(context, context.particle_vertex_buffer);
        bindings.bind_index(
            context,
            context.particle_index_buffer,
            vk::IndexType::UINT32,
        );
        context.device.cmd_bind_descriptor_sets(
            context.command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            layout,
            0,
            &sets,
            &[scene_offset],
        );
        context.device.cmd_push_constants(
            context.command_buffer,
            layout,
            vk::ShaderStageFlags::FRAGMENT,
            0,
            &draw.alpha_reference().to_le_bytes(),
        );
        context.device.cmd_draw_indexed(
            context.command_buffer,
            draw.index_count(),
            1,
            draw.first_index(),
            draw.vertex_offset(),
            0,
        );
    }
    Ok(())
}

fn record_ribbon(
    context: &RecordContext<'_>,
    draw: M2RibbonPreparedDraw,
    bindings: &mut WorldCommandBindings,
) -> Result<(), VulkanError> {
    let (pipeline, layout) = context
        .m2_ribbon_pipelines
        .raw(draw.pipeline())
        .ok_or(VulkanError::UnknownM2RibbonPipelineHandle)?;
    let texture = context
        .m2_texture_sets
        .raw(draw.texture_set())
        .ok_or(VulkanError::UnknownM2TextureSetHandle)?;
    let (scene_set, scene_offset) = instance_scene(
        context,
        draw.scene_index(),
        m2_scene_set(context, draw.light_bank()),
    )?;
    let sets = [scene_set, texture];
    // SAFETY: The prepared packet proves compatible renderer-local handles and
    // its range was checked against the slot's mapped PCT0 stream.
    unsafe {
        bindings.bind_pipeline(context, pipeline);
        bindings.bind_vertex(context, context.ribbon_vertex_buffer);
        context.device.cmd_bind_descriptor_sets(
            context.command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            layout,
            0,
            &sets,
            &[scene_offset],
        );
        context.device.cmd_draw(
            context.command_buffer,
            draw.vertex_count(),
            1,
            draw.first_vertex(),
            0,
        );
    }
    Ok(())
}

fn record_terrain(
    context: &RecordContext<'_>,
    draw: TerrainPreparedDraw,
    bindings: &mut WorldCommandBindings,
) -> Result<(), VulkanError> {
    let (pipeline, layout) = if context.shadow_frame.is_some() {
        context
            .terrain_pipelines
            .raw_primary_shadow(draw.pipeline())
    } else {
        context.terrain_pipelines.raw(draw.pipeline())
    }
    .ok_or(VulkanError::UnknownTerrainPipelineHandle)?;
    let (vertex, index) = context
        .terrain_meshes
        .buffers(draw.mesh())
        .ok_or(VulkanError::UnknownTerrainMeshHandle)?;
    let texture = context
        .terrain_texture_sets
        .raw(draw.texture_set())
        .ok_or(VulkanError::UnknownTerrainTextureSetHandle)?;
    let sets = [context.frame_sets[0], texture];
    // SAFETY: Prepared draw proves compatible renderer-local resources.
    unsafe {
        bindings.bind_pipeline(context, pipeline);
        bindings.bind_vertex(context, (vertex, 0));
        bindings.bind_index(context, (index, 0), vk::IndexType::UINT16);
        context.device.cmd_bind_descriptor_sets(
            context.command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            layout,
            0,
            &sets,
            &[],
        );
        if context.shadow_frame.is_some() {
            context.device.cmd_bind_descriptor_sets(
                context.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                layout,
                2,
                &[context.shadow_resources.receiver_set()],
                &[],
            );
        }
        context.device.cmd_push_constants(
            context.command_buffer,
            layout,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            &draw.push_bytes(),
        );
        context.device.cmd_draw_indexed(
            context.command_buffer,
            draw.index_count(),
            1,
            draw.first_index(),
            0,
            0,
        );
    }
    Ok(())
}

fn record_world_model(
    context: &RecordContext<'_>,
    draw_index: usize,
    draw: WorldModelPreparedDraw,
    bindings: &mut WorldCommandBindings,
) -> Result<(), VulkanError> {
    let receives_shadow = context.shadow_frame.is_some();
    let (pipeline, layout) = if receives_shadow {
        context
            .world_model_pipelines
            .raw_primary_shadow(draw.pipeline())
    } else {
        context.world_model_pipelines.raw(draw.pipeline())
    }
    .ok_or(VulkanError::UnknownWorldModelPipelineHandle)?;
    let (vertex, index) = context
        .world_model_meshes
        .buffers(draw.mesh())
        .ok_or(VulkanError::UnknownWorldModelMeshHandle)?;
    let texture = context
        .world_model_texture_sets
        .raw(draw.texture_set())
        .ok_or(VulkanError::UnknownWorldModelTextureSetHandle)?;
    let dynamic_offset = dynamic_offset(draw_index, context.world_model_material_stride)?;
    let sets = [
        context.frame_sets[1],
        context.frame_sets[2],
        texture,
        context.shadow_resources.receiver_set(),
    ];
    let sets = &sets[..if receives_shadow { 4 } else { 3 }];
    let range = draw.index_range();
    // SAFETY: Prepared draw proves compatible pipeline, UINT32 mesh, and set.
    unsafe {
        bindings.bind_pipeline(context, pipeline);
        bindings.bind_vertex(context, (vertex, 0));
        bindings.bind_index(context, (index, 0), vk::IndexType::UINT32);
        context.device.cmd_bind_descriptor_sets(
            context.command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            layout,
            0,
            sets,
            &[dynamic_offset],
        );
        context
            .device
            .cmd_draw_indexed(context.command_buffer, range[1], 1, range[0], 0, 0);
    }
    Ok(())
}

/// Each native sky model scene completes before the next sky compositor layer.
fn record_sky_models(
    context: &RecordContext<'_>,
    skyboxes: bool,
    bindings: &mut WorldCommandBindings,
) -> Result<(), VulkanError> {
    let Some(frame) = context.sky_models else {
        return Ok(());
    };
    let mut index = context.m2_draws.len();
    if skyboxes {
        index += frame.stars.len();
        for (slot, batch) in frame.skyboxes.iter().enumerate() {
            for draw in batch.draws {
                record_m2(
                    context,
                    index,
                    *draw,
                    context.frame_sets[9 + slot],
                    bindings,
                )?;
                index += 1;
            }
        }
    } else {
        for draw in frame.stars {
            record_m2(context, index, *draw, context.frame_sets[8], bindings)?;
            index += 1;
        }
    }
    Ok(())
}

fn record_m2(
    context: &RecordContext<'_>,
    draw_index: usize,
    draw: M2PreparedDraw,
    scene_set: vk::DescriptorSet,
    bindings: &mut WorldCommandBindings,
) -> Result<(), VulkanError> {
    // Sky model packets follow the ordinary M2 material range but do not
    // receive the ground-centered primary shadow map.
    let receives_shadow = context.shadow_frame.is_some() && draw_index < context.m2_draws.len();
    let (pipeline, layout) = if receives_shadow {
        context.m2_pipelines.raw_primary_shadow(draw.pipeline())
    } else {
        context.m2_pipelines.raw(draw.pipeline())
    }
    .ok_or(VulkanError::UnknownM2PipelineHandle)?;
    let (vertex, index) = context
        .m2_meshes
        .buffers(draw.mesh())
        .ok_or(VulkanError::UnknownM2MeshHandle)?;
    let texture = context
        .m2_texture_sets
        .raw(draw.texture_set())
        .ok_or(VulkanError::UnknownM2TextureSetHandle)?;
    let dynamic_offset = dynamic_offset(draw_index, context.m2_material_stride)?;
    let (scene_set, scene_offset) = instance_scene(context, draw.scene_index(), scene_set)?;
    let sets = [
        scene_set,
        context.frame_sets[6],
        context.frame_sets[7],
        texture,
        context.shadow_resources.m2_receiver_set(),
    ];
    // SAFETY: Prepared draw proves compatible resources and material range.
    unsafe {
        bindings.bind_pipeline(context, pipeline);
        bindings.bind_vertex(context, (vertex, 0));
        bindings.bind_index(context, (index, 0), vk::IndexType::UINT16);
        context.device.cmd_bind_descriptor_sets(
            context.command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            layout,
            0,
            &sets[..if receives_shadow { 5 } else { 4 }],
            &[scene_offset, dynamic_offset],
        );
        context.device.cmd_push_constants(
            context.command_buffer,
            layout,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            &draw.push_constants().to_bytes(),
        );
        context.device.cmd_draw_indexed(
            context.command_buffer,
            draw.index_count(),
            1,
            draw.first_index(),
            0,
            0,
        );
    }
    Ok(())
}

/// Selects one of the three compatible stock Glue scene descriptors.
fn m2_scene_set(
    context: &RecordContext<'_>,
    light_bank: crate::M2SceneLightBank,
) -> vk::DescriptorSet {
    context.frame_sets[3 + light_bank.index()]
}

fn instance_scene(
    context: &RecordContext<'_>,
    index: Option<u32>,
    fixed: vk::DescriptorSet,
) -> Result<(vk::DescriptorSet, u32), VulkanError> {
    match index {
        Some(index) => Ok((
            context.frame_sets[3],
            dynamic_offset(index as usize + 8, context.m2_scene_stride)?,
        )),
        None => Ok((fixed, 0)),
    }
}

fn dynamic_offset(index: usize, stride: u64) -> Result<u32, VulkanError> {
    (index as u64)
        .checked_mul(stride)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or(VulkanError::WorldFrameCapacity)
}

fn transition_attachments(context: &RecordContext<'_>) {
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

fn transition_to_present(context: &RecordContext<'_>) {
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

pub(super) fn submit_and_present(
    context: &WorldFrameContext<'_>,
    slot: &mut WorldFrameSlot,
    present_semaphore: vk::Semaphore,
    image_index: u32,
    profile: bool,
) -> Result<Option<WorldSubmitTimings>, VulkanError> {
    let waits = [vk::SemaphoreSubmitInfo::default()
        .semaphore(slot.image_available())
        .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)];
    let commands = [vk::CommandBufferSubmitInfo::default().command_buffer(slot.command_buffer())];
    let signals = [vk::SemaphoreSubmitInfo::default()
        .semaphore(present_semaphore)
        .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)];
    let submit = vk::SubmitInfo2::default()
        .wait_semaphore_infos(&waits)
        .command_buffer_infos(&commands)
        .signal_semaphore_infos(&signals);
    slot.reset_fence(context.device)?;
    let queue_submit_started = profile.then(std::time::Instant::now);
    // SAFETY: Command and synchronization resources live through fence retirement.
    if let Err(source) = unsafe {
        context
            .device
            .queue_submit2(context.graphics_queue, &[submit], slot.fence())
    } {
        slot.restore_signaled_fence(context.device)?;
        return Err(VulkanError::operation("submit world frame", source));
    }
    let queue_submit = queue_submit_started.map(|started| started.elapsed());
    let wait = [present_semaphore];
    let swapchains = [context.swapchain];
    let indices = [image_index];
    let present = vk::PresentInfoKHR::default()
        .wait_semaphores(&wait)
        .swapchains(&swapchains)
        .image_indices(&indices);
    let queue_present_started = profile.then(std::time::Instant::now);
    // SAFETY: Presentation waits for this submission's signal.
    unsafe {
        context
            .swapchain_loader
            .queue_present(context.present_queue, &present)
    }
    .map_err(|source| swapchain_error("present world frame", source))?;
    Ok(queue_submit
        .zip(queue_present_started)
        .map(|(queue_submit, queue_present_started)| WorldSubmitTimings {
            queue_submit,
            queue_present: queue_present_started.elapsed(),
        }))
}
