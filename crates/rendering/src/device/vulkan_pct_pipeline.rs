//! Shared PCT pipeline ownership for native surface ripples and underwater billboards.

#![allow(unsafe_code)]

use ash::{Device, vk};

use crate::{UnderwaterParticleVertex, VulkanError};

/// Native fixed-function variants sharing the packed XYZ/RGBA/UV vertex ABI.
#[derive(Clone, Copy)]
pub(in crate::device) enum PctPipelineKind {
    Ripple,
    Underwater,
}

/// Renderer-lifetime ownership of one PCT pipeline and descriptor ABI.
#[derive(Default)]
pub(in crate::device) struct PctPipeline {
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    descriptor: vk::DescriptorSetLayout,
}

impl PctPipeline {
    /// Creates the complete pipeline before any frame publishes descriptors.
    pub(in crate::device) fn prepare(
        &mut self,
        device: &Device,
        color: vk::Format,
        depth: vk::Format,
        kind: PctPipelineKind,
    ) -> Result<(), VulkanError> {
        if self.pipeline != vk::Pipeline::null() {
            return Ok(());
        }
        let result = self.create(device, color, depth, kind);
        if result.is_err() {
            self.destroy(device);
        }
        result
    }

    fn create(
        &mut self,
        device: &Device,
        color: vk::Format,
        depth: vk::Format,
        kind: PctPipelineKind,
    ) -> Result<(), VulkanError> {
        let bindings = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)];
        let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
        // SAFETY: Binding storage remains live and has no immutable samplers.
        self.descriptor = unsafe { device.create_descriptor_set_layout(&info, None) }
            .map_err(|source| VulkanError::operation("create PCT descriptor layout", source))?;
        let sets = [self.descriptor];
        let pushes = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .size(match kind {
                PctPipelineKind::Ripple => 68,
                PctPipelineKind::Underwater => 96,
            })];
        let info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&sets)
            .push_constant_ranges(&pushes);
        // SAFETY: The descriptor layout is live, and 96 bytes fit Vulkan's required minimum.
        self.layout = unsafe { device.create_pipeline_layout(&info, None) }
            .map_err(|source| VulkanError::operation("create PCT pipeline layout", source))?;
        let modules = ShaderModules::create(device, kind)?;
        self.pipeline = create_pipeline(device, self.layout, color, depth, &modules, kind)?;
        Ok(())
    }

    pub(in crate::device) const fn descriptor_layout(&self) -> vk::DescriptorSetLayout {
        self.descriptor
    }

    pub(in crate::device) const fn raw(&self) -> (vk::Pipeline, vk::PipelineLayout) {
        (self.pipeline, self.layout)
    }

    /// Releases the owned objects after every world-frame slot has retired.
    pub(in crate::device) fn destroy(&mut self, device: &Device) {
        // SAFETY: Renderer teardown or failed creation excludes all GPU users.
        unsafe {
            if self.pipeline != vk::Pipeline::null() {
                device.destroy_pipeline(self.pipeline, None);
            }
            if self.layout != vk::PipelineLayout::null() {
                device.destroy_pipeline_layout(self.layout, None);
            }
            if self.descriptor != vk::DescriptorSetLayout::null() {
                device.destroy_descriptor_set_layout(self.descriptor, None);
            }
        }
        *self = Self::default();
    }
}

