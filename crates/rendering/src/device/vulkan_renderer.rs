//! Logical device, queues, swapchain, and image-view lifetime ownership.

#![allow(unsafe_code)]

use std::path::{Path, PathBuf};

use ash::{Device, vk};
use solarity_asset::{BlpTextureSource, DecodedBlpTexture, M2Material, M2Texture};

use crate::device::vulkan_character_atlas::{
    CharacterAtlasTextureHandle, CharacterAtlasTextureRegistry, CharacterAtlasTextureResourceInfo,
};
use crate::device::vulkan_frame::{
    CinematicFrameIdentity, FrameContext, FrameRenderer, FrameUiContext,
};
use crate::device::vulkan_glow::{VulkanGlowRenderer, WorldFrameGlow};
use crate::device::vulkan_m2_draw::{M2PreparedDraw, prepare_draw};
use crate::device::vulkan_m2_frame::{M2FrameContext, M2FrameRenderer, M2FrameReport};
use crate::device::vulkan_m2_particle_draw::{
    M2ParticlePreparedDraw, prepare_draw as prepare_particle_draw,
};
use crate::device::vulkan_m2_particle_pipeline::{
    M2ParticlePipelineHandle, M2ParticlePipelineInfo, M2ParticlePipelineRegistry,
};
use crate::device::vulkan_m2_pipeline::{M2PipelineHandle, M2PipelineInfo, M2PipelineRegistry};
use crate::device::vulkan_m2_ribbon_draw::{
    M2RibbonPreparedDraw, prepare_draw as prepare_ribbon_draw,
};
use crate::device::vulkan_m2_ribbon_pipeline::{
    M2RibbonPipelineHandle, M2RibbonPipelineInfo, M2RibbonPipelineRegistry,
};
use crate::device::vulkan_m2_texture_set::{
    M2TextureSet, M2TextureSetHandle, M2TextureSetInfo, M2TextureSetRegistry,
};
use crate::device::vulkan_mesh::{
    M2MeshHandle, M2MeshRegistry, M2MeshResourceInfo, MeshUploadContext,
};
use crate::device::vulkan_sampler::{M2SamplerHandle, M2SamplerInfo, M2SamplerRegistry};
use crate::device::vulkan_selection::SelectedAdapter;
use crate::device::vulkan_terrain_draw::{
    TerrainPreparedDraw, prepare_draw as prepare_terrain_draw,
};
use crate::device::vulkan_terrain_frame::{
    TerrainFrameContext, TerrainFrameRenderer, TerrainFrameReport,
};
use crate::device::vulkan_terrain_material::{
    TerrainMaterialHandle, TerrainMaterialRegistry, TerrainMaterialResourceInfo,
};
use crate::device::vulkan_terrain_mesh::{
    TerrainMeshHandle, TerrainMeshRegistry, TerrainMeshResourceInfo,
};
use crate::device::vulkan_terrain_pipeline::{
    TerrainPipelineHandle, TerrainPipelineInfo, TerrainPipelineRegistry,
};
use crate::device::vulkan_terrain_texture_set::{
    TerrainTextureSet, TerrainTextureSetHandle, TerrainTextureSetInfo, TerrainTextureSetRegistry,
};
use crate::device::vulkan_texture::{
    BlpColorSpace, BlpTextureHandle, BlpTextureRegistry, BlpTextureResourceInfo,
    BlpTextureUploadError, BlpTextureUploadRequest, TextureUploadContext,
};
use crate::device::vulkan_ui_draw::{UiPreparedDraw, prepare_draw as prepare_ui_draw};
use crate::device::vulkan_ui_frame::{UiFrameContext, UiFrameRenderer, UiFrameReport};
use crate::device::vulkan_ui_glyph_texture::{
    UiGlyphTextureHandle, UiGlyphTextureRegistry, UiGlyphTextureResourceInfo,
};
use crate::device::vulkan_ui_mesh::{UiMeshHandle, UiMeshRegistry, UiMeshResourceInfo};
use crate::device::vulkan_ui_pipeline::{UiPipelineHandle, UiPipelineInfo, UiPipelineRegistry};
use crate::device::vulkan_ui_sampler::{UiSamplerHandle, UiSamplerInfo, UiSamplerRegistry};
use crate::device::vulkan_ui_texture_set::{
    UiSampledTexture, UiTextureSetHandle, UiTextureSetInfo, UiTextureSetRegistry,
};
use crate::device::vulkan_world_frame::{
    WorldFrameContext, WorldFrameRenderer, WorldFrameReport, WorldFrameScene, WorldFrameWindow,
    WorldUiOverlay,
};
use crate::device::vulkan_world_model_draw::{
    WorldModelPreparedDraw, prepare_draw as prepare_world_model_draw,
};
use crate::device::vulkan_world_model_mesh::{
    WorldModelMeshHandle, WorldModelMeshRegistry, WorldModelMeshResourceInfo,
};
use crate::device::vulkan_world_model_pipeline::{
    WorldModelPipelineHandle, WorldModelPipelineInfo, WorldModelPipelineRegistry,
};
use crate::device::vulkan_world_model_sampler::{
    WorldModelBaseMip, WorldModelSamplerHandle, WorldModelSamplerInfo, WorldModelSamplerRegistry,
    WorldModelTextureFiltering,
};
use crate::device::vulkan_world_model_texture_set::{
    WorldModelTextureSet, WorldModelTextureSetHandle, WorldModelTextureSetInfo,
    WorldModelTextureSetRegistry,
};
use crate::device::{VulkanBootstrap, VulkanError};
use crate::model::{CharacterAtlasTexture, M2MaterialUniform, M2MeshPlan, WorldModelMeshPlan};
use crate::model::{M2EffectOrder, M2SceneUniform};
use crate::shader::{M2ShaderPermutation, M2ShaderPlan, TerrainLayerCount};
use crate::{
    M2ParticleMeshPlan, M2ParticleSpirvProgram, M2RibbonMeshPlan, M2RibbonSpirvProgram,
    M2SpirvProgram, TerrainSceneUniform, TerrainTileMeshPlan, UiMeshPlan, UiRenderBlend,
    UiShaderSource, WorldModelSurfacePass,
};
use glam::{Mat4, Vec3};

/// Immutable evidence for the concrete Vulkan stack selected at startup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VulkanReport {
    device_name: String,
    api_version: u32,
    graphics_queue_family: u32,
    present_queue_family: u32,
    swapchain_image_count: usize,
    extent: (u32, u32),
    presented_texture_extent: Option<(u32, u32)>,
    presented_source_reused: Option<bool>,
    presented_ui_draw_count: Option<usize>,
}

impl VulkanReport {
    /// Returns the Vulkan-reported physical adapter name.
    #[must_use]
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Returns the packed Vulkan API version reported by the adapter.
    #[must_use]
    pub const fn api_version(&self) -> u32 {
        self.api_version
    }

    /// Returns the queue family used for graphics submissions.
    #[must_use]
    pub const fn graphics_queue_family(&self) -> u32 {
        self.graphics_queue_family
    }

    /// Returns the queue family used for presentation.
    #[must_use]
    pub const fn present_queue_family(&self) -> u32 {
        self.present_queue_family
    }

    /// Returns the number of images owned by the swapchain.
    #[must_use]
    pub const fn swapchain_image_count(&self) -> usize {
        self.swapchain_image_count
    }

    /// Returns the physical swapchain width and height.
    #[must_use]
    pub const fn extent(&self) -> (u32, u32) {
        self.extent
    }

    /// Returns the decoded BLP extent after the first frame has been presented.
    #[must_use]
    pub const fn presented_texture_extent(&self) -> Option<(u32, u32)> {
        self.presented_texture_extent
    }

    /// Reports whether the most recent decoded-pixel presentation reused its upload.
    #[must_use]
    pub const fn presented_source_reused(&self) -> Option<bool> {
        self.presented_source_reused
    }

    /// Returns the number of batches in the most recently presented UI frame.
    #[must_use]
    pub const fn presented_ui_draw_count(&self) -> Option<usize> {
        self.presented_ui_draw_count
    }
}

/// Sole owner of the initialized Vulkan presentation object graph.
pub struct VulkanRenderer {
    // Manual drop order is M2 buffers/pipelines, image views, swapchain,
    // allocator, device, then the bootstrap's surface, instance, and loader.
    bootstrap: VulkanBootstrap,
    adapter_index: usize,
    present_mode: VulkanPresentMode,
    device: Device,
    pipeline_cache: vk::PipelineCache,
    pipeline_cache_path: Option<PathBuf>,
    allocator: Option<vk_mem::Allocator>,
    m2_pipelines: M2PipelineRegistry,
    m2_particle_pipelines: M2ParticlePipelineRegistry,
    m2_ribbon_pipelines: M2RibbonPipelineRegistry,
    m2_frames: M2FrameRenderer,
    m2_meshes: M2MeshRegistry,
    world_model_meshes: WorldModelMeshRegistry,
    world_model_pipelines: WorldModelPipelineRegistry,
    world_model_samplers: WorldModelSamplerRegistry,
    world_model_texture_sets: WorldModelTextureSetRegistry,
    world_frames: WorldFrameRenderer,
    glow: VulkanGlowRenderer,
    terrain_meshes: TerrainMeshRegistry,
    terrain_materials: TerrainMaterialRegistry,
    terrain_pipelines: TerrainPipelineRegistry,
    terrain_frames: TerrainFrameRenderer,
    terrain_texture_sets: TerrainTextureSetRegistry,
    m2_samplers: M2SamplerRegistry,
    m2_texture_sets: M2TextureSetRegistry,
    character_atlas_textures: CharacterAtlasTextureRegistry,
    ui_pipelines: UiPipelineRegistry,
    cinematic_frames: FrameRenderer,
    ui_frames: UiFrameRenderer,
    ui_meshes: UiMeshRegistry,
    ui_samplers: UiSamplerRegistry,
    ui_texture_sets: UiTextureSetRegistry,
    ui_glyph_textures: UiGlyphTextureRegistry,
    blp_textures: BlpTextureRegistry,
    swapchain_loader: ash::khr::swapchain::Device,
    swapchain: vk::SwapchainKHR,
    swapchain_images: Vec<vk::Image>,
    image_views: Vec<vk::ImageView>,
    color_format: vk::Format,
    depth_format: vk::Format,
    graphics_queue: vk::Queue,
    present_queue: vk::Queue,
    report: VulkanReport,
    is_idle: bool,
    uniform_buffer_alignment: vk::DeviceSize,
    storage_buffer_alignment: vk::DeviceSize,
    sampler_anisotropy: bool,
    maximum_sampler_anisotropy: f32,
}

