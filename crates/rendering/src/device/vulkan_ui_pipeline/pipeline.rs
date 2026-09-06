//! Descriptor layout, shader modules, and fixed-function UI pipeline state.

#![allow(unsafe_code)]

use ash::{Device, vk};

use crate::device::VulkanError;
use crate::{UiRenderBlend, UiRenderVertex, UiSpirvProgram};

const CANVAS_PUSH_CONSTANT_BYTES: u32 = 20;

/// Common sampled-image and canvas-transform ABI for every UI variant.
#[derive(Default)]
pub(super) struct UiPipelineLayout {
    texture_set: vk::DescriptorSetLayout,
    handle: vk::PipelineLayout,
}

impl UiPipelineLayout {
    /// Lazily creates the shared ABI while covering partial failure cleanup.
    pub(super) fn ensure_created(&mut self, device: &Device) -> Result<(), VulkanError> {
        if self.handle != vk::PipelineLayout::null() {
            return Ok(());
        }
        let binding = vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT);
        let bindings = [binding];
        let descriptor_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
        // SAFETY: The binding slice is live and contains no immutable sampler pointer.
        self.texture_set = unsafe { device.create_descriptor_set_layout(&descriptor_info, None) }
            .map_err(|source| {
            VulkanError::operation("create UI texture descriptor layout", source)
        })?;
        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .offset(0)
            .size(CANVAS_PUSH_CONSTANT_BYTES);
        let sets = [self.texture_set];
        let push_ranges = [push_range];
        let layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&sets)
            .push_constant_ranges(&push_ranges);
        // SAFETY: The descriptor layout and all slices remain live for the call.
        self.handle =
            unsafe { device.create_pipeline_layout(&layout_info, None) }.map_err(|source| {
                self.destroy(device);
                VulkanError::operation("create UI pipeline layout", source)
            })?;
        Ok(())
    }

    pub(super) const fn handle(&self) -> vk::PipelineLayout {
        self.handle
    }

    pub(super) const fn texture_set(&self) -> vk::DescriptorSetLayout {
        self.texture_set
    }

    /// Releases the parent pipeline layout before its descriptor layout.
    pub(super) fn destroy(&mut self, device: &Device) {
        // SAFETY: Non-null handles belong to this device and are uniquely owned.
        unsafe {
            if self.handle != vk::PipelineLayout::null() {
                device.destroy_pipeline_layout(self.handle, None);
                self.handle = vk::PipelineLayout::null();
            }
            if self.texture_set != vk::DescriptorSetLayout::null() {
                device.destroy_descriptor_set_layout(self.texture_set, None);
                self.texture_set = vk::DescriptorSetLayout::null();
            }
        }
    }
}

/// Creates one dynamic-rendering pipeline with exact simple-render blend state.
pub(super) fn create_pipeline(
    device: &Device,
    layout: vk::PipelineLayout,
    color_format: vk::Format,
    blend: UiRenderBlend,
    program: &UiSpirvProgram,
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
        .stride(UiRenderVertex::BYTE_SIZE as u32)
        .input_rate(vk::VertexInputRate::VERTEX)];
    let attributes = [
        vertex_attribute(0, vk::Format::R32G32_SFLOAT, 0),
        vertex_attribute(1, vk::Format::R32G32_SFLOAT, 8),
        vertex_attribute(2, vk::Format::R32G32B32A32_SFLOAT, 16),
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
    let (source_factor, destination_factor) = match blend {
        UiRenderBlend::Alpha => (
            vk::BlendFactor::SRC_ALPHA,
            vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
        ),
        UiRenderBlend::Additive => (vk::BlendFactor::SRC_ALPHA, vk::BlendFactor::ONE),
        UiRenderBlend::AlphaMask => (vk::BlendFactor::ONE, vk::BlendFactor::ZERO),
    };
    let color_attachment = vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(true)
        .src_color_blend_factor(source_factor)
        .dst_color_blend_factor(destination_factor)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(source_factor)
        .dst_alpha_blend_factor(destination_factor)
        .alpha_blend_op(vk::BlendOp::ADD)
        .color_write_mask(if blend == UiRenderBlend::AlphaMask {
            vk::ColorComponentFlags::A
        } else {
            vk::ColorComponentFlags::RGBA
        });
    let color_attachments = [color_attachment];
    let color_blend =
        vk::PipelineColorBlendStateCreateInfo::default().attachments(&color_attachments);
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
    let color_formats = [color_format];
    let mut rendering =
        vk::PipelineRenderingCreateInfo::default().color_attachment_formats(&color_formats);
    let create_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport)
        .rasterization_state(&rasterization)
        .multisample_state(&multisample)
        .color_blend_state(&color_blend)
        .dynamic_state(&dynamic)
        .layout(layout)
        .push_next(&mut rendering);
    // SAFETY: All pointers and module/layout handles remain live for this call.
    unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), &[create_info], None) }
        .map_err(|(_partial, source)| {
            VulkanError::operation("create UI graphics pipeline", source)
        })?
        .into_iter()
        .next()
        .ok_or_else(|| {
            VulkanError::operation("create UI graphics pipeline", "driver returned none")
        })
}

/// Creates one attribute in the sole per-vertex binding.
fn vertex_attribute(
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

/// Temporary shader modules destroyed after pipeline creation returns.
struct ShaderModules<'device> {
    device: &'device Device,
    vertex: vk::ShaderModule,
    fragment: vk::ShaderModule,
}

impl<'device> ShaderModules<'device> {
    /// Creates both modules and covers fragment-module partial failure.
    fn create(device: &'device Device, program: &UiSpirvProgram) -> Result<Self, VulkanError> {
        let vertex_info = vk::ShaderModuleCreateInfo::default().code(program.vertex_words());
        // SAFETY: shaderc produced aligned SPIR-V words retained by the program.
        let vertex = unsafe { device.create_shader_module(&vertex_info, None) }
            .map_err(|source| VulkanError::operation("create UI vertex shader module", source))?;
        let fragment_info = vk::ShaderModuleCreateInfo::default().code(program.fragment_words());
        // SAFETY: The fragment words have the same compiler-owned invariant.
        let fragment = match unsafe { device.create_shader_module(&fragment_info, None) } {
            Ok(module) => module,
            Err(source) => {
                // SAFETY: The vertex module is live and has not been submitted.
                unsafe { device.destroy_shader_module(vertex, None) };
                return Err(VulkanError::operation(
                    "create UI fragment shader module",
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
    /// Releases both modules after Vulkan has consumed their create info.
    fn drop(&mut self) {
        // SAFETY: Both modules are uniquely owned and no submission references them.
        unsafe {
            self.device.destroy_shader_module(self.fragment, None);
            self.device.destroy_shader_module(self.vertex, None);
        }
    }
}
