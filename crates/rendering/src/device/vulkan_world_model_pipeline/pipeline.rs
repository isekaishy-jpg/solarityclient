//! Descriptor layout, modules, vertex ABI, and fixed WMO pipeline state.

#![allow(unsafe_code)]

use ash::{Device, vk};

use crate::device::VulkanError;
use crate::{
    WorldModelBlendFactor, WorldModelMaterialState, WorldModelRenderVertex, WorldModelSpirvProgram,
};

const DESCRIPTOR_SET_COUNT: usize = 3;

/// Common scene/material/texture descriptor ABI for every MapObj variant.
#[derive(Default)]
pub(super) struct WorldModelPipelineLayout {
    descriptor_sets: [vk::DescriptorSetLayout; DESCRIPTOR_SET_COUNT],
    handle: vk::PipelineLayout,
}

impl WorldModelPipelineLayout {
    pub(super) fn ensure_created(&mut self, device: &Device) -> Result<(), VulkanError> {
        if self.handle != vk::PipelineLayout::null() {
            return Ok(());
        }
        let bindings = [
            vec![descriptor_binding(
                0,
                vk::DescriptorType::UNIFORM_BUFFER,
                vk::ShaderStageFlags::VERTEX,
            )],
            vec![descriptor_binding(
                0,
                vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            )],
            vec![
                descriptor_binding(
                    0,
                    vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    vk::ShaderStageFlags::FRAGMENT,
                ),
                descriptor_binding(
                    1,
                    vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    vk::ShaderStageFlags::FRAGMENT,
                ),
            ],
        ];
        for (index, set_bindings) in bindings.iter().enumerate() {
            let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(set_bindings);
            // SAFETY: The binding slices remain live for this call.
            self.descriptor_sets[index] =
                unsafe { device.create_descriptor_set_layout(&info, None) }.map_err(|source| {
                    self.destroy(device);
                    VulkanError::operation("create WMO descriptor set layout", source)
                })?;
        }
        let info = vk::PipelineLayoutCreateInfo::default().set_layouts(&self.descriptor_sets);
        // SAFETY: All descriptor layouts are live children of this device.
        self.handle = unsafe { device.create_pipeline_layout(&info, None) }.map_err(|source| {
            self.destroy(device);
            VulkanError::operation("create WMO pipeline layout", source)
        })?;
        Ok(())
    }

    pub(super) const fn handle(&self) -> vk::PipelineLayout {
        self.handle
    }

    pub(super) fn destroy(&mut self, device: &Device) {
        // SAFETY: Non-null handles belong to this device and are uniquely owned.
        unsafe {
            if self.handle != vk::PipelineLayout::null() {
                device.destroy_pipeline_layout(self.handle, None);
                self.handle = vk::PipelineLayout::null();
            }
            for set in self.descriptor_sets.iter_mut().rev() {
                if *set != vk::DescriptorSetLayout::null() {
                    device.destroy_descriptor_set_layout(*set, None);
                    *set = vk::DescriptorSetLayout::null();
                }
            }
        }
    }
}

fn descriptor_binding(
    binding: u32,
    descriptor_type: vk::DescriptorType,
    stages: vk::ShaderStageFlags,
) -> vk::DescriptorSetLayoutBinding<'static> {
    vk::DescriptorSetLayoutBinding::default()
        .binding(binding)
        .descriptor_type(descriptor_type)
        .descriptor_count(1)
        .stage_flags(stages)
}

