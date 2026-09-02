//! Swapchain-local image chain for stock FFXBox4, FFXGauss4, and FFXGlow.

#![allow(unsafe_code)]

use ash::{Device, vk};
use vk_mem::Alloc;

use crate::device::VulkanError;
use crate::{GlowShaderPass, GlowSpirvCompiler, GlowSpirvProgram, WorldScreenWindow};

/// Validated stock FFXGlow factor and display-gamma pair for one world frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldFrameGlow {
    strength: f32,
    gamma: f32,
}

impl WorldFrameGlow {
    /// Validates and retains the live FFXGlow and display-gamma inputs.
    pub fn new(strength: f32, gamma: f32) -> Result<Self, VulkanError> {
        if !strength.is_finite() || !(0.0..=4.0).contains(&strength) {
            return Err(VulkanError::GlowStrength { strength });
        }
        if !gamma.is_finite() || !(0.1..=4.0).contains(&gamma) {
            return Err(VulkanError::GlowGamma { gamma });
        }
        Ok(Self { strength, gamma })
    }

    pub(super) const fn strength(self) -> f32 {
        self.strength
    }

    pub(super) const fn gamma(self) -> f32 {
        self.gamma
    }
}

struct GlowImage {
    image: vk::Image,
    allocation: Option<vk_mem::Allocation>,
    view: vk::ImageView,
}

impl GlowImage {
    const fn empty() -> Self {
        Self {
            image: vk::Image::null(),
            allocation: None,
            view: vk::ImageView::null(),
        }
    }

    fn create(
        device: &Device,
        allocator: &vk_mem::Allocator,
        format: vk::Format,
        extent: vk::Extent2D,
        usage: vk::ImageUsageFlags,
    ) -> Result<Self, VulkanError> {
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width: extent.width,
                height: extent.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let allocation_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::AutoPreferDevice,
            required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
            ..Default::default()
        };
        // SAFETY: VMA creates and binds one allocation to this exact image.
        let (image, allocation) = unsafe { allocator.create_image(&image_info, &allocation_info) }
            .map_err(|source| VulkanError::operation("create glow image", source))?;
        let range = color_range();
        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(range);
        // SAFETY: The image and its sole color subresource are live.
        let view = match unsafe { device.create_image_view(&view_info, None) } {
            Ok(view) => view,
            Err(source) => {
                let mut allocation = allocation;
                // SAFETY: No view or command references the failed image.
                unsafe { allocator.destroy_image(image, &mut allocation) };
                return Err(VulkanError::operation("create glow image view", source));
            }
        };
        Ok(Self {
            image,
            allocation: Some(allocation),
            view,
        })
    }

    fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        // SAFETY: The device is idle and this owner releases the view first.
        unsafe {
            if self.view != vk::ImageView::null() {
                device.destroy_image_view(self.view, None);
                self.view = vk::ImageView::null();
            }
            if let Some(mut allocation) = self.allocation.take() {
                allocator.destroy_image(self.image, &mut allocation);
                self.image = vk::Image::null();
            }
        }
    }
}

struct GlowSlot {
    scene: GlowImage,
    box_target: GlowImage,
    horizontal: GlowImage,
    vertical: GlowImage,
    sets: [vk::DescriptorSet; 4],
}

impl GlowSlot {
    const fn empty() -> Self {
        Self {
            scene: GlowImage::empty(),
            box_target: GlowImage::empty(),
            horizontal: GlowImage::empty(),
            vertical: GlowImage::empty(),
            sets: [vk::DescriptorSet::null(); 4],
        }
    }

    fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        self.vertical.destroy(device, allocator);
        self.horizontal.destroy(device, allocator);
        self.box_target.destroy(device, allocator);
        self.scene.destroy(device, allocator);
    }
}

/// Lazily allocated post-process state shaped to the current swapchain.
#[derive(Default)]
pub(in crate::device) struct VulkanGlowRenderer {
    extent: Option<vk::Extent2D>,
    format: vk::Format,
    set_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    sampler: vk::Sampler,
    descriptor_pool: vk::DescriptorPool,
    composite: vk::Pipeline,
    blur: vk::Pipeline,
    box_filter: vk::Pipeline,
    slots: Vec<GlowSlot>,
}

