//! One reusable swapchain frame for terrain, WMO, and M2 world geometry.

#![allow(unsafe_code)]

mod command;
mod resource;
mod types;

use ash::{Device, vk};
use glam::Mat4;

use crate::WorldScreenWindow;
use crate::device::VulkanError;
use crate::device::vulkan_capture::FrameReadback;
use crate::device::vulkan_frame::swapchain_error;
use crate::device::vulkan_glow::{VulkanGlowRenderer, WorldFrameGlow};
use crate::device::vulkan_liquid::{LiquidFrameCreateContext, LiquidMeshRegistry, LiquidPipelines};
use crate::device::vulkan_m2_draw::M2PreparedDraw;
use crate::device::vulkan_m2_particle_draw::M2ParticlePreparedDraw;
use crate::device::vulkan_m2_particle_pipeline::M2ParticlePipelineRegistry;
use crate::device::vulkan_m2_pipeline::M2PipelineRegistry;
use crate::device::vulkan_m2_ribbon_draw::M2RibbonPreparedDraw;
use crate::device::vulkan_m2_ribbon_pipeline::M2RibbonPipelineRegistry;
use crate::device::vulkan_m2_texture_set::M2TextureSetRegistry;
use crate::device::vulkan_mesh::M2MeshRegistry;
use crate::device::vulkan_ripple::RipplePipeline;
use crate::device::vulkan_terrain_draw::TerrainPreparedDraw;
use crate::device::vulkan_terrain_mesh::TerrainMeshRegistry;
use crate::device::vulkan_terrain_pipeline::TerrainPipelineRegistry;
use crate::device::vulkan_terrain_texture_set::TerrainTextureSetRegistry;
use crate::device::vulkan_texture::BlpTextureRegistry;
use crate::device::vulkan_ui_draw::UiPreparedDraw;
use crate::device::vulkan_ui_frame::UiOverlayRecordContext;
use crate::device::vulkan_ui_mesh::UiMeshRegistry;
use crate::device::vulkan_ui_pipeline::UiPipelineRegistry;
use crate::device::vulkan_ui_texture_set::UiTextureSetRegistry;
use crate::device::vulkan_world_model_draw::WorldModelPreparedDraw;
use crate::device::vulkan_world_model_mesh::WorldModelMeshRegistry;
use crate::device::vulkan_world_model_pipeline::WorldModelPipelineRegistry;
use crate::device::vulkan_world_model_texture_set::WorldModelTextureSetRegistry;
use crate::{M2ParticleRenderVertex, M2RibbonRenderVertex};

use command::{RecordContext, record, submit_and_present};
use resource::{FrameCreateContext, WorldFrameResources};

pub use types::{WorldFrameReport, WorldFrameScene};

