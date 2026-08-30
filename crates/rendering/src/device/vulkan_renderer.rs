//! Logical device, queues, swapchain, and image-view lifetime ownership.

#![allow(unsafe_code)]

use ash::{Device, vk};
use solarity_asset::{BlpTextureSource, DecodedBlpTexture, M2Texture};

use crate::device::vulkan_frame::{FrameContext, present_blp};
use crate::device::vulkan_m2_draw::{M2PreparedDraw, prepare_draw};
use crate::device::vulkan_m2_frame::{M2FrameContext, M2FrameRenderer, M2FrameReport};
use crate::device::vulkan_m2_pipeline::{M2PipelineHandle, M2PipelineInfo, M2PipelineRegistry};
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
    BlpTextureUploadError, TextureUploadContext,
};
use crate::device::vulkan_ui_draw::{UiPreparedDraw, prepare_draw as prepare_ui_draw};
use crate::device::vulkan_ui_frame::{UiFrameContext, UiFrameRenderer, UiFrameReport};
use crate::device::vulkan_ui_mesh::{UiMeshHandle, UiMeshRegistry, UiMeshResourceInfo};
use crate::device::vulkan_ui_pipeline::{UiPipelineHandle, UiPipelineInfo, UiPipelineRegistry};
use crate::device::vulkan_ui_sampler::{UiSamplerHandle, UiSamplerInfo, UiSamplerRegistry};
use crate::device::vulkan_ui_texture_set::{
    UiSampledTexture, UiTextureSetHandle, UiTextureSetInfo, UiTextureSetRegistry,
};
use crate::device::{VulkanBootstrap, VulkanError};
use crate::model::M2SceneUniform;
use crate::model::{M2MaterialUniform, M2MeshPlan};
use crate::shader::{M2ShaderPermutation, M2ShaderPlan, TerrainLayerCount};
use crate::{TerrainTileMeshPlan, UiMeshPlan, UiRenderBlend, UiShaderSource};
use glam::Mat4;

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
    device: Device,
    allocator: Option<vk_mem::Allocator>,
    m2_pipelines: M2PipelineRegistry,
    m2_frames: M2FrameRenderer,
    m2_meshes: M2MeshRegistry,
    terrain_meshes: TerrainMeshRegistry,
    terrain_materials: TerrainMaterialRegistry,
    terrain_pipelines: TerrainPipelineRegistry,
    terrain_texture_sets: TerrainTextureSetRegistry,
    m2_samplers: M2SamplerRegistry,
    m2_texture_sets: M2TextureSetRegistry,
    ui_pipelines: UiPipelineRegistry,
    ui_frames: UiFrameRenderer,
    ui_meshes: UiMeshRegistry,
    ui_samplers: UiSamplerRegistry,
    ui_texture_sets: UiTextureSetRegistry,
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
}

impl VulkanRenderer {
    /// Completes device and swapchain initialization for the attached surface.
    pub(super) fn start(
        bootstrap: VulkanBootstrap,
        requested_extent: (u32, u32),
        adapter_index: usize,
    ) -> Result<Self, VulkanError> {
        let selected = SelectedAdapter::select(&bootstrap, adapter_index)?;
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
            device,
            allocator: None,
            m2_pipelines: M2PipelineRegistry::default(),
            m2_frames: M2FrameRenderer::default(),
            m2_meshes: M2MeshRegistry::default(),
            terrain_meshes: TerrainMeshRegistry::default(),
            terrain_materials: TerrainMaterialRegistry::default(),
            terrain_pipelines: TerrainPipelineRegistry::default(),
            terrain_texture_sets: TerrainTextureSetRegistry::default(),
            m2_samplers: M2SamplerRegistry::default(),
            m2_texture_sets: M2TextureSetRegistry::default(),
            ui_pipelines: UiPipelineRegistry::default(),
            ui_frames: UiFrameRenderer::default(),
            ui_meshes: UiMeshRegistry::default(),
            ui_samplers: UiSamplerRegistry::default(),
            ui_texture_sets: UiTextureSetRegistry::default(),
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
                presented_ui_draw_count: None,
            },
            is_idle: false,
            uniform_buffer_alignment: selected.uniform_buffer_alignment,
            storage_buffer_alignment: selected.storage_buffer_alignment,
        };
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
        self.wait_idle()
    }

    /// Uploads and presents one decoded stock BLP before the window is revealed.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] when frame composition, staging, command
    /// submission, synchronization, or presentation fails.
    pub fn present_blp(&mut self, texture: &DecodedBlpTexture) -> Result<(), VulkanError> {
        let allocator = self.allocator.as_ref().ok_or_else(|| {
            VulkanError::operation("access Vulkan allocator", "allocator is unavailable")
        })?;
        present_blp(FrameContext {
            device: &self.device,
            allocator,
            swapchain_loader: &self.swapchain_loader,
            swapchain: self.swapchain,
            swapchain_images: &self.swapchain_images,
            graphics_queue: self.graphics_queue,
            present_queue: self.present_queue,
            graphics_queue_family: self.report.graphics_queue_family,
            frame_extent: self.report.extent,
            texture,
        })?;
        self.report.presented_texture_extent = Some((texture.width(), texture.height()));
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

    /// Returns immutable diagnostics for a live renderer-owned BLP image.
    #[must_use]
    pub fn blp_texture_info(&self, handle: BlpTextureHandle) -> Option<&BlpTextureResourceInfo> {
        self.blp_textures.info(handle)
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

    /// Returns immutable diagnostics for one live UI mesh generation.
    #[must_use]
    pub fn ui_mesh_info(&self, handle: UiMeshHandle) -> Option<UiMeshResourceInfo> {
        self.ui_meshes.info(handle)
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
            self.color_format,
            self.depth_format,
            plan,
            permutation,
        )
    }

    /// Returns immutable diagnostics for a live renderer-owned M2 pipeline.
    #[must_use]
    pub fn m2_pipeline_info(&self, handle: M2PipelineHandle) -> Option<&M2PipelineInfo> {
        self.m2_pipelines.info(handle)
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
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST)
            .pre_transform(selected.surface_capabilities.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(vk::PresentModeKHR::FIFO)
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
        self.ui_frames.destroy(&self.device);
        self.terrain_texture_sets.destroy(&self.device);
        if let Some(allocator) = self.allocator.as_ref() {
            self.m2_frames.destroy(&self.device, allocator);
            self.ui_meshes.destroy(allocator);
            self.ui_texture_sets.destroy(&self.device);
            self.m2_texture_sets.destroy(&self.device);
            self.blp_textures.destroy(&self.device, allocator);
            self.terrain_materials.destroy(&self.device, allocator);
            self.terrain_meshes.destroy(allocator);
            self.m2_meshes.destroy(allocator);
        }
        self.m2_samplers.destroy(&self.device);
        self.ui_samplers.destroy(&self.device);
        self.ui_pipelines.destroy(&self.device);
        self.terrain_pipelines.destroy(&self.device);
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
    let create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_infos)
        .enabled_extension_names(&extension_names)
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