impl VulkanGlowRenderer {
    pub(in crate::device) fn ensure(
        &mut self,
        device: &Device,
        allocator: &vk_mem::Allocator,
        format: vk::Format,
        extent: (u32, u32),
        slot_count: usize,
    ) -> Result<(), VulkanError> {
        let extent = vk::Extent2D {
            width: extent.0,
            height: extent.1,
        };
        if self.extent == Some(extent) && self.format == format && self.slots.len() == slot_count {
            return Ok(());
        }
        self.destroy(device, allocator);
        let result = self.create(device, allocator, format, extent, slot_count);
        if let Err(error) = result {
            self.destroy(device, allocator);
            return Err(error);
        }
        Ok(())
    }

    fn create(
        &mut self,
        device: &Device,
        allocator: &vk_mem::Allocator,
        format: vk::Format,
        extent: vk::Extent2D,
        slot_count: usize,
    ) -> Result<(), VulkanError> {
        if extent.width == 0 || extent.height == 0 || slot_count == 0 {
            return Err(VulkanError::operation(
                "create glow resources",
                "empty swapchain",
            ));
        }
        let bindings = [0, 1].map(|binding| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(binding)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)
        });
        let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
        // SAFETY: Binding storage remains live for the call.
        self.set_layout = unsafe { device.create_descriptor_set_layout(&info, None) }
            .map_err(|source| VulkanError::operation("create glow descriptor layout", source))?;
        let sets = [self.set_layout];
        let push = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .size(16)];
        let info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&sets)
            .push_constant_ranges(&push);
        // SAFETY: Descriptor layout and slices remain live.
        self.pipeline_layout = unsafe { device.create_pipeline_layout(&info, None) }
            .map_err(|source| VulkanError::operation("create glow pipeline layout", source))?;
        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .max_lod(0.0);
        // SAFETY: The sampler has no external pointers.
        self.sampler = unsafe { device.create_sampler(&sampler_info, None) }
            .map_err(|source| VulkanError::operation("create glow sampler", source))?;
        let compiler = GlowSpirvCompiler::new().map_err(glow_shader_error)?;
        self.composite = create_pipeline(
            device,
            self.pipeline_layout,
            format,
            &compiler
                .compile(GlowShaderPass::Composite)
                .map_err(glow_shader_error)?,
        )?;
        self.blur = create_pipeline(
            device,
            self.pipeline_layout,
            format,
            &compiler
                .compile(GlowShaderPass::Blur)
                .map_err(glow_shader_error)?,
        )?;
        self.box_filter = create_pipeline(
            device,
            self.pipeline_layout,
            format,
            &compiler
                .compile(GlowShaderPass::Box)
                .map_err(glow_shader_error)?,
        )?;
        let set_count = u32::try_from(slot_count.saturating_mul(4)).map_err(|_source| {
            VulkanError::operation("create glow descriptors", "too many slots")
        })?;
        let pool_sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(set_count.saturating_mul(2))];
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(set_count)
            .pool_sizes(&pool_sizes);
        // SAFETY: Counts cover two bindings for every allocated set.
        self.descriptor_pool = unsafe { device.create_descriptor_pool(&pool_info, None) }
            .map_err(|source| VulkanError::operation("create glow descriptor pool", source))?;
        let quarter = vk::Extent2D {
            width: extent.width.div_ceil(4).max(1),
            height: extent.height.div_ceil(4).max(1),
        };
        for _ in 0..slot_count {
            let mut slot = GlowSlot::empty();
            let result = (|| {
                slot.scene = GlowImage::create(
                    device,
                    allocator,
                    format,
                    extent,
                    vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
                )?;
                let target_usage =
                    vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
                slot.box_target =
                    GlowImage::create(device, allocator, format, quarter, target_usage)?;
                slot.horizontal =
                    GlowImage::create(device, allocator, format, quarter, target_usage)?;
                slot.vertical =
                    GlowImage::create(device, allocator, format, quarter, target_usage)?;
                let layouts = [self.set_layout; 4];
                let info = vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(self.descriptor_pool)
                    .set_layouts(&layouts);
                // SAFETY: Pool capacity and all layouts are valid.
                let allocated =
                    unsafe { device.allocate_descriptor_sets(&info) }.map_err(|source| {
                        VulkanError::operation("allocate glow descriptors", source)
                    })?;
                slot.sets.copy_from_slice(&allocated);
                update_set(
                    device,
                    slot.sets[0],
                    self.sampler,
                    slot.scene.view,
                    slot.scene.view,
                );
                update_set(
                    device,
                    slot.sets[1],
                    self.sampler,
                    slot.box_target.view,
                    slot.box_target.view,
                );
                update_set(
                    device,
                    slot.sets[2],
                    self.sampler,
                    slot.horizontal.view,
                    slot.horizontal.view,
                );
                update_set(
                    device,
                    slot.sets[3],
                    self.sampler,
                    slot.scene.view,
                    slot.vertical.view,
                );
                Ok(())
            })();
            if let Err(error) = result {
                slot.destroy(device, allocator);
                return Err(error);
            }
            self.slots.push(slot);
        }
        self.extent = Some(extent);
        self.format = format;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::device) fn record(
        &self,
        device: &Device,
        command_buffer: vk::CommandBuffer,
        swapchain_image: vk::Image,
        swapchain_view: vk::ImageView,
        image_index: u32,
        window: WorldScreenWindow,
        glow: WorldFrameGlow,
    ) -> Result<(), VulkanError> {
        let slot = self
            .slots
            .get(image_index as usize)
            .ok_or(VulkanError::WorldFrameCapacity)?;
        let extent = self.extent.ok_or_else(|| {
            VulkanError::operation("record glow pass", "resources are unavailable")
        })?;
        transition_swapchain_to_copy(device, command_buffer, swapchain_image);
        transition_image(
            device,
            command_buffer,
            slot.scene.image,
            vk::PipelineStageFlags2::NONE,
            vk::AccessFlags2::NONE,
            vk::ImageLayout::UNDEFINED,
            vk::PipelineStageFlags2::TRANSFER,
            vk::AccessFlags2::TRANSFER_WRITE,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );
        let copy = [vk::ImageCopy::default()
            .src_subresource(color_layers())
            .dst_subresource(color_layers())
            .extent(vk::Extent3D {
                width: extent.width,
                height: extent.height,
                depth: 1,
            })];
        // SAFETY: Both images are in transfer layouts and cover the same extent.
        unsafe {
            device.cmd_copy_image(
                command_buffer,
                swapchain_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                slot.scene.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &copy,
            )
        };
        transition_image(
            device,
            command_buffer,
            slot.scene.image,
            vk::PipelineStageFlags2::TRANSFER,
            vk::AccessFlags2::TRANSFER_WRITE,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::PipelineStageFlags2::FRAGMENT_SHADER,
            vk::AccessFlags2::SHADER_SAMPLED_READ,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        );
        transition_image(
            device,
            command_buffer,
            swapchain_image,
            vk::PipelineStageFlags2::TRANSFER,
            vk::AccessFlags2::TRANSFER_READ,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::COLOR_ATTACHMENT_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );
        let quarter = vk::Extent2D {
            width: extent.width.div_ceil(4).max(1),
            height: extent.height.div_ceil(4).max(1),
        };
        self.record_target(
            device,
            command_buffer,
            &slot.box_target,
            quarter,
            self.box_filter,
            slot.sets[0],
            [
                1.0 / extent.width as f32,
                1.0 / extent.height as f32,
                0.0,
                0.0,
            ],
        );
        self.record_target(
            device,
            command_buffer,
            &slot.horizontal,
            quarter,
            self.blur,
            slot.sets[1],
            [
                1.0 / quarter.width as f32,
                1.0 / quarter.height as f32,
                1.0,
                0.0,
            ],
        );
        self.record_target(
            device,
            command_buffer,
            &slot.vertical,
            quarter,
            self.blur,
            slot.sets[2],
            [
                1.0 / quarter.width as f32,
                1.0 / quarter.height as f32,
                0.0,
                1.0,
            ],
        );
        begin_color_target(
            device,
            command_buffer,
            swapchain_view,
            extent,
            vk::AttachmentLoadOp::LOAD,
        );
        set_viewport_scissor(device, command_buffer, extent, Some(window));
        bind_and_draw(
            device,
            command_buffer,
            self.composite,
            self.pipeline_layout,
            slot.sets[3],
            [glow.strength(), glow.gamma(), 0.0, 0.0],
        );
        // SAFETY: Composite rendering scope is active.
        unsafe { device.cmd_end_rendering(command_buffer) };
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn record_target(
        &self,
        device: &Device,
        command_buffer: vk::CommandBuffer,
        target: &GlowImage,
        extent: vk::Extent2D,
        pipeline: vk::Pipeline,
        set: vk::DescriptorSet,
        parameters: [f32; 4],
    ) {
        transition_image(
            device,
            command_buffer,
            target.image,
            vk::PipelineStageFlags2::NONE,
            vk::AccessFlags2::NONE,
            vk::ImageLayout::UNDEFINED,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );
        begin_color_target(
            device,
            command_buffer,
            target.view,
            extent,
            vk::AttachmentLoadOp::CLEAR,
        );
        set_viewport_scissor(device, command_buffer, extent, None);
        bind_and_draw(
            device,
            command_buffer,
            pipeline,
            self.pipeline_layout,
            set,
            parameters,
        );
        // SAFETY: Target rendering scope is active.
        unsafe { device.cmd_end_rendering(command_buffer) };
        transition_image(
            device,
            command_buffer,
            target.image,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::PipelineStageFlags2::FRAGMENT_SHADER,
            vk::AccessFlags2::SHADER_SAMPLED_READ,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        );
    }

    pub(in crate::device) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        for slot in self.slots.iter_mut().rev() {
            slot.destroy(device, allocator);
        }
        self.slots.clear();
        // SAFETY: The device is idle and all dependent image views are gone.
        unsafe {
            if self.descriptor_pool != vk::DescriptorPool::null() {
                device.destroy_descriptor_pool(self.descriptor_pool, None);
                self.descriptor_pool = vk::DescriptorPool::null();
            }
            for pipeline in [&mut self.box_filter, &mut self.blur, &mut self.composite] {
                if *pipeline != vk::Pipeline::null() {
                    device.destroy_pipeline(*pipeline, None);
                    *pipeline = vk::Pipeline::null();
                }
            }
            if self.sampler != vk::Sampler::null() {
                device.destroy_sampler(self.sampler, None);
                self.sampler = vk::Sampler::null();
            }
            if self.pipeline_layout != vk::PipelineLayout::null() {
                device.destroy_pipeline_layout(self.pipeline_layout, None);
                self.pipeline_layout = vk::PipelineLayout::null();
            }
            if self.set_layout != vk::DescriptorSetLayout::null() {
                device.destroy_descriptor_set_layout(self.set_layout, None);
                self.set_layout = vk::DescriptorSetLayout::null();
            }
        }
        self.extent = None;
        self.format = vk::Format::UNDEFINED;
    }
}