/// Stock `gxVSync` presentation policy requested at device startup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VulkanPresentMode {
    /// Present at the display cadence through Vulkan's required FIFO mode.
    Synchronized,
    /// Prefer immediate or mailbox presentation, falling back to FIFO.
    Uncapped,
}

impl VulkanRenderer {
    /// Completes device and swapchain initialization for the attached surface.
    pub(super) fn start(
        bootstrap: VulkanBootstrap,
        requested_extent: (u32, u32),
        adapter_index: usize,
        present_mode: VulkanPresentMode,
    ) -> Result<Self, VulkanError> {
        let selected = SelectedAdapter::select(&bootstrap, adapter_index, present_mode)?;
        let device = create_device(&bootstrap, &selected)?;
        // SAFETY: Both family indices were queried from this physical device,
        // and queue zero was requested during logical-device creation.
        let graphics_queue = unsafe { device.get_device_queue(selected.graphics_family, 0) };
        // SAFETY: Same invariant as the graphics queue; families may be equal.
        let present_queue = unsafe { device.get_device_queue(selected.present_family, 0) };
        let swapchain_loader = ash::khr::swapchain::Device::new(&bootstrap.instance, &device);

        let extent = choose_extent(selected.surface_capabilities, requested_extent);
        let mut renderer = Self {
            bootstrap,
            adapter_index,
            present_mode,
            device,
            pipeline_cache: vk::PipelineCache::null(),
            pipeline_cache_path: None,
            allocator: None,
            m2_pipelines: M2PipelineRegistry::default(),
            m2_particle_pipelines: M2ParticlePipelineRegistry::default(),
            m2_ribbon_pipelines: M2RibbonPipelineRegistry::default(),
            m2_frames: M2FrameRenderer::default(),
            m2_meshes: M2MeshRegistry::default(),
            world_model_meshes: WorldModelMeshRegistry::default(),
            world_model_pipelines: WorldModelPipelineRegistry::default(),
            world_model_samplers: WorldModelSamplerRegistry::default(),
            world_model_texture_sets: WorldModelTextureSetRegistry::default(),
            world_frames: WorldFrameRenderer::default(),
            glow: VulkanGlowRenderer::default(),
            terrain_meshes: TerrainMeshRegistry::default(),
            terrain_materials: TerrainMaterialRegistry::default(),
            terrain_pipelines: TerrainPipelineRegistry::default(),
            terrain_frames: TerrainFrameRenderer::default(),
            terrain_texture_sets: TerrainTextureSetRegistry::default(),
            m2_samplers: M2SamplerRegistry::default(),
            m2_texture_sets: M2TextureSetRegistry::default(),
            character_atlas_textures: CharacterAtlasTextureRegistry::default(),
            ui_pipelines: UiPipelineRegistry::default(),
            cinematic_frames: FrameRenderer::default(),
            ui_frames: UiFrameRenderer::default(),
            ui_meshes: UiMeshRegistry::default(),
            ui_samplers: UiSamplerRegistry::default(),
            ui_texture_sets: UiTextureSetRegistry::default(),
            ui_glyph_textures: UiGlyphTextureRegistry::default(),
            blp_textures: BlpTextureRegistry::default(),
            swapchain_loader,
            swapchain: vk::SwapchainKHR::null(),
            swapchain_images: Vec::new(),
            image_views: Vec::new(),
            color_format: selected.surface_format.format,
            depth_format: selected.depth_format,
            graphics_queue,
            present_queue,
            report: VulkanReport {
                // The selection remains borrowed until swapchain construction;
                // the report independently owns its diagnostic adapter label.
                device_name: selected.device_name.clone(),
                api_version: selected.api_version,
                graphics_queue_family: selected.graphics_family,
                present_queue_family: selected.present_family,
                swapchain_image_count: 0,
                extent: (extent.width, extent.height),
                presented_texture_extent: None,
                presented_source_reused: None,
                presented_ui_draw_count: None,
            },
            is_idle: false,
            uniform_buffer_alignment: selected.uniform_buffer_alignment,
            storage_buffer_alignment: selected.storage_buffer_alignment,
            sampler_anisotropy: selected.sampler_anisotropy,
            maximum_sampler_anisotropy: selected.maximum_sampler_anisotropy,
        };
        renderer.replace_pipeline_cache(&[])?;
        renderer.create_allocator(selected.physical_device)?;
        renderer.create_swapchain(&selected, extent)?;
        Ok(renderer)
    }

    /// Returns immutable startup facts for diagnostics and compatibility tests.
    #[must_use]
    pub const fn report(&self) -> &VulkanReport {
        &self.report
    }

    /// Waits until all submitted device work is complete before owner teardown.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] if the driver reports device loss or another
    /// failure while waiting. Drop still attempts safe handle destruction.
    pub fn shutdown(&mut self) -> Result<(), VulkanError> {
        self.wait_idle()?;
        self.save_pipeline_cache()
    }