fn create_pipeline(
    device: &Device,
    layout: vk::PipelineLayout,
    color_format: vk::Format,
    depth_format: vk::Format,
    modules: &ShaderModules<'_>,
    kind: PctPipelineKind,
) -> Result<vk::Pipeline, VulkanError> {
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(modules.vertex)
            .name(c"main"),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(modules.fragment)
            .name(c"main"),
    ];
    let bindings = [vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(UnderwaterParticleVertex::BYTE_SIZE as u32)
        .input_rate(vk::VertexInputRate::VERTEX)];
    let attributes = [
        attribute(0, vk::Format::R32G32B32_SFLOAT, 0),
        attribute(1, vk::Format::R8G8B8A8_UNORM, 12),
        attribute(2, vk::Format::R32G32_SFLOAT, 16),
    ];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&bindings)
        .vertex_attribute_descriptions(&attributes);
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let depth = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(true)
        .depth_write_enable(false)
        .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL);
    // Native GX blend 3 is source alpha / one; blend 2 uses inverse source
    // alpha. Neither enables separate alpha blending (A2F964/A2F994).
    let destination = match kind {
        PctPipelineKind::Ripple => vk::BlendFactor::ONE,
        PctPipelineKind::Underwater => vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
    };
    let attachments = [vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_color_blend_factor(destination)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_alpha_blend_factor(destination)
        .alpha_blend_op(vk::BlendOp::ADD)
        .color_write_mask(vk::ColorComponentFlags::RGBA)];
    let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
    let color_formats = [color_format];
    let mut rendering = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(&color_formats)
        .depth_attachment_format(depth_format)
        .stencil_attachment_format(depth_format);
    let info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport)
        .rasterization_state(&rasterization)
        .multisample_state(&multisample)
        .depth_stencil_state(&depth)
        .color_blend_state(&blend)
        .dynamic_state(&dynamic)
        .layout(layout)
        .push_next(&mut rendering);
    // SAFETY: All descriptor layouts, shader modules, and borrowed state remain live.
    unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), &[info], None) }
        .map_err(|(partial, source)| {
            for pipeline in partial {
                if pipeline != vk::Pipeline::null() {
                    // SAFETY: Failed creation never submitted the returned partial pipelines.
                    unsafe { device.destroy_pipeline(pipeline, None) };
                }
            }
            VulkanError::operation("create PCT graphics pipeline", source)
        })?
        .into_iter()
        .next()
        .ok_or_else(|| {
            VulkanError::operation("create PCT graphics pipeline", "driver returned none")
        })
}

fn attribute(
    location: u32,
    format: vk::Format,
    offset: u32,
) -> vk::VertexInputAttributeDescription {
    vk::VertexInputAttributeDescription::default()
        .location(location)
        .binding(0)
        .format(format)
        .offset(offset)
}

/// Temporary modules whose ownership ends after graphics pipeline creation.
struct ShaderModules<'a> {
    device: &'a Device,
    vertex: vk::ShaderModule,
    fragment: vk::ShaderModule,
}

impl<'a> ShaderModules<'a> {
    fn create(device: &'a Device, kind: PctPipelineKind) -> Result<Self, VulkanError> {
        let create = |bytes: &[u8]| {
            let words: Vec<u32> = bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|word| u32::from_le_bytes(*word))
                .collect();
            let info = vk::ShaderModuleCreateInfo::default().code(&words);
            // SAFETY: shaderc validates the complete word-aligned SPIR-V at build time.
            unsafe { device.create_shader_module(&info, None) }
                .map_err(|source| VulkanError::operation("create PCT shader module", source))
        };
        let (vertex_bytes, fragment_bytes): (&[u8], &[u8]) = match kind {
            PctPipelineKind::Ripple => (
                include_bytes!(concat!(env!("OUT_DIR"), "/ripple.vert.spv")),
                include_bytes!(concat!(env!("OUT_DIR"), "/ripple.frag.spv")),
            ),
            PctPipelineKind::Underwater => (
                include_bytes!(concat!(env!("OUT_DIR"), "/underwater.vert.spv")),
                include_bytes!(concat!(env!("OUT_DIR"), "/underwater.frag.spv")),
            ),
        };
        let vertex = create(vertex_bytes)?;
        let fragment = match create(fragment_bytes) {
            Ok(fragment) => fragment,
            Err(error) => {
                // SAFETY: This unsubmitted vertex module has unique local ownership.
                unsafe { device.destroy_shader_module(vertex, None) };
                return Err(error);
            }
        };
        Ok(Self {
            device,
            vertex,
            fragment,
        })
    }
}

impl Drop for ShaderModules<'_> {
    fn drop(&mut self) {
        // SAFETY: Pipeline creation has finished using these uniquely owned modules.
        unsafe {
            self.device.destroy_shader_module(self.fragment, None);
            self.device.destroy_shader_module(self.vertex, None);
        }
    }
}