pub(in crate::device) struct WorldFrameContext<'a> {
    pub(in crate::device) device: &'a Device,
    pub(in crate::device) allocator: &'a vk_mem::Allocator,
    pub(in crate::device) capture: Option<&'a FrameReadback>,
    pub(in crate::device) swapchain_loader: &'a ash::khr::swapchain::Device,
    pub(in crate::device) swapchain: vk::SwapchainKHR,
    pub(in crate::device) swapchain_images: &'a [vk::Image],
    pub(in crate::device) image_views: &'a [vk::ImageView],
    pub(in crate::device) graphics_queue: vk::Queue,
    pub(in crate::device) present_queue: vk::Queue,
    pub(in crate::device) graphics_queue_family: u32,
    pub(in crate::device) extent: (u32, u32),
    pub(in crate::device) depth_format: vk::Format,
    pub(in crate::device) uniform_alignment: vk::DeviceSize,
    pub(in crate::device) storage_alignment: vk::DeviceSize,
    pub(in crate::device) terrain_pipelines: &'a TerrainPipelineRegistry,
    pub(in crate::device) terrain_meshes: &'a TerrainMeshRegistry,
    pub(in crate::device) terrain_texture_sets: &'a TerrainTextureSetRegistry,
    pub(in crate::device) liquid_pipelines: &'a LiquidPipelines,
    pub(in crate::device) ripple_pipeline: &'a RipplePipeline,
    pub(in crate::device) liquid_meshes: &'a LiquidMeshRegistry,
    pub(in crate::device) liquid_textures: &'a BlpTextureRegistry,
    pub(in crate::device) maximum_sampler_anisotropy: f32,
    pub(in crate::device) world_model_pipelines: &'a WorldModelPipelineRegistry,
    pub(in crate::device) world_model_meshes: &'a WorldModelMeshRegistry,
    pub(in crate::device) world_model_texture_sets: &'a WorldModelTextureSetRegistry,
    pub(in crate::device) m2_pipelines: &'a M2PipelineRegistry,
    pub(in crate::device) m2_meshes: &'a M2MeshRegistry,
    pub(in crate::device) m2_texture_sets: &'a M2TextureSetRegistry,
    pub(in crate::device) m2_particle_pipelines: &'a M2ParticlePipelineRegistry,
    pub(in crate::device) m2_ribbon_pipelines: &'a M2RibbonPipelineRegistry,
    pub(in crate::device) ui_pipelines: &'a UiPipelineRegistry,
    pub(in crate::device) ui_meshes: &'a UiMeshRegistry,
    pub(in crate::device) ui_texture_sets: &'a UiTextureSetRegistry,
    pub(in crate::device) glow: Option<(&'a VulkanGlowRenderer, WorldFrameGlow)>,
}

/// One color-only UI overlay appended after all world/M2 effect draws.
#[derive(Clone, Copy)]
pub(in crate::device) struct WorldUiOverlay<'a> {
    pub(in crate::device) logical_extent: [f32; 2],
    pub(in crate::device) draws: &'a [UiPreparedDraw],
    pub(in crate::device) overlay: &'a [UiPreparedDraw],
}

/// Partial normalized screen window used by Glue model widgets.
#[derive(Clone, Copy)]
pub(in crate::device) struct WorldFrameWindow {
    pub(in crate::device) screen: WorldScreenWindow,
}

pub(in crate::device) struct WorldFrameRenderer {
    resources: WorldFrameResources,
    profiler: Option<WorldFrameProfiler>,
}

impl Default for WorldFrameRenderer {
    fn default() -> Self {
        Self {
            resources: WorldFrameResources::default(),
            profiler: WorldFrameProfiler::from_environment(),
        }
    }
}

struct WorldFrameProfiler {
    window_started: std::time::Instant,
    frame_count: u64,
    ensure_us: u128,
    wait_write_us: u128,
    acquire_us: u128,
    record_us: u128,
    queue_submit_us: u128,
    queue_present_us: u128,
    maximum_us: [u128; 6],
}

impl WorldFrameProfiler {
    fn from_environment() -> Option<Self> {
        std::env::var_os("SOLARITY_FRAME_TIMINGS").map(|_value| Self {
            window_started: std::time::Instant::now(),
            frame_count: 0,
            ensure_us: 0,
            wait_write_us: 0,
            acquire_us: 0,
            record_us: 0,
            queue_submit_us: 0,
            queue_present_us: 0,
            maximum_us: [0; 6],
        })
    }

