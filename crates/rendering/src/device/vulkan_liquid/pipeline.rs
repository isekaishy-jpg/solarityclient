//! Descriptor layout and programmable liquid pipeline state from native 8A5590/8A6090.

#![allow(unsafe_code)]

use ash::{Device, vk};

use crate::device::VulkanError;
use crate::{LiquidRenderVertex, LiquidShader, LiquidSpirvProgram};

/// One dynamic draw uniform and the depth/surface texture pair.
#[derive(Default)]
pub(super) struct LiquidPipelineLayout {
    descriptor_sets: [vk::DescriptorSetLayout; 2],
    handle: vk::PipelineLayout,
}

impl LiquidPipelineLayout {
    /// Creates the shared descriptor ABI transactionally on first use.
    pub(super) fn ensure_created(&mut self, device: &Device) -> Result<(), VulkanError> {
        if self.handle != vk::PipelineLayout::null() {
            return Ok(());
        }
        let scene = [binding(
            0,
            vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
        )];
        let material = (0..2)
            .map(|index| {
                binding(
                    index,
                    vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    vk::ShaderStageFlags::FRAGMENT,
                )
            })
            .collect::<Vec<_>>();
        for (index, bindings) in [scene.as_slice(), material.as_slice()].iter().enumerate() {
            let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(bindings);
            // SAFETY: Binding storage remains live and has no immutable samplers.
            self.descriptor_sets[index] =
                unsafe { device.create_descriptor_set_layout(&info, None) }.map_err(|source| {
                    self.destroy(device);
                    VulkanError::operation("create liquid descriptor set layout", source)
                })?;
        }
        let info = vk::PipelineLayoutCreateInfo::default().set_layouts(&self.descriptor_sets);
        // SAFETY: Descriptor layouts and borrowed slices remain live.
        self.handle = unsafe { device.create_pipeline_layout(&info, None) }.map_err(|source| {
            self.destroy(device);
            VulkanError::operation("create liquid pipeline layout", source)
        })?;
        Ok(())
    }

    pub(super) const fn handle(&self) -> vk::PipelineLayout {
        self.handle
    }

    pub(super) fn descriptor_set(&self, index: usize) -> Option<vk::DescriptorSetLayout> {
        self.descriptor_sets.get(index).copied()
    }

    /// Releases layouts after frame descriptors and pipelines have retired.
    pub(super) fn destroy(&mut self, device: &Device) {
        // SAFETY: All handles are uniquely owned by this idle renderer.
        unsafe {
            if self.handle != vk::PipelineLayout::null() {
                device.destroy_pipeline_layout(self.handle, None);
                self.handle = vk::PipelineLayout::null();
            }
            for layout in self.descriptor_sets.iter_mut().rev() {
                if *layout != vk::DescriptorSetLayout::null() {
                    device.destroy_descriptor_set_layout(*layout, None);
                    *layout = vk::DescriptorSetLayout::null();
                }
            }
        }
    }
}

/// Declares a single descriptor without immutable sampler ownership.
fn binding(
    binding: u32,
    descriptor_type: vk::DescriptorType,
    stage_flags: vk::ShaderStageFlags,
) -> vk::DescriptorSetLayoutBinding<'static> {
    vk::DescriptorSetLayoutBinding::default()
        .binding(binding)
        .descriptor_type(descriptor_type)
        .descriptor_count(1)
        .stage_flags(stage_flags)
}

/// Creates one build-validated shader family for the world attachment formats.
pub(super) fn create_pipeline(
    device: &Device,
    layout: vk::PipelineLayout,
    color_format: vk::Format,
    depth_format: vk::Format,
    program: &LiquidSpirvProgram,
) -> Result<vk::Pipeline, VulkanError> {
    let modules = ShaderModules::create(device, program)?;
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
        .stride(LiquidRenderVertex::BYTE_SIZE as u32)
        .input_rate(vk::VertexInputRate::VERTEX)];
    let attributes = [
        attribute(0, vk::Format::R32G32B32_SFLOAT, 0),
        attribute(1, vk::Format::R32G32B32_SFLOAT, 12),
        attribute(2, vk::Format::R8G8B8A8_UNORM, 24),
        attribute(3, vk::Format::R32G32_SFLOAT, 28),
        attribute(4, vk::Format::R32G32_SFLOAT, 36),
    ];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&bindings)
        .vertex_attribute_descriptions(&attributes);
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_STRIP);
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        // Native material draws explicitly disable culling in GX state 17.
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    // Native water sets GX depth-write state 15 to !MaxUserClipPlanes (8A56A2,
    // D3D caps mapping 68F253). The programmable user-clipping path disables
    // writes; opaque magma retains the enclosing world pass's depth writes.
    let depth = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(true)
        .depth_write_enable(program.shader() == LiquidShader::Magma)
        .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL);
    let attachments = [vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(program.shader() != LiquidShader::Magma)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
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
    // SAFETY: All referenced objects and slices remain live for this call.
    unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), &[info], None) }
        .map_err(|(partial, source)| {
            for pipeline in partial {
                if pipeline != vk::Pipeline::null() {
                    // SAFETY: Failed creation never submitted these newly returned pipelines.
                    unsafe { device.destroy_pipeline(pipeline, None) };
                }
            }
            VulkanError::operation("create liquid graphics pipeline", source)
        })?
        .into_iter()
        .next()
        .ok_or_else(|| {
            VulkanError::operation("create liquid graphics pipeline", "driver returned none")
        })
}

/// Describes one packed attribute in the shared 44-byte liquid vertex.
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

/// Temporary modules retained only until graphics pipeline creation completes.
struct ShaderModules<'a> {
    device: &'a Device,
    vertex: vk::ShaderModule,
    fragment: vk::ShaderModule,
}

impl<'a> ShaderModules<'a> {
    /// Creates both stages and releases the first if the second fails.
    fn create(device: &'a Device, program: &LiquidSpirvProgram) -> Result<Self, VulkanError> {
        let vertex_info = vk::ShaderModuleCreateInfo::default().code(program.vertex_words());
        // SAFETY: shaderc produced aligned native SPIR-V words.
        let vertex =
            unsafe { device.create_shader_module(&vertex_info, None) }.map_err(|source| {
                VulkanError::operation("create liquid vertex shader module", source)
            })?;
        let fragment_info = vk::ShaderModuleCreateInfo::default().code(program.fragment_words());
        // SAFETY: Same invariant as the vertex module.
        let fragment = match unsafe { device.create_shader_module(&fragment_info, None) } {
            Ok(fragment) => fragment,
            Err(source) => {
                // SAFETY: The vertex module is uniquely owned and unsubmitted.
                unsafe { device.destroy_shader_module(vertex, None) };
                return Err(VulkanError::operation(
                    "create liquid fragment shader module",
                    source,
                ));
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
        // SAFETY: Pipeline creation no longer retains module handles.
        unsafe {
            self.device.destroy_shader_module(self.fragment, None);
            self.device.destroy_shader_module(self.vertex, None);
        }
    }
}