fn update_set(
    device: &Device,
    set: vk::DescriptorSet,
    sampler: vk::Sampler,
    first: vk::ImageView,
    second: vk::ImageView,
) {
    let images = [first, second].map(|view| {
        vk::DescriptorImageInfo::default()
            .sampler(sampler)
            .image_view(view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
    });
    let writes = [0, 1].map(|binding| {
        vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(binding)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(std::slice::from_ref(&images[binding as usize]))
    });
    // SAFETY: Descriptor set and referenced sampler/views remain live.
    unsafe { device.update_descriptor_sets(&writes, &[]) };
}

fn create_pipeline(
    device: &Device,
    layout: vk::PipelineLayout,
    format: vk::Format,
    program: &GlowSpirvProgram,
) -> Result<vk::Pipeline, VulkanError> {
    let vertex_info = vk::ShaderModuleCreateInfo::default().code(program.vertex_words());
    // SAFETY: shaderc emitted aligned SPIR-V words.
    let vertex = unsafe { device.create_shader_module(&vertex_info, None) }
        .map_err(|source| VulkanError::operation("create glow vertex module", source))?;
    let fragment_info = vk::ShaderModuleCreateInfo::default().code(program.fragment_words());
    // SAFETY: Same invariant as the vertex module.
    let fragment = match unsafe { device.create_shader_module(&fragment_info, None) } {
        Ok(module) => module,
        Err(source) => {
            // SAFETY: The unsent vertex module is uniquely owned.
            unsafe { device.destroy_shader_module(vertex, None) };
            return Err(VulkanError::operation(
                "create glow fragment module",
                source,
            ));
        }
    };
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex)
            .name(c"main"),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fragment)
            .name(c"main"),
    ];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
    let assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let raster = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let attachment = [vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)];
    let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachment);
    let states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&states);
    let formats = [format];
    let mut rendering =
        vk::PipelineRenderingCreateInfo::default().color_attachment_formats(&formats);
    let info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&assembly)
        .viewport_state(&viewport)
        .rasterization_state(&raster)
        .multisample_state(&multisample)
        .color_blend_state(&blend)
        .dynamic_state(&dynamic)
        .layout(layout)
        .push_next(&mut rendering);
    // SAFETY: All pipeline state and modules remain live during creation.
    let result =
        unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), &[info], None) }
            .map_err(|(_partial, source)| VulkanError::operation("create glow pipeline", source))
            .and_then(|pipelines| {
                pipelines.into_iter().next().ok_or_else(|| {
                    VulkanError::operation("create glow pipeline", "driver returned none")
                })
            });
    // SAFETY: Vulkan has consumed module bytecode regardless of pipeline result.
    unsafe {
        device.destroy_shader_module(fragment, None);
        device.destroy_shader_module(vertex, None);
    }
    result
}