    fn record(&mut self, phases: [std::time::Duration; 6]) {
        let elapsed_us = phases.map(|elapsed| elapsed.as_micros());
        self.frame_count = self.frame_count.saturating_add(1);
        self.ensure_us = self.ensure_us.saturating_add(elapsed_us[0]);
        self.wait_write_us = self.wait_write_us.saturating_add(elapsed_us[1]);
        self.acquire_us = self.acquire_us.saturating_add(elapsed_us[2]);
        self.record_us = self.record_us.saturating_add(elapsed_us[3]);
        self.queue_submit_us = self.queue_submit_us.saturating_add(elapsed_us[4]);
        self.queue_present_us = self.queue_present_us.saturating_add(elapsed_us[5]);
        for (maximum, elapsed) in self.maximum_us.iter_mut().zip(elapsed_us) {
            *maximum = (*maximum).max(elapsed);
        }
        let window_elapsed = self.window_started.elapsed();
        if window_elapsed < std::time::Duration::from_secs(2) {
            return;
        }
        let divisor = self.frame_count.max(1) as f64;
        tracing::info!(
            frame_count = self.frame_count,
            ensure_mean_us = self.ensure_us as f64 / divisor,
            wait_write_mean_us = self.wait_write_us as f64 / divisor,
            acquire_mean_us = self.acquire_us as f64 / divisor,
            record_mean_us = self.record_us as f64 / divisor,
            queue_submit_mean_us = self.queue_submit_us as f64 / divisor,
            queue_present_mean_us = self.queue_present_us as f64 / divisor,
            ensure_max_us = self.maximum_us[0],
            wait_write_max_us = self.maximum_us[1],
            acquire_max_us = self.maximum_us[2],
            record_max_us = self.maximum_us[3],
            queue_submit_max_us = self.maximum_us[4],
            queue_present_max_us = self.maximum_us[5],
            "profiled unified Vulkan frame phases"
        );
        self.window_started = std::time::Instant::now();
        self.frame_count = 0;
        self.ensure_us = 0;
        self.wait_write_us = 0;
        self.acquire_us = 0;
        self.record_us = 0;
        self.queue_submit_us = 0;
        self.queue_present_us = 0;
        self.maximum_us = [0; 6];
    }
}