    /// Loads driver pipeline data and selects its persistence destination.
    ///
    /// Callers configure this before preparing graphics pipelines. Invalid or
    /// incompatible driver data is discarded and replaced by an empty cache.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] if the cache directory cannot be created, its
    /// data cannot be read, or Vulkan cannot create an empty cache.
    pub fn configure_pipeline_cache(&mut self, path: &Path) -> Result<(), VulkanError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| {
                VulkanError::operation("create Vulkan pipeline-cache directory", source)
            })?;
        }
        let initial_data = match std::fs::read(path) {
            Ok(data) => data,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(source) => {
                return Err(VulkanError::operation("read Vulkan pipeline cache", source));
            }
        };
        if let Err(source) = self.replace_pipeline_cache(&initial_data) {
            tracing::warn!(
                path = %path.display(),
                error = %source,
                "discarding incompatible Vulkan pipeline cache"
            );
            self.replace_pipeline_cache(&[])?;
        }
        self.pipeline_cache_path = Some(path.to_path_buf());
        Ok(())
    }

    /// Persists all driver data learned since the previous save.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] when the driver cannot export its cache or the
    /// configured destination cannot be written.
    pub fn save_pipeline_cache(&self) -> Result<(), VulkanError> {
        let Some(path) = self.pipeline_cache_path.as_ref() else {
            return Ok(());
        };
        // SAFETY: The cache is live, externally synchronized by this renderer,
        // and the returned bytes are copied into Rust-owned storage.
        let data = unsafe { self.device.get_pipeline_cache_data(self.pipeline_cache) }
            .map_err(|source| VulkanError::operation("read Vulkan pipeline-cache data", source))?;
        std::fs::write(path, data)
            .map_err(|source| VulkanError::operation("write Vulkan pipeline cache", source))
    }

    /// Uploads and presents one decoded stock BLP before the window is revealed.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] when frame composition, staging, command
    /// submission, synchronization, or presentation fails.
    pub fn present_blp(&mut self, texture: &DecodedBlpTexture) -> Result<(), VulkanError> {
        self.present_rgba8((texture.width(), texture.height()), texture.rgba8())?;
        self.report.presented_texture_extent = Some((texture.width(), texture.height()));
        Ok(())
    }

    /// Fits and presents one tightly packed RGBA8 frame.
    ///
    /// This direct pixel boundary serves decoded cinematics without claiming a
    /// texture-cache identity or retaining the caller's frame allocation.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] when the source extent or byte count is invalid,
    /// or when staging, command submission, synchronization, or presentation
    /// fails.
    pub fn present_rgba8(
        &mut self,
        source_extent: (u32, u32),
        rgba8: &[u8],
    ) -> Result<(), VulkanError> {
        self.with_swapchain_retry(|renderer| {
            renderer.present_rgba8_once(source_extent, rgba8, None, None)
        })
    }

    /// Presents one retained authored movie frame with linear hardware scaling.
    ///
    /// Repeated calls with the same identity reuse the device-local decoded
    /// image while still presenting at the display's FIFO cadence.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for malformed pixels or Vulkan failures.
    pub fn present_cinematic_rgba8(
        &mut self,
        identity: CinematicFrameIdentity,
        source_extent: (u32, u32),
        rgba8: &[u8],
    ) -> Result<(), VulkanError> {
        self.with_swapchain_retry(|renderer| {
            renderer.present_rgba8_once(source_extent, rgba8, Some(identity), None)
        })
    }

    /// Fits one tightly packed RGBA8 frame and composites retained UI over it.
    ///
    /// This is the movie presentation boundary: decoded pixels retain the
    /// direct transfer path while process-wide overlays are blended before the
    /// acquired swapchain image enters presentation layout.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for invalid source or logical extents, malformed
    /// pixels, stale UI resources, or Vulkan presentation failures.
    pub fn present_rgba8_with_ui(
        &mut self,
        source_extent: (u32, u32),
        rgba8: &[u8],
        logical_extent: [f32; 2],
        draws: &[UiPreparedDraw],
    ) -> Result<(), VulkanError> {
        if logical_extent
            .iter()
            .any(|extent| !extent.is_finite() || *extent <= 0.0)
        {
            return Err(VulkanError::UiFrameExtent);
        }
        self.with_swapchain_retry(|renderer| {
            renderer.present_rgba8_once(source_extent, rgba8, None, Some((logical_extent, draws)))
        })?;
        self.report.presented_ui_draw_count = Some(draws.len());
        Ok(())
    }

    /// Presents one retained authored movie frame and its process-wide UI.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for invalid extents, malformed pixels, stale UI
    /// resources, or Vulkan presentation failures.
    pub fn present_cinematic_rgba8_with_ui(
        &mut self,
        identity: CinematicFrameIdentity,
        source_extent: (u32, u32),
        rgba8: &[u8],
        logical_extent: [f32; 2],
        draws: &[UiPreparedDraw],
    ) -> Result<(), VulkanError> {
        if logical_extent
            .iter()
            .any(|extent| !extent.is_finite() || *extent <= 0.0)
        {
            return Err(VulkanError::UiFrameExtent);
        }
        self.with_swapchain_retry(|renderer| {
            renderer.present_rgba8_once(
                source_extent,
                rgba8,
                Some(identity),
                Some((logical_extent, draws)),
            )
        })?;
        self.report.presented_ui_draw_count = Some(draws.len());
        Ok(())
    }

    fn present_rgba8_once(
        &mut self,
        source_extent: (u32, u32),
        rgba8: &[u8],
        identity: Option<CinematicFrameIdentity>,
        ui: Option<([f32; 2], &[UiPreparedDraw])>,
    ) -> Result<(), VulkanError> {
        let allocator = self.allocator.as_ref().ok_or_else(|| {
            VulkanError::operation("access Vulkan allocator", "allocator is unavailable")
        })?;
        let uploaded = self.cinematic_frames.present(FrameContext {
            device: &self.device,
            allocator,
            swapchain_loader: &self.swapchain_loader,
            swapchain: self.swapchain,
            swapchain_images: &self.swapchain_images,
            image_views: &self.image_views,
            graphics_queue: self.graphics_queue,
            present_queue: self.present_queue,
            graphics_queue_family: self.report.graphics_queue_family,
            frame_extent: self.report.extent,
            source_extent,
            rgba8,
            identity,
            ui: ui.map(|(logical_extent, draws)| FrameUiContext {
                logical_extent,
                pipelines: &self.ui_pipelines,
                meshes: &self.ui_meshes,
                texture_sets: &self.ui_texture_sets,
                draws,
            }),
        })?;
        self.report.presented_source_reused = Some(!uploaded);
        self.is_idle = false;
        Ok(())
    }

    fn with_swapchain_retry<T>(
        &mut self,
        mut present: impl FnMut(&mut Self) -> Result<T, VulkanError>,
    ) -> Result<T, VulkanError> {
        match present(self) {
            Err(VulkanError::SwapchainOutOfDate) => {
                self.recreate_swapchain()?;
                present(self)
            }
            result => result,
        }
    }

    /// Rebuilds every swapchain-shaped frame owner after the desktop surface
    /// reports its ordinary resize/out-of-date transition.
    fn recreate_swapchain(&mut self) -> Result<(), VulkanError> {
        let previous_extent = self.report.extent;
        self.wait_idle()?;
        let allocator = self.allocator.as_ref().ok_or_else(|| {
            VulkanError::operation("access Vulkan allocator", "allocator is unavailable")
        })?;
        self.cinematic_frames.destroy(&self.device, allocator);
        self.ui_frames.destroy(&self.device);
        self.world_frames.destroy(&self.device, allocator);
        self.glow.destroy(&self.device, allocator);
        self.terrain_frames.destroy(&self.device, allocator);
        self.m2_frames.destroy(&self.device, allocator);
        // SAFETY: The device is idle, so no frame can reference these views or
        // the old swapchain while they are released in dependency order.
        unsafe {
            for image_view in self.image_views.drain(..).rev() {
                self.device.destroy_image_view(image_view, None);
            }
            if self.swapchain != vk::SwapchainKHR::null() {
                self.swapchain_loader
                    .destroy_swapchain(self.swapchain, None);
                self.swapchain = vk::SwapchainKHR::null();
            }
        }
        self.swapchain_images.clear();
        let selected =
            SelectedAdapter::select(&self.bootstrap, self.adapter_index, self.present_mode)?;
        if selected.surface_format.format != self.color_format
            || selected.depth_format != self.depth_format
            || selected.graphics_family != self.report.graphics_queue_family
            || selected.present_family != self.report.present_queue_family
        {
            return Err(VulkanError::operation(
                "recreate swapchain",
                "adapter presentation contract changed",
            ));
        }
        let extent = choose_extent(selected.surface_capabilities, self.report.extent);
        self.report.extent = (extent.width, extent.height);
        self.create_swapchain(&selected, extent)?;
        tracing::info!(
            previous_width = previous_extent.0,
            previous_height = previous_extent.1,
            width = extent.width,
            height = extent.height,
            image_count = self.swapchain_images.len(),
            "recreated Vulkan swapchain"
        );
        Ok(())
    }

    /// Uploads one selected M2 profile to shared device-local geometry buffers.
    ///
    /// Model path and profile form the resource identity, matching the decoded
    /// asset cache. Repeated uploads return the existing stable handle without
    /// another allocation or transfer.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] when the plan is empty, handle capacity is
    /// exhausted, or Vulkan allocation, recording, submission, or waiting fails.
    pub fn upload_m2_mesh(&mut self, plan: &M2MeshPlan) -> Result<M2MeshHandle, VulkanError> {
        let allocator = self.allocator.as_ref().ok_or_else(|| {
            VulkanError::operation("access Vulkan allocator", "allocator is unavailable")
        })?;
        self.m2_meshes.upload(
            MeshUploadContext {
                device: &self.device,
                allocator,
                graphics_queue: self.graphics_queue,
                graphics_queue_family: self.report.graphics_queue_family,
            },
            plan,
        )
    }

    /// Returns immutable diagnostics for a live renderer-owned M2 resource.
    #[must_use]
    pub fn m2_mesh_info(&self, handle: M2MeshHandle) -> Option<&M2MeshResourceInfo> {
        self.m2_meshes.info(handle)
    }

    /// Uploads one combined root/group WMO generation to device-local buffers.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for empty geometry, handle exhaustion, or a
    /// failed Vulkan allocation, transfer, submission, or queue wait.
    pub fn upload_world_model_mesh(
        &mut self,
        plan: &WorldModelMeshPlan,
    ) -> Result<WorldModelMeshHandle, VulkanError> {
        let allocator = self.allocator.as_ref().ok_or_else(|| {
            VulkanError::operation("access Vulkan allocator", "allocator is unavailable")
        })?;
        self.world_model_meshes.upload(
            MeshUploadContext {
                device: &self.device,
                allocator,
                graphics_queue: self.graphics_queue,
                graphics_queue_family: self.report.graphics_queue_family,
            },
            plan,
        )
    }

    /// Returns immutable diagnostics for one live WMO geometry allocation.
    #[must_use]
    pub fn world_model_mesh_info(
        &self,
        handle: WorldModelMeshHandle,
    ) -> Option<&WorldModelMeshResourceInfo> {
        self.world_model_meshes.info(handle)
    }

    /// Uploads one aggregate resident ADT to shared device-local buffers.
    ///
    /// Repeated submission of the same immutable plan returns its stable
    /// renderer-local handle without another staging allocation or queue wait.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for empty geometry, exhausted handle space, or
    /// Vulkan allocation, transfer, submission, and synchronization failures.
    pub fn upload_terrain_mesh(
        &mut self,
        plan: &TerrainTileMeshPlan,
    ) -> Result<TerrainMeshHandle, VulkanError> {
        let allocator = self.allocator.as_ref().ok_or_else(|| {
            VulkanError::operation("access Vulkan allocator", "allocator is unavailable")
        })?;
        self.terrain_meshes.upload(
            MeshUploadContext {
                device: &self.device,
                allocator,
                graphics_queue: self.graphics_queue,
                graphics_queue_family: self.report.graphics_queue_family,
            },
            plan,
        )
    }

    /// Returns immutable diagnostics for one live terrain allocation.
    #[must_use]
    pub fn terrain_mesh_info(&self, handle: TerrainMeshHandle) -> Option<TerrainMeshResourceInfo> {
        self.terrain_meshes.info(handle)
    }

    /// Uploads the resident ADT's combined RGB blend and alpha shadow atlas.
    ///
    /// The image is linear RGBA8: blend weights and authored shadow opacity
    /// must not receive sRGB conversion. Repeated submission deduplicates by
    /// immutable tile-plan identity.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for exhausted handle space or Vulkan image,
    /// staging, transfer, synchronization, and view-creation failures.
    pub fn upload_terrain_material(
        &mut self,
        plan: &TerrainTileMeshPlan,
    ) -> Result<TerrainMaterialHandle, VulkanError> {
        let allocator = self.allocator.as_ref().ok_or_else(|| {
            VulkanError::operation("access Vulkan allocator", "allocator is unavailable")
        })?;
        self.terrain_materials.upload(
            TextureUploadContext {
                device: &self.device,
                allocator,
                graphics_queue: self.graphics_queue,
                graphics_queue_family: self.report.graphics_queue_family,
            },
            plan,
        )
    }

    /// Returns immutable diagnostics for one live terrain material atlas.
    #[must_use]
    pub fn terrain_material_info(
        &self,
        handle: TerrainMaterialHandle,
    ) -> Option<TerrainMaterialResourceInfo> {
        self.terrain_materials.info(handle)
    }

    /// Creates or retrieves the opaque pipeline for one MCNK layer count.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for shader compilation, layout creation, handle
    /// exhaustion, or driver pipeline creation failure.
    pub fn prepare_terrain_pipeline(
        &mut self,
        layer_count: TerrainLayerCount,
    ) -> Result<TerrainPipelineHandle, VulkanError> {
        self.terrain_pipelines.prepare(
            &self.device,
            self.color_format,
            self.depth_format,
            layer_count,
        )
    }

    /// Returns immutable diagnostics for one live terrain pipeline.
    #[must_use]
    pub fn terrain_pipeline_info(
        &self,
        handle: TerrainPipelineHandle,
    ) -> Option<TerrainPipelineInfo> {
        self.terrain_pipelines.info(handle)
    }

    /// Creates persistent terrain atlas/diffuse descriptor sets in one batch.
    ///
    /// Unique requests share a descriptor set, and pool capacity is derived
    /// from this call's exact new-set count rather than a guessed global limit.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for foreign image handles, handle exhaustion,
    /// sampler/layout creation, pool creation, or descriptor allocation failure.
    pub fn prepare_terrain_texture_sets(
        &mut self,
        requested: &[TerrainTextureSet],
    ) -> Result<Vec<TerrainTextureSetHandle>, VulkanError> {
        let layout = self.terrain_pipelines.material_set_layout(&self.device)?;
        self.terrain_texture_sets.prepare(
            &self.device,
            layout,
            &self.terrain_materials,
            &self.blp_textures,
            requested,
        )
    }

    /// Returns immutable diagnostics for one live terrain texture set.
    #[must_use]
    pub fn terrain_texture_set_info(
        &self,
        handle: TerrainTextureSetHandle,
    ) -> Option<TerrainTextureSetInfo> {
        self.terrain_texture_sets.info(handle)
    }

    /// Joins one authored MCNK to matching renderer-local terrain resources.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for foreign handles, plan skew, invalid index
    /// ranges, non-sRGB diffuse images, or layer/pipeline/descriptor mismatch.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_terrain_draw(
        &self,
        mesh: TerrainMeshHandle,
        pipeline: TerrainPipelineHandle,
        texture_set: TerrainTextureSetHandle,
        texture_request: &TerrainTextureSet,
        plan: &TerrainTileMeshPlan,
        chunk_index: usize,
    ) -> Result<TerrainPreparedDraw, VulkanError> {
        prepare_terrain_draw(
            &self.terrain_meshes,
            &self.terrain_materials,
            &self.terrain_pipelines,
            &self.terrain_texture_sets,
            &self.blp_textures,
            mesh,
            pipeline,
            texture_set,
            texture_request,
            plan,
            chunk_index,
        )
    }

    /// Records and presents one camera-selected terrain draw list.
    ///
    /// Scene buffers, depth images, descriptors, commands, and synchronization
    /// are allocated once per swapchain image and reused without steady-state
    /// allocation. Draw order is preserved exactly as submitted.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for empty input, frame-resource creation, scene
    /// upload, command recording, queue submission, or presentation failure.
    pub fn present_terrain(
        &mut self,
        scene: TerrainSceneUniform,
        draws: &[TerrainPreparedDraw],
    ) -> Result<TerrainFrameReport, VulkanError> {
        self.with_swapchain_retry(|renderer| renderer.present_terrain_once(scene, draws))
    }

    fn present_terrain_once(
        &mut self,
        scene: TerrainSceneUniform,
        draws: &[TerrainPreparedDraw],
    ) -> Result<TerrainFrameReport, VulkanError> {
        let allocator = self.allocator.as_ref().ok_or_else(|| {
            VulkanError::operation("access Vulkan allocator", "allocator is unavailable")
        })?;
        let scene_layout = self.terrain_pipelines.scene_set_layout(&self.device)?;
        let report = self.terrain_frames.present(
            TerrainFrameContext {
                device: &self.device,
                allocator,
                swapchain_loader: &self.swapchain_loader,
                swapchain: self.swapchain,
                swapchain_images: &self.swapchain_images,
                image_views: &self.image_views,
                graphics_queue: self.graphics_queue,
                present_queue: self.present_queue,
                graphics_queue_family: self.report.graphics_queue_family,
                extent: self.report.extent,
                depth_format: self.depth_format,
                pipelines: &self.terrain_pipelines,
                meshes: &self.terrain_meshes,
                texture_sets: &self.terrain_texture_sets,
            },
            scene_layout,
            scene,
            draws,
        )?;
        self.is_idle = false;
        Ok(report)
    }

    /// Uploads every authored mip from one selected BLP source exactly once.
    ///
    /// Color interpretation is explicit because the BLP container does not
    /// identify which Vulkan transfer function the owning material expects.
    ///
    /// # Errors
    ///
    /// Returns [`BlpTextureUploadError`] when a mip cannot decode or Vulkan
    /// allocation, recording, submission, synchronization, or view creation fails.
    pub fn upload_blp_texture(
        &mut self,
        source: &BlpTextureSource,
        color_space: BlpColorSpace,
    ) -> Result<BlpTextureHandle, BlpTextureUploadError> {
        let allocator = self.allocator.as_ref().ok_or_else(|| {
            VulkanError::operation("access Vulkan allocator", "allocator is unavailable")
        })?;
        self.blp_textures.upload(
            TextureUploadContext {
                device: &self.device,
                allocator,
                graphics_queue: self.graphics_queue,
                graphics_queue_family: self.report.graphics_queue_family,
            },
            source,
            color_space,
        )
    }

    /// Uploads all newly encountered path/color-space identities together.
    ///
    /// Returned handles preserve request order and duplicates. Already resident
    /// identities cause no transfer; all remaining identities share one exact
    /// staging allocation, command buffer, queue submission, and fence wait.
    ///
    /// # Errors
    ///
    /// Returns [`BlpTextureUploadError`] when any source cannot decode or the
    /// all-or-nothing Vulkan admission batch cannot complete.
    pub fn upload_blp_textures(
        &mut self,
        requests: &[BlpTextureUploadRequest<'_>],
    ) -> Result<Vec<BlpTextureHandle>, BlpTextureUploadError> {
        let allocator = self.allocator.as_ref().ok_or_else(|| {
            VulkanError::operation("access Vulkan allocator", "allocator is unavailable")
        })?;
        self.blp_textures.upload_batch(
            TextureUploadContext {
                device: &self.device,
                allocator,
                graphics_queue: self.graphics_queue,
                graphics_queue_family: self.report.graphics_queue_family,
            },
            requests,
        )
    }

    /// Creates or retrieves stock's opaque 8x8 green WMO placeholder.
    ///
    /// This is the recovered MapObj image for a valid empty material stage,
    /// not a general missing-file fallback. Failed authored BLP loads remain
    /// errors before this boundary.
    ///
    /// # Errors
    ///
    /// Returns [`BlpTextureUploadError`] for Vulkan allocation or transfer failure.
    pub fn upload_stock_world_model_green(
        &mut self,
    ) -> Result<BlpTextureHandle, BlpTextureUploadError> {
        let allocator = self.allocator.as_ref().ok_or_else(|| {
            VulkanError::operation("access Vulkan allocator", "allocator is unavailable")
        })?;
        self.blp_textures
            .upload_stock_world_model_green(TextureUploadContext {
                device: &self.device,
                allocator,
                graphics_queue: self.graphics_queue,
                graphics_queue_family: self.report.graphics_queue_family,
            })
    }

    /// Creates or retrieves stock's opaque 8x8 white M2 placeholder.
    ///
    /// M2Shared.cpp `0x0083CC80` selects this generated image when a texture
    /// declaration has no filename.
    ///
    /// # Errors
    ///
    /// Returns [`BlpTextureUploadError`] for Vulkan allocation or transfer failure.
    pub fn upload_stock_m2_white(&mut self) -> Result<BlpTextureHandle, BlpTextureUploadError> {
        let allocator = self.allocator.as_ref().ok_or_else(|| {
            VulkanError::operation("access Vulkan allocator", "allocator is unavailable")
        })?;
        self.blp_textures
            .upload_stock_m2_white(TextureUploadContext {
                device: &self.device,
                allocator,
                graphics_queue: self.graphics_queue,
                graphics_queue_family: self.report.graphics_queue_family,
            })
    }

    /// Creates or retrieves stock's opaque 8x8 green failed-request texture.
    ///
    /// Texture.cpp `0x004B9760` selects this image after a model texture cannot
    /// be opened or decoded.
    ///
    /// # Errors
    ///
    /// Returns [`BlpTextureUploadError`] for Vulkan allocation or transfer failure.
    pub fn upload_stock_m2_failure(&mut self) -> Result<BlpTextureHandle, BlpTextureUploadError> {
        let allocator = self.allocator.as_ref().ok_or_else(|| {
            VulkanError::operation("access Vulkan allocator", "allocator is unavailable")
        })?;
        self.blp_textures
            .upload_stock_m2_failure(TextureUploadContext {
                device: &self.device,
                allocator,
                graphics_queue: self.graphics_queue,
                graphics_queue_family: self.report.graphics_queue_family,
            })
    }

    /// Returns immutable diagnostics for a live renderer-owned BLP image.
    #[must_use]
    pub fn blp_texture_info(&self, handle: BlpTextureHandle) -> Option<&BlpTextureResourceInfo> {
        self.blp_textures.info(handle)
    }

    /// Returns retired queue submissions spent admitting BLP resources.
    #[must_use]
    pub const fn blp_texture_upload_submission_count(&self) -> u64 {
        self.blp_textures.upload_submission_count()
    }

    /// Uploads one complete placement-owned character body atlas.
    ///
    /// The composed texture is sampled as sRGB and deliberately receives no
    /// archive path identity. Callers retain the returned handle with the
    /// placement whose customization produced these pixels.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] when the private atlas invariant is broken,
    /// handle capacity is exhausted, or Vulkan upload cannot complete.
    pub fn upload_character_atlas_texture(
        &mut self,
        atlas: &CharacterAtlasTexture,
    ) -> Result<CharacterAtlasTextureHandle, VulkanError> {
        let allocator = self.allocator.as_ref().ok_or_else(|| {
            VulkanError::operation("access Vulkan allocator", "allocator is unavailable")
        })?;
        self.character_atlas_textures.upload(
            TextureUploadContext {
                device: &self.device,
                allocator,
                graphics_queue: self.graphics_queue,
                graphics_queue_family: self.report.graphics_queue_family,
            },
            atlas,
        )
    }

    /// Returns immutable diagnostics for one live composed body atlas.
    #[must_use]
    pub fn character_atlas_texture_info(
        &self,
        handle: CharacterAtlasTextureHandle,
    ) -> Option<CharacterAtlasTextureResourceInfo> {
        self.character_atlas_textures.info(handle)
    }

    /// Returns retired queue submissions spent admitting dynamic body atlases.
    #[must_use]
    pub const fn character_atlas_upload_submission_count(&self) -> u64 {
        self.character_atlas_textures.upload_submission_count()
    }

    /// Creates or retrieves one exact simple-render source/blend pipeline.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] when shader compilation, layout creation, or
    /// driver pipeline creation fails.
    pub fn prepare_ui_pipeline(
        &mut self,
        source: UiShaderSource,
        blend: UiRenderBlend,
    ) -> Result<UiPipelineHandle, VulkanError> {
        self.ui_pipelines
            .prepare(&self.device, self.color_format, source, blend)
    }

    /// Returns immutable diagnostics for one live UI pipeline.
    #[must_use]
    pub fn ui_pipeline_info(&self, handle: UiPipelineHandle) -> Option<UiPipelineInfo> {
        self.ui_pipelines.info(handle)
    }

    /// Uploads one immutable UI presentation generation to device-local buffers.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for empty geometry, handle exhaustion, allocation,
    /// transfer recording, submission, or synchronization failure.
    pub fn upload_ui_mesh(&mut self, plan: &UiMeshPlan) -> Result<UiMeshHandle, VulkanError> {
        let allocator = self.allocator.as_ref().ok_or_else(|| {
            VulkanError::operation("access Vulkan allocator", "allocator is unavailable")
        })?;
        self.ui_meshes.upload(
            MeshUploadContext {
                device: &self.device,
                allocator,
                graphics_queue: self.graphics_queue,
                graphics_queue_family: self.report.graphics_queue_family,
            },
            plan,
        )
    }

    /// Replaces one stable UI mesh through the next ordered frame submission.
    pub fn replace_ui_mesh(
        &mut self,
        handle: UiMeshHandle,
        plan: &UiMeshPlan,
    ) -> Result<(), VulkanError> {
        let allocator = self.allocator.as_ref().ok_or_else(|| {
            VulkanError::operation("access Vulkan allocator", "allocator is unavailable")
        })?;
        self.ui_meshes.replace(
            MeshUploadContext {
                device: &self.device,
                allocator,
                graphics_queue: self.graphics_queue,
                graphics_queue_family: self.report.graphics_queue_family,
            },
            handle,
            plan,
        )
    }

    /// Returns immutable diagnostics for one live UI mesh generation.
    #[must_use]
    pub fn ui_mesh_info(&self, handle: UiMeshHandle) -> Option<UiMeshResourceInfo> {
        self.ui_meshes.info(handle)
    }

    /// Uploads or retrieves one immutable linear RGBA8 glyph atlas generation.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for malformed byte counts, handle exhaustion,
    /// allocation, recording, submission, or synchronization failure.
    pub fn upload_ui_glyph_texture(
        &mut self,
        identity: u64,
        extent: (u32, u32),
        rgba8: &[u8],
    ) -> Result<UiGlyphTextureHandle, VulkanError> {
        let allocator = self.allocator.as_ref().ok_or_else(|| {
            VulkanError::operation("access Vulkan allocator", "allocator is unavailable")
        })?;
        self.ui_glyph_textures.upload(
            TextureUploadContext {
                device: &self.device,
                allocator,
                graphics_queue: self.graphics_queue,
                graphics_queue_family: self.report.graphics_queue_family,
            },
            identity,
            extent,
            rgba8,
        )
    }

    /// Returns immutable diagnostics for one live glyph atlas.
    #[must_use]
    pub fn ui_glyph_texture_info(
        &self,
        handle: UiGlyphTextureHandle,
    ) -> Option<UiGlyphTextureResourceInfo> {
        self.ui_glyph_textures.info(handle)
    }

    /// Creates or retrieves one exact UI texture-axis sampler.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] when handle capacity is exhausted or the driver
    /// rejects sampler creation.
    pub fn prepare_ui_sampler(
        &mut self,
        info: UiSamplerInfo,
    ) -> Result<UiSamplerHandle, VulkanError> {
        self.ui_samplers.prepare(&self.device, info)
    }

    /// Returns immutable diagnostics for one live UI sampler.
    #[must_use]
    pub fn ui_sampler_info(&self, handle: UiSamplerHandle) -> Option<UiSamplerInfo> {
        self.ui_samplers.info(handle)
    }

    /// Creates persistent UI sampled-image descriptor sets in one exact batch.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for foreign handles, descriptor allocation
    /// failures, or exhausted renderer-local handle space.
    pub fn prepare_ui_texture_sets(
        &mut self,
        requested: &[UiSampledTexture],
    ) -> Result<Vec<UiTextureSetHandle>, VulkanError> {
        let layout = self.ui_pipelines.texture_set_layout(&self.device)?;
        self.ui_texture_sets.prepare(
            &self.device,
            layout,
            &self.blp_textures,
            &self.ui_glyph_textures,
            &self.ui_samplers,
            requested,
        )
    }

    /// Returns immutable diagnostics for one live UI descriptor set.
    #[must_use]
    pub fn ui_texture_set_info(&self, handle: UiTextureSetHandle) -> Option<UiTextureSetInfo> {
        self.ui_texture_sets.info(handle)
    }

    /// Joins one ordered UI batch to compatible renderer-local resources.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for foreign handles, plan skew, invalid ranges,
    /// or source/blend/image/sampler state mismatches.
    pub fn prepare_ui_draw(
        &self,
        mesh: UiMeshHandle,
        pipeline: UiPipelineHandle,
        texture_set: Option<UiTextureSetHandle>,
        plan: &UiMeshPlan,
        batch_index: usize,
    ) -> Result<UiPreparedDraw, VulkanError> {
        prepare_ui_draw(
            &self.ui_meshes,
            &self.ui_pipelines,
            &self.ui_texture_sets,
            &self.blp_textures,
            &self.ui_glyph_textures,
            &self.ui_samplers,
            mesh,
            pipeline,
            texture_set,
            plan,
            batch_index,
        )
    }

    /// Records and presents one complete ordered UI batch list.
    ///
    /// Logical extent is the coordinate space used when the immutable mesh was
    /// prepared. It is independent from the physical swapchain extent so stock
    /// UI geometry scales without rebuilding or special-casing HD textures.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for an empty frame, invalid logical extent,
    /// swapchain resource mismatch, command recording, submission, or present
    /// failure.
    pub fn present_ui(
        &mut self,
        logical_extent: [f32; 2],
        draws: &[UiPreparedDraw],
    ) -> Result<UiFrameReport, VulkanError> {
        self.with_swapchain_retry(|renderer| renderer.present_ui_once(logical_extent, draws))
    }

    /// Clears the current swapchain image to opaque black and presents it.
    ///
    /// This is the explicit handoff surface used between independently loaded
    /// presentation domains, where retaining the previous frame would expose
    /// stale cinematic or loading-screen contents.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for an invalid logical extent, swapchain
    /// acquisition, command recording, submission, or presentation failure.
    pub fn present_clear(&mut self, logical_extent: [f32; 2]) -> Result<(), VulkanError> {
        self.with_swapchain_retry(|renderer| renderer.present_clear_once(logical_extent))
    }

    fn present_clear_once(&mut self, logical_extent: [f32; 2]) -> Result<(), VulkanError> {
        let report = self.ui_frames.present_clear(
            UiFrameContext {
                device: &self.device,
                swapchain_loader: &self.swapchain_loader,
                swapchain: self.swapchain,
                swapchain_images: &self.swapchain_images,
                image_views: &self.image_views,
                graphics_queue: self.graphics_queue,
                present_queue: self.present_queue,
                graphics_queue_family: self.report.graphics_queue_family,
                extent: self.report.extent,
                pipelines: &self.ui_pipelines,
                meshes: &self.ui_meshes,
                texture_sets: &self.ui_texture_sets,
            },
            logical_extent,
        )?;
        debug_assert_eq!(report.draw_count(), 0);
        self.is_idle = false;
        Ok(())
    }

    fn present_ui_once(
        &mut self,
        logical_extent: [f32; 2],
        draws: &[UiPreparedDraw],
    ) -> Result<UiFrameReport, VulkanError> {
        let report = self.ui_frames.present(
            UiFrameContext {
                device: &self.device,
                swapchain_loader: &self.swapchain_loader,
                swapchain: self.swapchain,
                swapchain_images: &self.swapchain_images,
                image_views: &self.image_views,
                graphics_queue: self.graphics_queue,
                present_queue: self.present_queue,
                graphics_queue_family: self.report.graphics_queue_family,
                extent: self.report.extent,
                pipelines: &self.ui_pipelines,
                meshes: &self.ui_meshes,
                texture_sets: &self.ui_texture_sets,
            },
            logical_extent,
            draws,
        )?;
        self.is_idle = false;
        self.report.presented_ui_draw_count = Some(report.draw_count());
        Ok(report)
    }

    /// Creates or retrieves the graphics pipeline for one exact M2 draw state.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] when shader translation, module/layout creation,
    /// or driver pipeline compilation fails.
    pub fn prepare_m2_pipeline(
        &mut self,
        plan: M2ShaderPlan,
        permutation: M2ShaderPermutation,
    ) -> Result<M2PipelineHandle, VulkanError> {
        self.m2_pipelines.prepare(
            &self.device,
            self.pipeline_cache,
            self.color_format,
            self.depth_format,
            plan,
            permutation,
        )
    }

    /// Creates or retrieves an M2 pipeline from worker-compiled SPIR-V.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] when module/layout or driver pipeline creation
    /// fails for the program's validated shader identity.
    pub fn prepare_precompiled_m2_pipeline(
        &mut self,
        program: &M2SpirvProgram,
    ) -> Result<M2PipelineHandle, VulkanError> {
        self.m2_pipelines.prepare_precompiled(
            &self.device,
            self.pipeline_cache,
            self.color_format,
            self.depth_format,
            program,
        )
    }

    /// Returns immutable diagnostics for a live renderer-owned M2 pipeline.
    #[must_use]
    pub fn m2_pipeline_info(&self, handle: M2PipelineHandle) -> Option<&M2PipelineInfo> {
        self.m2_pipelines.info(handle)
    }

    /// Creates or retrieves the stock PNC0T0 pipeline for one particle emitter.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for descriptor ABI creation, pinned shader
    /// compilation, handle exhaustion, or graphics-pipeline creation failure.
    pub fn prepare_m2_particle_pipeline(
        &mut self,
        blending_type: u8,
        particle_flags: u32,
    ) -> Result<M2ParticlePipelineHandle, VulkanError> {
        let scene_set = self.m2_pipelines.frame_set_layouts(&self.device)?[0];
        let texture_set = self.m2_pipelines.texture_set_layout(&self.device)?;
        self.m2_particle_pipelines.prepare(
            &self.device,
            self.pipeline_cache,
            self.color_format,
            self.depth_format,
            scene_set,
            texture_set,
            crate::M2MaterialState::from_particle(blending_type, particle_flags),
        )
    }

    /// Creates or retrieves a particle pipeline from worker-compiled SPIR-V.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] when descriptor/layout or driver pipeline
    /// creation fails for the program's material identity.
    pub fn prepare_precompiled_m2_particle_pipeline(
        &mut self,
        program: &M2ParticleSpirvProgram,
    ) -> Result<M2ParticlePipelineHandle, VulkanError> {
        let scene_set = self.m2_pipelines.frame_set_layouts(&self.device)?[0];
        let texture_set = self.m2_pipelines.texture_set_layout(&self.device)?;
        self.m2_particle_pipelines.prepare_precompiled(
            &self.device,
            self.pipeline_cache,
            self.color_format,
            self.depth_format,
            scene_set,
            texture_set,
            program,
        )
    }

    /// Returns immutable diagnostics for one live particle pipeline.
    #[must_use]
    pub fn m2_particle_pipeline_info(
        &self,
        handle: M2ParticlePipelineHandle,
    ) -> Option<M2ParticlePipelineInfo> {
        self.m2_particle_pipelines.info(handle)
    }

    /// Joins one dynamic ordinary-particle mesh to compatible GPU resources.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for foreign handles, material or texture-set
    /// disagreement, or ranges outside Vulkan's indexed-draw domains.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_m2_particle_draw(
        &self,
        pipeline: M2ParticlePipelineHandle,
        texture_set: M2TextureSetHandle,
        blending_type: u8,
        particle_flags: u32,
        order: M2EffectOrder,
        first_vertex: u32,
        first_index: u32,
        mesh: &M2ParticleMeshPlan,
    ) -> Result<M2ParticlePreparedDraw, VulkanError> {
        self.prepare_m2_particle_draw_range(
            pipeline,
            texture_set,
            blending_type,
            particle_flags,
            order,
            first_vertex,
            first_index,
            mesh.vertices().len(),
            mesh.indices().len(),
        )
    }

    /// Joins one range in retained particle storage to compatible GPU resources.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] under the same conditions as
    /// [`Self::prepare_m2_particle_draw`].
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_m2_particle_draw_range(
        &self,
        pipeline: M2ParticlePipelineHandle,
        texture_set: M2TextureSetHandle,
        blending_type: u8,
        particle_flags: u32,
        order: M2EffectOrder,
        first_vertex: u32,
        first_index: u32,
        vertex_count: usize,
        index_count: usize,
    ) -> Result<M2ParticlePreparedDraw, VulkanError> {
        prepare_particle_draw(
            &self.m2_particle_pipelines,
            &self.m2_texture_sets,
            pipeline,
            texture_set,
            blending_type,
            particle_flags,
            order,
            first_vertex,
            first_index,
            vertex_count,
            index_count,
        )
    }

    /// Creates or retrieves the stock PCT0 ribbon pipeline for one material.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for descriptor ABI creation, pinned shader
    /// compilation, handle exhaustion, or graphics-pipeline creation failure.
    pub fn prepare_m2_ribbon_pipeline(
        &mut self,
        material: M2Material,
    ) -> Result<M2RibbonPipelineHandle, VulkanError> {
        let scene_set = self.m2_pipelines.frame_set_layouts(&self.device)?[0];
        let texture_set = self.m2_pipelines.texture_set_layout(&self.device)?;
        self.m2_ribbon_pipelines.prepare(
            &self.device,
            self.pipeline_cache,
            self.color_format,
            self.depth_format,
            scene_set,
            texture_set,
            crate::M2MaterialState::from_material(material),
        )
    }

    /// Creates or retrieves a ribbon pipeline from worker-compiled SPIR-V.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] when descriptor/layout or driver pipeline
    /// creation fails for the program's material identity.
    pub fn prepare_precompiled_m2_ribbon_pipeline(
        &mut self,
        program: &M2RibbonSpirvProgram,
    ) -> Result<M2RibbonPipelineHandle, VulkanError> {
        let scene_set = self.m2_pipelines.frame_set_layouts(&self.device)?[0];
        let texture_set = self.m2_pipelines.texture_set_layout(&self.device)?;
        self.m2_ribbon_pipelines.prepare_precompiled(
            &self.device,
            self.pipeline_cache,
            self.color_format,
            self.depth_format,
            scene_set,
            texture_set,
            program,
        )
    }

    /// Returns immutable diagnostics for one live ribbon pipeline.
    #[must_use]
    pub fn m2_ribbon_pipeline_info(
        &self,
        handle: M2RibbonPipelineHandle,
    ) -> Option<M2RibbonPipelineInfo> {
        self.m2_ribbon_pipelines.info(handle)
    }

    /// Joins one dynamic ribbon strip to compatible renderer-local resources.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for foreign handles, a material or texture-set
    /// mismatch, or a strip range outside Vulkan's 32-bit vertex domain.
    pub fn prepare_m2_ribbon_draw(
        &self,
        pipeline: M2RibbonPipelineHandle,
        texture_set: M2TextureSetHandle,
        material: M2Material,
        order: M2EffectOrder,
        first_vertex: u32,
        mesh: &M2RibbonMeshPlan,
    ) -> Result<M2RibbonPreparedDraw, VulkanError> {
        prepare_ribbon_draw(
            &self.m2_ribbon_pipelines,
            &self.m2_texture_sets,
            pipeline,
            texture_set,
            material,
            order,
            first_vertex,
            mesh,
        )
    }

    /// Creates or retrieves one exact ordinary/unified WMO surface pipeline.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for a null stock effect slot, shader failure,
    /// handle exhaustion, or Vulkan layout/module/pipeline creation failure.
    pub fn prepare_world_model_pipeline(
        &mut self,
        unified: bool,
        pass: WorldModelSurfacePass,
    ) -> Result<WorldModelPipelineHandle, VulkanError> {
        self.world_model_pipelines.prepare(
            &self.device,
            self.color_format,
            self.depth_format,
            unified,
            pass,
        )
    }

    /// Returns immutable diagnostics for one live WMO graphics pipeline.
    #[must_use]
    pub fn world_model_pipeline_info(
        &self,
        handle: WorldModelPipelineHandle,
    ) -> Option<WorldModelPipelineInfo> {
        self.world_model_pipelines.info(handle)
    }

    /// Creates one WMO sampler from global filtering and MOMT axis clamps.
    ///
    /// The requested anisotropy is capped to the selected adapter exactly as
    /// stock caps its global filter class to device capability.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for handle exhaustion or sampler creation.
    pub fn prepare_world_model_sampler(
        &mut self,
        material: crate::WorldModelMaterialState,
        filtering: WorldModelTextureFiltering,
        base_mip: WorldModelBaseMip,
    ) -> Result<WorldModelSamplerHandle, VulkanError> {
        self.world_model_samplers.prepare(
            &self.device,
            material,
            filtering,
            base_mip,
            self.sampler_anisotropy,
            self.maximum_sampler_anisotropy,
        )
    }

    /// Returns immutable diagnostics for one live WMO sampler.
    #[must_use]
    pub fn world_model_sampler_info(
        &self,
        handle: WorldModelSamplerHandle,
    ) -> Option<WorldModelSamplerInfo> {
        self.world_model_samplers.info(handle)
    }

    /// Creates persistent WMO image/sampler descriptor sets in one exact batch.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for foreign resource handles, exhausted handle
    /// space, or Vulkan layout/pool/allocation failures.
    pub fn prepare_world_model_texture_sets(
        &mut self,
        requested: &[WorldModelTextureSet],
    ) -> Result<Vec<WorldModelTextureSetHandle>, VulkanError> {
        let layout = self
            .world_model_pipelines
            .texture_set_layout(&self.device)?;
        self.world_model_texture_sets.prepare(
            &self.device,
            layout,
            &self.blp_textures,
            &self.world_model_samplers,
            requested,
        )
    }

    /// Returns immutable diagnostics for one live WMO texture descriptor.
    #[must_use]
    pub fn world_model_texture_set_info(
        &self,
        handle: WorldModelTextureSetHandle,
    ) -> Option<WorldModelTextureSetInfo> {
        self.world_model_texture_sets.info(handle)
    }

    /// Joins one logical MOBA pass to matching renderer-local WMO resources.
    ///
    /// The material uniform is formed here from the validated pass, placement,
    /// root ambient, and current DayNight emissive scalar so callers cannot
    /// combine correct handles with unrelated shader constants.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for foreign handles, CPU/GPU generation skew,
    /// invalid draw/pass ranges, or material/pipeline/texture disagreement.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_world_model_draw(
        &self,
        mesh: WorldModelMeshHandle,
        pipeline: WorldModelPipelineHandle,
        texture_set: WorldModelTextureSetHandle,
        plan: &WorldModelMeshPlan,
        draw_index: usize,
        pass_index: usize,
        model: Mat4,
        environment_emissive: f32,
        fog_color: Vec3,
    ) -> Result<WorldModelPreparedDraw, VulkanError> {
        prepare_world_model_draw(
            &self.world_model_meshes,
            &self.world_model_pipelines,
            &self.world_model_texture_sets,
            &self.blp_textures,
            mesh,
            pipeline,
            texture_set,
            plan,
            draw_index,
            pass_index,
            model,
            environment_emissive,
            fog_color,
        )
    }

    /// Presents terrain, physical WMO passes, and M2s in one world framebuffer.
    ///
    /// Exactly one swapchain image is acquired, color/depth are cleared once,
    /// and the completed shared rendering scope is presented once.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for empty input, insufficient M2 bones, missing
    /// shadow resources, frame growth, recording, submission, or presentation.
    #[allow(clippy::too_many_arguments)]
    pub fn present_world_frame(
        &mut self,
        scene: WorldFrameScene,
        bone_transforms: &[Mat4],
        terrain_draws: &[TerrainPreparedDraw],
        world_model_draws: &[WorldModelPreparedDraw],
        m2_draws: &[M2PreparedDraw],
        particle_vertices: &[crate::M2ParticleRenderVertex],
        particle_indices: &[u32],
        particle_draws: &[M2ParticlePreparedDraw],
        ribbon_vertices: &[crate::M2RibbonRenderVertex],
        ribbon_draws: &[M2RibbonPreparedDraw],
    ) -> Result<WorldFrameReport, VulkanError> {
        self.with_swapchain_retry(|renderer| {
            renderer.present_world_frame_internal(
                scene,
                bone_transforms,
                terrain_draws,
                world_model_draws,
                m2_draws,
                particle_vertices,
                particle_indices,
                particle_draws,
                ribbon_vertices,
                ribbon_draws,
                crate::WorldScreenWindow::FULL,
                None,
                None,
            )
        })
    }

    /// Presents the unified model/effect scene followed by a loaded UI pass.
    ///
    /// This is the compositor boundary used by stock Glue `Model` and
    /// `ModelFFX` frames. The overlay shares the acquired image and submission
    /// but begins a separate color-only dynamic-rendering scope.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::present_world_frame`] plus invalid
    /// UI packet references.
    #[allow(clippy::too_many_arguments)]
    pub fn present_world_frame_with_ui(
        &mut self,
        scene: WorldFrameScene,
        bone_transforms: &[Mat4],
        terrain_draws: &[TerrainPreparedDraw],
        world_model_draws: &[WorldModelPreparedDraw],
        m2_draws: &[M2PreparedDraw],
        particle_vertices: &[crate::M2ParticleRenderVertex],
        particle_indices: &[u32],
        particle_draws: &[M2ParticlePreparedDraw],
        ribbon_vertices: &[crate::M2RibbonRenderVertex],
        ribbon_draws: &[M2RibbonPreparedDraw],
        screen_window: crate::WorldScreenWindow,
        ui_logical_extent: [f32; 2],
        ui_draws: &[UiPreparedDraw],
    ) -> Result<WorldFrameReport, VulkanError> {
        self.with_swapchain_retry(|renderer| {
            renderer.present_world_frame_internal(
                scene,
                bone_transforms,
                terrain_draws,
                world_model_draws,
                m2_draws,
                particle_vertices,
                particle_indices,
                particle_draws,
                ribbon_vertices,
                ribbon_draws,
                screen_window,
                Some(WorldUiOverlay {
                    logical_extent: ui_logical_extent,
                    draws: ui_draws,
                }),
                None,
            )
        })
    }

    /// Presents a ModelFFX scene through stock glow/gamma before loaded UI.
    #[allow(clippy::too_many_arguments)]
    pub fn present_world_frame_with_ui_and_glow(
        &mut self,
        scene: WorldFrameScene,
        bone_transforms: &[Mat4],
        terrain_draws: &[TerrainPreparedDraw],
        world_model_draws: &[WorldModelPreparedDraw],
        m2_draws: &[M2PreparedDraw],
        particle_vertices: &[crate::M2ParticleRenderVertex],
        particle_indices: &[u32],
        particle_draws: &[M2ParticlePreparedDraw],
        ribbon_vertices: &[crate::M2RibbonRenderVertex],
        ribbon_draws: &[M2RibbonPreparedDraw],
        screen_window: crate::WorldScreenWindow,
        glow: WorldFrameGlow,
        ui_logical_extent: [f32; 2],
        ui_draws: &[UiPreparedDraw],
    ) -> Result<WorldFrameReport, VulkanError> {
        self.with_swapchain_retry(|renderer| {
            renderer.present_world_frame_internal(
                scene,
                bone_transforms,
                terrain_draws,
                world_model_draws,
                m2_draws,
                particle_vertices,
                particle_indices,
                particle_draws,
                ribbon_vertices,
                ribbon_draws,
                screen_window,
                Some(WorldUiOverlay {
                    logical_extent: ui_logical_extent,
                    draws: ui_draws,
                }),
                Some(glow),
            )
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn present_world_frame_internal(
        &mut self,
        scene: WorldFrameScene,
        bone_transforms: &[Mat4],
        terrain_draws: &[TerrainPreparedDraw],
        world_model_draws: &[WorldModelPreparedDraw],
        m2_draws: &[M2PreparedDraw],
        particle_vertices: &[crate::M2ParticleRenderVertex],
        particle_indices: &[u32],
        particle_draws: &[M2ParticlePreparedDraw],
        ribbon_vertices: &[crate::M2RibbonRenderVertex],
        ribbon_draws: &[M2RibbonPreparedDraw],
        screen_window: crate::WorldScreenWindow,
        ui: Option<WorldUiOverlay<'_>>,
        glow: Option<WorldFrameGlow>,
    ) -> Result<WorldFrameReport, VulkanError> {
        let allocator = self.allocator.as_ref().ok_or_else(|| {
            VulkanError::operation("access Vulkan allocator", "allocator is unavailable")
        })?;
        let terrain_layout = self.terrain_pipelines.scene_set_layout(&self.device)?;
        let world_model_layouts = self.world_model_pipelines.frame_set_layouts(&self.device)?;
        let m2_layouts = self.m2_pipelines.frame_set_layouts(&self.device)?;
        let descriptor_layouts = [
            terrain_layout,
            world_model_layouts[0],
            world_model_layouts[1],
            m2_layouts[0],
            m2_layouts[0],
            m2_layouts[0],
            m2_layouts[1],
            m2_layouts[2],
        ];
        if glow.is_some() {
            self.glow.ensure(
                &self.device,
                allocator,
                self.color_format,
                self.report.extent,
                self.swapchain_images.len(),
            )?;
        }
        let report = self.world_frames.present(
            WorldFrameContext {
                device: &self.device,
                allocator,
                swapchain_loader: &self.swapchain_loader,
                swapchain: self.swapchain,
                swapchain_images: &self.swapchain_images,
                image_views: &self.image_views,
                graphics_queue: self.graphics_queue,
                present_queue: self.present_queue,
                graphics_queue_family: self.report.graphics_queue_family,
                extent: self.report.extent,
                depth_format: self.depth_format,
                uniform_alignment: self.uniform_buffer_alignment,
                storage_alignment: self.storage_buffer_alignment,
                terrain_pipelines: &self.terrain_pipelines,
                terrain_meshes: &self.terrain_meshes,
                terrain_texture_sets: &self.terrain_texture_sets,
                world_model_pipelines: &self.world_model_pipelines,
                world_model_meshes: &self.world_model_meshes,
                world_model_texture_sets: &self.world_model_texture_sets,
                m2_pipelines: &self.m2_pipelines,
                m2_meshes: &self.m2_meshes,
                m2_texture_sets: &self.m2_texture_sets,
                m2_particle_pipelines: &self.m2_particle_pipelines,
                m2_ribbon_pipelines: &self.m2_ribbon_pipelines,
                ui_pipelines: &self.ui_pipelines,
                ui_meshes: &self.ui_meshes,
                ui_texture_sets: &self.ui_texture_sets,
                glow: glow.map(|settings| (&self.glow, settings)),
            },
            descriptor_layouts,
            scene,
            bone_transforms,
            terrain_draws,
            world_model_draws,
            m2_draws,
            particle_vertices,
            particle_indices,
            particle_draws,
            ribbon_vertices,
            ribbon_draws,
            WorldFrameWindow {
                screen: screen_window,
            },
            ui,
        )?;
        self.is_idle = false;
        Ok(report)
    }

    /// Creates or retrieves stock's linear, base-mip M2 sampler state.
    ///
    /// Horizontal and vertical addressing come directly from the texture
    /// declaration. Uploaded image identity remains independent so the same
    /// path can be sampled by multiple materials without another allocation.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] when handle capacity is exhausted or the driver
    /// rejects sampler creation.
    pub fn prepare_m2_sampler(
        &mut self,
        texture: &M2Texture,
    ) -> Result<M2SamplerHandle, VulkanError> {
        self.m2_samplers.prepare(&self.device, texture)
    }

    /// Returns immutable diagnostics for a live renderer-owned M2 sampler.
    #[must_use]
    pub fn m2_sampler_info(&self, handle: M2SamplerHandle) -> Option<M2SamplerInfo> {
        self.m2_samplers.info(handle)
    }

    /// Creates persistent sampled-image descriptor sets in one exact-size batch.
    ///
    /// Existing sets are shared by their image/sampler stage identity. A single
    /// descriptor pool is created for only the unique new sets in this call,
    /// matching a decoded model's known material count without a guessed limit.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for foreign resource handles, handle exhaustion,
    /// descriptor-layout creation, pool creation, or set allocation failure.
    pub fn prepare_m2_texture_sets(
        &mut self,
        requested: &[M2TextureSet],
    ) -> Result<Vec<M2TextureSetHandle>, VulkanError> {
        let layout = self.m2_pipelines.texture_set_layout(&self.device)?;
        self.m2_texture_sets.prepare(
            &self.device,
            layout,
            &self.blp_textures,
            &self.character_atlas_textures,
            &self.m2_samplers,
            requested,
        )
    }

    /// Returns immutable diagnostics for a live texture descriptor set.
    #[must_use]
    pub fn m2_texture_set_info(&self, handle: M2TextureSetHandle) -> Option<M2TextureSetInfo> {
        self.m2_texture_sets.info(handle)
    }

    /// Joins one CPU material batch to compatible renderer-local resources.
    ///
    /// The returned packet is the only M2 draw form accepted by frame command
    /// recording, preventing a mesh, pipeline, texture set, or index span from
    /// being combined across unrelated models.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] when any handle is foreign, the draw index/range
    /// is invalid, or fixed pipeline/texture state disagrees with the CPU plan.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_m2_draw(
        &self,
        mesh: M2MeshHandle,
        pipeline: M2PipelineHandle,
        texture_set: M2TextureSetHandle,
        plan: &M2MeshPlan,
        draw_index: usize,
        runtime_alpha_fade: bool,
        material: M2MaterialUniform,
        bone_transform_offset: u32,
        flags: u32,
    ) -> Result<M2PreparedDraw, VulkanError> {
        prepare_draw(
            &self.m2_meshes,
            &self.m2_pipelines,
            &self.m2_texture_sets,
            mesh,
            pipeline,
            texture_set,
            plan,
            draw_index,
            runtime_alpha_fade,
            material,
            bone_transform_offset,
            flags,
        )
    }

    /// Records and presents one asset-backed M2 scene using reusable frame slots.
    ///
    /// Resources grow to the submitted high-water draw/bone counts and are then
    /// reused without steady-state host or Vulkan allocation. One independently
    /// fenced slot exists for each swapchain image.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] for an empty frame, insufficient bone transforms,
    /// resource growth/mapping, command recording, submission, or presentation.
    pub fn present_m2(
        &mut self,
        scene: M2SceneUniform,
        bone_transforms: &[Mat4],
        draws: &[M2PreparedDraw],
    ) -> Result<M2FrameReport, VulkanError> {
        self.with_swapchain_retry(|renderer| {
            renderer.present_m2_once(scene, bone_transforms, draws)
        })
    }

    fn present_m2_once(
        &mut self,
        scene: M2SceneUniform,
        bone_transforms: &[Mat4],
        draws: &[M2PreparedDraw],
    ) -> Result<M2FrameReport, VulkanError> {
        let allocator = self.allocator.as_ref().ok_or_else(|| {
            VulkanError::operation("access Vulkan allocator", "allocator is unavailable")
        })?;
        let frame_layouts = self.m2_pipelines.frame_set_layouts(&self.device)?;
        let report = self.m2_frames.present(
            M2FrameContext {
                device: &self.device,
                allocator,
                swapchain_loader: &self.swapchain_loader,
                swapchain: self.swapchain,
                swapchain_images: &self.swapchain_images,
                image_views: &self.image_views,
                graphics_queue: self.graphics_queue,
                present_queue: self.present_queue,
                graphics_queue_family: self.report.graphics_queue_family,
                extent: self.report.extent,
                depth_format: self.depth_format,
                uniform_alignment: self.uniform_buffer_alignment,
                storage_alignment: self.storage_buffer_alignment,
                pipelines: &self.m2_pipelines,
                meshes: &self.m2_meshes,
                texture_sets: &self.m2_texture_sets,
            },
            frame_layouts,
            scene,
            bone_transforms,
            draws,
        )?;
        self.is_idle = false;
        Ok(report)
    }

    /// Creates the swapchain and one owned color view for each borrowed image.
    fn create_swapchain(
        &mut self,
        selected: &SelectedAdapter,
        extent: vk::Extent2D,
    ) -> Result<(), VulkanError> {
        let image_count = swapchain_image_count(selected.surface_capabilities);
        let queue_families = [selected.graphics_family, selected.present_family];
        let mut create_info = vk::SwapchainCreateInfoKHR::default()
            .surface(self.bootstrap.surface)
            .min_image_count(image_count)
            .image_format(selected.surface_format.format)
            .image_color_space(selected.surface_format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(
                vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | vk::ImageUsageFlags::TRANSFER_DST
                    | vk::ImageUsageFlags::TRANSFER_SRC,
            )
            .pre_transform(selected.surface_capabilities.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(selected.present_mode)
            .clipped(true);
        if selected.graphics_family == selected.present_family {
            create_info = create_info.image_sharing_mode(vk::SharingMode::EXCLUSIVE);
        } else {
            create_info = create_info
                .image_sharing_mode(vk::SharingMode::CONCURRENT)
                .queue_family_indices(&queue_families);
        }
        // SAFETY: All handles and slices in the create info belong to this
        // owner and remain alive for the duration of the call.
        self.swapchain = unsafe { self.swapchain_loader.create_swapchain(&create_info, None) }
            .map_err(|source| VulkanError::operation("create swapchain", source))?;
        // SAFETY: `self.swapchain` was created successfully by this loader.
        let images = unsafe { self.swapchain_loader.get_swapchain_images(self.swapchain) }
            .map_err(|source| VulkanError::operation("enumerate swapchain images", source))?;
        self.image_views.reserve(images.len());
        for &image in &images {
            let subresource_range = vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(1);
            let view_info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(selected.surface_format.format)
                .components(vk::ComponentMapping::default())
                .subresource_range(subresource_range);
            // SAFETY: The swapchain image is live, the format matches its
            // creation format, and the view range addresses its sole mip/layer.
            let view = unsafe { self.device.create_image_view(&view_info, None) }
                .map_err(|source| VulkanError::operation("create swapchain image view", source))?;
            self.image_views.push(view);
        }
        self.swapchain_images = images;
        self.report.swapchain_image_count = self.image_views.len();
        Ok(())
    }

    /// Establishes the VMA owner before any device-local client resources exist.
    fn create_allocator(&mut self, physical_device: vk::PhysicalDevice) -> Result<(), VulkanError> {
        let mut create_info = vk_mem::AllocatorCreateInfo::new(
            &self.bootstrap.instance,
            &self.device,
            physical_device,
        );
        create_info.vulkan_api_version = vk::API_VERSION_1_3;
        // SAFETY: Instance, device, and physical device form the validated live
        // Vulkan object graph and outlive the stored allocator.
        let allocator = unsafe { vk_mem::Allocator::new(create_info) }
            .map_err(|source| VulkanError::operation("create Vulkan allocator", source))?;
        self.allocator = Some(allocator);
        Ok(())
    }

    fn replace_pipeline_cache(&mut self, initial_data: &[u8]) -> Result<(), VulkanError> {
        let info = vk::PipelineCacheCreateInfo::default().initial_data(initial_data);
        // SAFETY: The initial byte slice remains live for the duration of the
        // call and Vulkan validates its implementation-specific cache header.
        let replacement = unsafe { self.device.create_pipeline_cache(&info, None) }
            .map_err(|source| VulkanError::operation("create Vulkan pipeline cache", source))?;
        // SAFETY: The old cache belongs to this device and no pipeline-create
        // call can overlap this exclusive renderer mutation.
        unsafe {
            if self.pipeline_cache != vk::PipelineCache::null() {
                self.device
                    .destroy_pipeline_cache(self.pipeline_cache, None);
            }
        }
        self.pipeline_cache = replacement;
        Ok(())
    }

    /// Performs the one fallible teardown synchronization step idempotently.
    fn wait_idle(&mut self) -> Result<(), VulkanError> {
        if self.is_idle {
            return Ok(());
        }
        // SAFETY: The device is live and exclusively owned. Waiting does not
        // invalidate any handle and is permitted before teardown.
        unsafe { self.device.device_wait_idle() }
            .map_err(|source| VulkanError::operation("wait for device idle", source))?;
        self.is_idle = true;
        Ok(())
    }
}

impl Drop for VulkanRenderer {
    /// Releases Vulkan children in reverse dependency order.
    fn drop(&mut self) {
        let _idle_result = self.wait_idle();
        let _pipeline_cache_result = self.save_pipeline_cache();
        self.ui_frames.destroy(&self.device);
        if let Some(allocator) = self.allocator.as_ref() {
            self.cinematic_frames.destroy(&self.device, allocator);
            self.world_frames.destroy(&self.device, allocator);
            self.glow.destroy(&self.device, allocator);
            self.terrain_frames.destroy(&self.device, allocator);
            self.m2_frames.destroy(&self.device, allocator);
            self.ui_meshes.destroy(allocator);
            self.ui_texture_sets.destroy(&self.device);
            self.world_model_texture_sets.destroy(&self.device);
            self.m2_texture_sets.destroy(&self.device);
            self.character_atlas_textures
                .destroy(&self.device, allocator);
            self.ui_glyph_textures.destroy(&self.device, allocator);
            self.blp_textures.destroy(&self.device, allocator);
            self.terrain_materials.destroy(&self.device, allocator);
            self.terrain_meshes.destroy(allocator);
            self.world_model_meshes.destroy(allocator);
            self.m2_meshes.destroy(allocator);
        }
        self.terrain_texture_sets.destroy(&self.device);
        self.world_model_samplers.destroy(&self.device);
        self.m2_samplers.destroy(&self.device);
        self.ui_samplers.destroy(&self.device);
        self.ui_pipelines.destroy(&self.device);
        self.terrain_pipelines.destroy(&self.device);
        self.world_model_pipelines.destroy(&self.device);
        self.m2_particle_pipelines.destroy(&self.device);
        self.m2_ribbon_pipelines.destroy(&self.device);
        self.m2_pipelines.destroy(&self.device);
        // SAFETY: Every handle was created by this device/loader and this owner
        // destroys each exactly once after attempting to idle the device.
        unsafe {
            for image_view in self.image_views.drain(..).rev() {
                self.device.destroy_image_view(image_view, None);
            }
            if self.swapchain != vk::SwapchainKHR::null() {
                self.swapchain_loader
                    .destroy_swapchain(self.swapchain, None);
                self.swapchain = vk::SwapchainKHR::null();
            }
            // VMA owns no Vulkan handles after all future allocated resources
            // have been destroyed; dropping it before the device is mandatory.
            drop(self.allocator.take());
            if self.pipeline_cache != vk::PipelineCache::null() {
                self.device
                    .destroy_pipeline_cache(self.pipeline_cache, None);
                self.pipeline_cache = vk::PipelineCache::null();
            }
            self.device.destroy_device(None);
        }
    }
}

/// Creates one queue per distinct family and enables required Vulkan 1.3 features.
fn create_device(
    bootstrap: &VulkanBootstrap,
    selected: &SelectedAdapter,
) -> Result<Device, VulkanError> {
    let priority = [1.0_f32];
    let mut family_indices = vec![selected.graphics_family];
    if selected.present_family != selected.graphics_family {
        family_indices.push(selected.present_family);
    }
    let queue_infos = family_indices
        .iter()
        .map(|family| {
            vk::DeviceQueueCreateInfo::default()
                .queue_family_index(*family)
                .queue_priorities(&priority)
        })
        .collect::<Vec<_>>();
    let extension_names = [ash::khr::swapchain::NAME.as_ptr()];
    let mut vulkan13 = vk::PhysicalDeviceVulkan13Features::default()
        .dynamic_rendering(true)
        .synchronization2(true);
    let enabled_features = vk::PhysicalDeviceFeatures::default()
        .sampler_anisotropy(selected.sampler_anisotropy)
        .texture_compression_bc(true);
    let create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_infos)
        .enabled_extension_names(&extension_names)
        .enabled_features(&enabled_features)
        .push_next(&mut vulkan13);
    // SAFETY: Queue families and features were queried from this physical
    // device, and all create-info slices remain alive for the call.
    unsafe {
        bootstrap
            .instance
            .create_device(selected.physical_device, &create_info, None)
    }
    .map_err(|source| VulkanError::operation("create logical device", source))
}

/// Converts the surface's fixed or variable extent into a legal swapchain size.
fn choose_extent(capabilities: vk::SurfaceCapabilitiesKHR, requested: (u32, u32)) -> vk::Extent2D {
    if capabilities.current_extent.width != u32::MAX {
        return capabilities.current_extent;
    }
    vk::Extent2D {
        width: requested.0.clamp(
            capabilities.min_image_extent.width,
            capabilities.max_image_extent.width,
        ),
        height: requested.1.clamp(
            capabilities.min_image_extent.height,
            capabilities.max_image_extent.height,
        ),
    }
}

/// Requests one image beyond the surface minimum without exceeding its maximum.
fn swapchain_image_count(capabilities: vk::SurfaceCapabilitiesKHR) -> u32 {
    let preferred = capabilities.min_image_count.saturating_add(1);
    if capabilities.max_image_count == 0 {
        preferred
    } else {
        preferred.min(capabilities.max_image_count)
    }
}