fn begin_color_target(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    view: vk::ImageView,
    extent: vk::Extent2D,
    load_op: vk::AttachmentLoadOp,
) {
    let attachment = [vk::RenderingAttachmentInfo::default()
        .image_view(view)
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(load_op)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(vk::ClearValue {
            color: vk::ClearColorValue { float32: [0.0; 4] },
        })];
    let info = vk::RenderingInfo::default()
        .render_area(vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent,
        })
        .layer_count(1)
        .color_attachments(&attachment);
    // SAFETY: View layout and pipeline format agree with this scope.
    unsafe { device.cmd_begin_rendering(command_buffer, &info) };
}

fn set_viewport_scissor(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    extent: vk::Extent2D,
    window: Option<WorldScreenWindow>,
) {
    let viewport = vk::Viewport {
        x: 0.0,
        y: 0.0,
        width: extent.width as f32,
        height: extent.height as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    };
    let scissor = window.map_or(
        vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent,
        },
        |window| {
            let width = extent.width as f32;
            let height = extent.height as f32;
            let left = (window.minimum_x() + 1.0) * 0.5 * width;
            let right = (window.maximum_x() + 1.0) * 0.5 * width;
            let bottom = (window.minimum_y() + 1.0) * 0.5 * height;
            let top = (window.maximum_y() + 1.0) * 0.5 * height;
            vk::Rect2D {
                offset: vk::Offset2D {
                    x: left.floor() as i32,
                    y: (height - top).floor() as i32,
                },
                extent: vk::Extent2D {
                    width: (right.ceil() - left.floor()) as u32,
                    height: (top.ceil() - bottom.floor()) as u32,
                },
            }
        },
    );
    // SAFETY: The active pipeline declares both states dynamic.
    unsafe {
        device.cmd_set_viewport(command_buffer, 0, &[viewport]);
        device.cmd_set_scissor(command_buffer, 0, &[scissor]);
    }
}