impl WorldFrameRenderer {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::device) fn present(
        &mut self,
        context: WorldFrameContext<'_>,
        descriptor_layouts: [vk::DescriptorSetLayout; 8],
        scene: WorldFrameScene<'_>,
        bone_transforms: &[Mat4],
        terrain_draws: &[TerrainPreparedDraw],
        world_model_draws: &[WorldModelPreparedDraw],
        m2_draws: &[M2PreparedDraw],
        particle_vertices: &[M2ParticleRenderVertex],
        particle_indices: &[u32],
        particle_draws: &[M2ParticlePreparedDraw],
        ribbon_vertices: &[M2RibbonRenderVertex],
        ribbon_draws: &[M2RibbonPreparedDraw],
        window: WorldFrameWindow,
        ui: Option<WorldUiOverlay<'_>>,
    ) -> Result<WorldFrameReport, VulkanError> {
        let ensure_started = std::time::Instant::now();
        if terrain_draws.is_empty()
            && world_model_draws.is_empty()
            && m2_draws.is_empty()
            && particle_draws.is_empty()
            && ribbon_draws.is_empty()
            && scene.liquids().is_none_or(|frame| frame.draws().is_empty())
        {
            return Err(VulkanError::EmptyWorldFrame);
        }
        if let Some(frame) = scene.liquids() {
            for draw in frame.draws() {
                if context.liquid_meshes.raw(draw.mesh()).is_none() {
                    return Err(VulkanError::operation(
                        "validate liquid frame",
                        "unknown liquid mesh handle",
                    ));
                }
                if context.liquid_textures.view(draw.surface()).is_none() {
                    return Err(VulkanError::UnknownBlpTextureHandle);
                }
            }
        }
        if ribbon_draws.iter().any(|draw| {
            usize::try_from(draw.first_vertex())
                .ok()
                .and_then(|first| {
                    usize::try_from(draw.vertex_count())
                        .ok()
                        .and_then(|count| first.checked_add(count))
                })
                .is_none_or(|end| end > ribbon_vertices.len())
        }) {
            return Err(VulkanError::M2RibbonDrawVertexRange);
        }
        if particle_draws.iter().any(|draw| {
            let Some(first_index) = usize::try_from(draw.first_index()).ok() else {
                return true;
            };
            let Some(index_count) = usize::try_from(draw.index_count()).ok() else {
                return true;
            };
            let Some(end_index) = first_index.checked_add(index_count) else {
                return true;
            };
            let Some(indices) = particle_indices.get(first_index..end_index) else {
                return true;
            };
            let Some(vertex_offset) = usize::try_from(draw.vertex_offset()).ok() else {
                return true;
            };
            indices.iter().copied().any(|index| {
                usize::try_from(index)
                    .ok()
                    .and_then(|index| vertex_offset.checked_add(index))
                    .is_none_or(|index| index >= particle_vertices.len())
            })
        }) {
            return Err(VulkanError::M2ParticleDrawIndexRange);
        }
        if m2_draws.iter().any(|draw| {
            context
                .m2_pipelines
                .info(draw.pipeline())
                .is_some_and(|pipeline| pipeline.permutation().has_shadows())
        }) {
            return Err(VulkanError::M2ShadowResourcesUnavailable);
        }
        let required_bones = m2_draws
            .iter()
            .map(|draw| draw.required_bone_transforms())
            .max()
            .unwrap_or(0);
        if required_bones > bone_transforms.len() {
            return Err(VulkanError::M2FrameBoneTransforms {
                required: required_bones,
                available: bone_transforms.len(),
            });
        }
        self.resources.ensure(FrameCreateContext {
            device: context.device,
            allocator: context.allocator,
            descriptor_layouts,
            graphics_queue_family: context.graphics_queue_family,
            slot_count: context.swapchain_images.len(),
            world_model_draw_capacity: world_model_draws.len(),
            m2_draw_capacity: m2_draws.len(),
            bone_capacity: bone_transforms.len(),
            particle_vertex_capacity: particle_vertices
                .len()
                .max(scene.particle_vertex_capacity()),
            particle_index_capacity: particle_indices.len().max(scene.particle_index_capacity()),
            ribbon_vertex_capacity: ribbon_vertices.len(),
            uniform_alignment: context.uniform_alignment,
            storage_alignment: context.storage_alignment,
            extent: context.extent,
            depth_format: context.depth_format,
        })?;
        let ensure_elapsed = ensure_started.elapsed();
        let slot_index = self.resources.next_slot_index()?;
        let wait_write_started = std::time::Instant::now();
        let (acquired, wait_write_elapsed, acquire_elapsed) = {
            let slot = self.resources.slot_mut(slot_index)?;
            slot.wait_and_reset(context.device)?;
            if let Some(frame) = scene.liquids().filter(|frame| !frame.draws().is_empty()) {
                slot.liquids.ensure(LiquidFrameCreateContext {
                    device: context.device,
                    allocator: context.allocator,
                    layouts: context.liquid_pipelines.descriptor_layouts()?,
                    count: frame.draws().len(),
                    alignment: context.uniform_alignment,
                    filtering: frame.filtering(),
                    maximum_anisotropy: context.maximum_sampler_anisotropy,
                })?;
                slot.liquids.write(
                    context.device,
                    context.allocator,
                    context.liquid_textures,
                    frame,
                )?;
            }
            if let Some(frame) = scene.ripples().filter(|frame| frame.draw_count() != 0) {
                slot.ripples.ensure(
                    context.device,
                    context.allocator,
                    context.ripple_pipeline.descriptor_layout(),
                    frame.vertex_count(),
                )?;
                slot.ripples.write(
                    context.device,
                    context.allocator,
                    context.liquid_textures,
                    frame,
                )?;
            }
            slot.write(
                context.allocator,
                scene,
                bone_transforms,
                world_model_draws,
                m2_draws,
                particle_vertices,
                particle_indices,
                ribbon_vertices,
            )?;
            let wait_write_elapsed = wait_write_started.elapsed();
            let acquire_started = std::time::Instant::now();
            // SAFETY: Swapchain and acquire semaphore live through submission.
            let acquired = unsafe {
                context.swapchain_loader.acquire_next_image(
                    context.swapchain,
                    u64::MAX,
                    slot.image_available(),
                    vk::Fence::null(),
                )
            }
            .map_err(|source| swapchain_error("acquire world frame image", source))?;
            (acquired, wait_write_elapsed, acquire_started.elapsed())
        };
        let (image_index, _suboptimal) = acquired;
        let present_semaphore = self.resources.present_semaphore(image_index)?;
        let image = context
            .swapchain_images
            .get(image_index as usize)
            .copied()
            .ok_or(VulkanError::WorldFrameCapacity)?;
        let image_view = context
            .image_views
            .get(image_index as usize)
            .copied()
            .ok_or(VulkanError::WorldFrameCapacity)?;
        let slot = self.resources.slot_mut(slot_index)?;
        let record_started = std::time::Instant::now();
        record(RecordContext {
            device: context.device,
            capture: context.capture,
            command_buffer: slot.command_buffer(),
            image,
            image_view,
            depth_image: slot.depth_image(),
            depth_view: slot.depth_view(),
            extent: context.extent,
            screen_window: window.screen,
            frame_sets: slot.descriptor_sets(),
            world_model_material_stride: slot.world_model_material_stride(),
            m2_material_stride: slot.m2_material_stride(),
            terrain_pipelines: context.terrain_pipelines,
            terrain_meshes: context.terrain_meshes,
            terrain_texture_sets: context.terrain_texture_sets,
            liquid_pipelines: context.liquid_pipelines,
            liquid_meshes: context.liquid_meshes,
            liquid_resources: &slot.liquids,
            ripple_pipeline: context.ripple_pipeline,
            ripple_resources: &slot.ripples,
            ripple_frame: scene.ripples(),
            liquid_draws: scene.liquids().map_or(&[], |frame| frame.draws()),
            liquid_scene_order: scene
                .liquids()
                .map(|frame| frame.water_scene_order())
                .or_else(|| scene.ripples().map(|frame| frame.water_scene_order()))
                .unwrap_or(u32::MAX),
            world_model_pipelines: context.world_model_pipelines,
            world_model_meshes: context.world_model_meshes,
            world_model_texture_sets: context.world_model_texture_sets,
            m2_pipelines: context.m2_pipelines,
            m2_meshes: context.m2_meshes,
            m2_texture_sets: context.m2_texture_sets,
            m2_particle_pipelines: context.m2_particle_pipelines,
            m2_ribbon_pipelines: context.m2_ribbon_pipelines,
            terrain_draws,
            world_model_draws,
            m2_draws,
            particle_draws,
            ribbon_draws,
            particle_vertex_buffer: slot.particle_vertex_buffer(),
            particle_index_buffer: slot.particle_index_buffer(),
            ribbon_vertex_buffer: slot.ribbon_vertex_buffer(),
            ui: ui.map(|ui| UiOverlayRecordContext {
                device: context.device,
                command_buffer: slot.command_buffer(),
                image_view,
                extent: context.extent,
                logical_extent: ui.logical_extent,
                pipelines: context.ui_pipelines,
                meshes: context.ui_meshes,
                texture_sets: context.ui_texture_sets,
                draws: ui.draws,
                overlay: ui.overlay,
            }),
            glow: context.glow,
            image_index,
        })?;
        let record_elapsed = record_started.elapsed();
        let submit_timings = submit_and_present(
            &context,
            slot,
            present_semaphore,
            image_index,
            self.profiler.is_some(),
        )?;
        if let Some(profiler) = self.profiler.as_mut() {
            let submit_timings = submit_timings.ok_or_else(|| {
                VulkanError::operation("profile world frame", "queue timings are unavailable")
            })?;
            profiler.record([
                ensure_elapsed,
                wait_write_elapsed,
                acquire_elapsed,
                record_elapsed,
                submit_timings.queue_submit,
                submit_timings.queue_present,
            ]);
        }
        Ok(WorldFrameReport::new(
            terrain_draws.len(),
            scene.liquids().map_or(0, |frame| frame.draws().len()),
            world_model_draws.len(),
            m2_draws.len(),
            particle_draws.len(),
            particle_vertices.len(),
            particle_indices.len(),
            ribbon_draws.len(),
            ribbon_vertices.len(),
            bone_transforms.len(),
        )
        .with_ripple_draw_count(scene.ripples().map_or(0, |frame| frame.draw_count())))
    }

    pub(in crate::device) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        self.resources.destroy(device, allocator);
    }
}