pub(super) fn create_pipeline(
    device: &Device,
    layout: vk::PipelineLayout,
    color_format: vk::Format,
    depth_format: vk::Format,
    material: WorldModelMaterialState,
    program: &WorldModelSpirvProgram,
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
    let binding = [vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(WorldModelRenderVertex::BYTE_SIZE as u32)
        .input_rate(vk::VertexInputRate::VERTEX)];
    let attributes = vertex_attributes();
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&binding)
        .vertex_attribute_descriptions(&attributes);
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(if material.cull_enabled() {
            vk::CullModeFlags::BACK
        } else {
            vk::CullModeFlags::NONE
        })
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let depth = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(material.depth_test_enabled())
        .depth_write_enable(material.depth_write_enabled())
        .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL);
    let blend = material.blend();
    let color_factors = blend.color_factors();
    let alpha_factors = blend.alpha_factors();
    let attachment = [vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(blend.enabled())
        .src_color_blend_factor(blend_factor(color_factors[0]))
        .dst_color_blend_factor(blend_factor(color_factors[1]))
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(blend_factor(alpha_factors[0]))
        .dst_alpha_blend_factor(blend_factor(alpha_factors[1]))
        .alpha_blend_op(vk::BlendOp::ADD)
        .color_write_mask(vk::ColorComponentFlags::RGBA)];
    let color_blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachment);
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
        .color_blend_state(&color_blend)
        .dynamic_state(&dynamic)
        .layout(layout)
        .push_next(&mut rendering);
    // SAFETY: All create-info slices, modules, and the layout remain live.
    unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), &[info], None) }
        .map_err(|(_partial, source)| {
            VulkanError::operation("create WMO graphics pipeline", source)
        })?
        .into_iter()
        .next()
        .ok_or_else(|| {
            VulkanError::operation("create WMO graphics pipeline", "driver returned none")
        })
}

fn vertex_attributes() -> [vk::VertexInputAttributeDescription; 6] {
    [
        vertex_attribute(0, vk::Format::R32G32B32_SFLOAT, 0),
        vertex_attribute(1, vk::Format::R32G32B32_SFLOAT, 12),
        vertex_attribute(2, vk::Format::R32G32_SFLOAT, 24),
        vertex_attribute(3, vk::Format::R32G32_SFLOAT, 32),
        vertex_attribute(4, vk::Format::R32G32B32A32_SFLOAT, 40),
        vertex_attribute(5, vk::Format::R32G32B32A32_SFLOAT, 56),
    ]
}

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

const fn blend_factor(factor: WorldModelBlendFactor) -> vk::BlendFactor {
    match factor {
        WorldModelBlendFactor::Zero => vk::BlendFactor::ZERO,
        WorldModelBlendFactor::One => vk::BlendFactor::ONE,
        WorldModelBlendFactor::SourceAlpha => vk::BlendFactor::SRC_ALPHA,
        WorldModelBlendFactor::OneMinusSourceAlpha => vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
        WorldModelBlendFactor::SourceColor => vk::BlendFactor::SRC_COLOR,
        WorldModelBlendFactor::DestinationColor => vk::BlendFactor::DST_COLOR,
        WorldModelBlendFactor::DestinationAlpha => vk::BlendFactor::DST_ALPHA,
    }
}

struct ShaderModules<'device> {
    device: &'device Device,
    vertex: vk::ShaderModule,
    fragment: vk::ShaderModule,
}

impl<'device> ShaderModules<'device> {
    fn create(
        device: &'device Device,
        program: &WorldModelSpirvProgram,
    ) -> Result<Self, VulkanError> {
        let vertex_info = vk::ShaderModuleCreateInfo::default().code(program.vertex_words());
        // SAFETY: shaderc produced aligned SPIR-V words retained by `program`.
        let vertex = unsafe { device.create_shader_module(&vertex_info, None) }
            .map_err(|source| VulkanError::operation("create WMO vertex shader module", source))?;
        let fragment_info = vk::ShaderModuleCreateInfo::default().code(program.fragment_words());
        // SAFETY: The same invariant holds for the fragment module.
        let fragment = match unsafe { device.create_shader_module(&fragment_info, None) } {
            Ok(module) => module,
            Err(source) => {
                // SAFETY: The unsubmitted vertex module is uniquely owned.
                unsafe { device.destroy_shader_module(vertex, None) };
                return Err(VulkanError::operation(
                    "create WMO fragment shader module",
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