fn bind_and_draw(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    set: vk::DescriptorSet,
    parameters: [f32; 4],
) {
    let bytes = parameters.map(f32::to_ne_bytes).concat();
    // SAFETY: Pipeline/layout/set share the fixed ABI and the triangle has no vertex input.
    unsafe {
        device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, pipeline);
        device.cmd_bind_descriptor_sets(
            command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            layout,
            0,
            &[set],
            &[],
        );
        device.cmd_push_constants(
            command_buffer,
            layout,
            vk::ShaderStageFlags::FRAGMENT,
            0,
            &bytes,
        );
        device.cmd_draw(command_buffer, 3, 1, 0, 0);
    }
}

fn transition_swapchain_to_copy(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
) {
    transition_image(
        device,
        command_buffer,
        image,
        vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        vk::PipelineStageFlags2::TRANSFER,
        vk::AccessFlags2::TRANSFER_READ,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
    );
}

#[allow(clippy::too_many_arguments)]
fn transition_image(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
    source_stage: vk::PipelineStageFlags2,
    source_access: vk::AccessFlags2,
    old_layout: vk::ImageLayout,
    destination_stage: vk::PipelineStageFlags2,
    destination_access: vk::AccessFlags2,
    new_layout: vk::ImageLayout,
) {
    let barriers = [vk::ImageMemoryBarrier2::default()
        .src_stage_mask(source_stage)
        .src_access_mask(source_access)
        .old_layout(old_layout)
        .dst_stage_mask(destination_stage)
        .dst_access_mask(destination_access)
        .new_layout(new_layout)
        .image(image)
        .subresource_range(color_range())];
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&barriers);
    // SAFETY: The caller orders uses of this live image in one command buffer.
    unsafe { device.cmd_pipeline_barrier2(command_buffer, &dependency) };
}

fn color_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1)
}

fn color_layers() -> vk::ImageSubresourceLayers {
    vk::ImageSubresourceLayers::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .layer_count(1)
}

fn glow_shader_error(error: impl ToString) -> VulkanError {
    VulkanError::GlowShader {
        message: error.to_string(),
    }
}
