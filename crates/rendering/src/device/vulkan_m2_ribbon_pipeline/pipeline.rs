//! Borrowed descriptor ABI, shader modules, and fixed ribbon pipeline state.

#![allow(unsafe_code)]

use ash::{Device, vk};

use crate::device::VulkanError;
use crate::{M2BlendFactor, M2MaterialState, M2RibbonRenderVertex, M2RibbonSpirvProgram};

/// Pipeline layout borrowing the renderer's compatible M2 descriptor layouts.
#[derive(Default)]
pub(super) struct M2RibbonPipelineLayout {
    handle: vk::PipelineLayout,
    scene_set: vk::DescriptorSetLayout,
    texture_set: vk::DescriptorSetLayout,
}

impl M2RibbonPipelineLayout {
    pub(super) fn ensure_created(
        &mut self,
        device: &Device,
        scene_set: vk::DescriptorSetLayout,
        texture_set: vk::DescriptorSetLayout,
    ) -> Result<(), VulkanError> {
        if self.handle != vk::PipelineLayout::null() {
            if self.scene_set != scene_set || self.texture_set != texture_set {
                return Err(VulkanError::M2RibbonDescriptorLayoutChanged);
            }
            return Ok(());
        }
        let sets = [scene_set, texture_set];
        let info = vk::PipelineLayoutCreateInfo::default().set_layouts(&sets);
        // SAFETY: Both borrowed layouts are live and remain owned by the M2
        // pipeline registry until after this layout is destroyed.
        self.handle = unsafe { device.create_pipeline_layout(&info, None) }
            .map_err(|source| VulkanError::operation("create M2 ribbon pipeline layout", source))?;
        self.scene_set = scene_set;
        self.texture_set = texture_set;
        Ok(())
    }

    pub(super) const fn handle(&self) -> vk::PipelineLayout {
        self.handle
    }

    pub(super) fn destroy(&mut self, device: &Device) {
        // SAFETY: The non-null layout belongs to this device and is idle.
        unsafe {
            if self.handle != vk::PipelineLayout::null() {
                device.destroy_pipeline_layout(self.handle, None);
            }
        }
        self.handle = vk::PipelineLayout::null();
        self.scene_set = vk::DescriptorSetLayout::null();
        self.texture_set = vk::DescriptorSetLayout::null();
    }
}

pub(super) fn create_pipeline(
    device: &Device,
    pipeline_cache: vk::PipelineCache,
    layout: vk::PipelineLayout,
    color_format: vk::Format,
    depth_format: vk::Format,
    material: M2MaterialState,
    program: &M2RibbonSpirvProgram,
) -> Result<vk::Pipeline, VulkanError> {
    let modules = ShaderModules::create(device, program)?;
    let vertex_value = program.vertex_specialization()[0].to_ne_bytes();
    let vertex_entries = [vk::SpecializationMapEntry {
        constant_id: 0,
        offset: 0,
        size: 4,
    }];
    let vertex_specialization = vk::SpecializationInfo::default()
        .map_entries(&vertex_entries)
        .data(&vertex_value);
    let fragment_value = program.fragment_specialization()[0].to_ne_bytes();
    let fragment_entries = [vk::SpecializationMapEntry {
        constant_id: 0,
        offset: 0,
        size: 4,
    }];
    let fragment_specialization = vk::SpecializationInfo::default()
        .map_entries(&fragment_entries)
        .data(&fragment_value);
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(modules.vertex)
            .name(c"main")
            .specialization_info(&vertex_specialization),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(modules.fragment)
            .name(c"main")
            .specialization_info(&fragment_specialization),
    ];
    let bindings = [vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(M2RibbonRenderVertex::BYTE_SIZE as u32)
        .input_rate(vk::VertexInputRate::VERTEX)];
    let attributes = [
        vertex_attribute(0, vk::Format::R32G32B32_SFLOAT, 0),
        vertex_attribute(1, vk::Format::B8G8R8A8_UNORM, 12),
        vertex_attribute(2, vk::Format::R32G32_SFLOAT, 16),
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
        .cull_mode(if material.cull_enabled() {
            vk::CullModeFlags::BACK
        } else {
            vk::CullModeFlags::NONE
        })
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(material.depth_test_enabled())
        .depth_write_enable(material.depth_write_enabled())
        .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL);
    let color_attachment = vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(material.blend_enabled())
        .src_color_blend_factor(blend_factor(material.source_blend()))
        .dst_color_blend_factor(blend_factor(material.destination_blend()))
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(blend_factor(material.source_blend()))
        .dst_alpha_blend_factor(blend_factor(material.destination_blend()))
        .alpha_blend_op(vk::BlendOp::ADD)
        .color_write_mask(vk::ColorComponentFlags::RGBA);
    let color_attachments = [color_attachment];
    let color_blend =
        vk::PipelineColorBlendStateCreateInfo::default().attachments(&color_attachments);
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
        .depth_stencil_state(&depth_stencil)
        .color_blend_state(&color_blend)
        .dynamic_state(&dynamic)
        .layout(layout)
        .push_next(&mut rendering);
    // SAFETY: Every create-info pointer and module/layout handle remains live.
    unsafe { device.create_graphics_pipelines(pipeline_cache, &[info], None) }
        .map_err(|(_partial, source)| {
            VulkanError::operation("create M2 ribbon graphics pipeline", source)
        })?
        .into_iter()
        .next()
        .ok_or_else(|| {
            VulkanError::operation("create M2 ribbon graphics pipeline", "driver returned none")
        })
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

const fn blend_factor(factor: M2BlendFactor) -> vk::BlendFactor {
    match factor {
        M2BlendFactor::Zero => vk::BlendFactor::ZERO,
        M2BlendFactor::One => vk::BlendFactor::ONE,
        M2BlendFactor::SourceAlpha => vk::BlendFactor::SRC_ALPHA,
        M2BlendFactor::OneMinusSourceAlpha => vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
        M2BlendFactor::DestinationColor => vk::BlendFactor::DST_COLOR,
        M2BlendFactor::SourceColor => vk::BlendFactor::SRC_COLOR,
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
        program: &M2RibbonSpirvProgram,
    ) -> Result<Self, VulkanError> {
        let vertex_info = vk::ShaderModuleCreateInfo::default().code(program.vertex_words());
        // SAFETY: shaderc produced aligned words retained by `program`.
        let vertex =
            unsafe { device.create_shader_module(&vertex_info, None) }.map_err(|source| {
                VulkanError::operation("create M2 ribbon vertex shader module", source)
            })?;
        let fragment_info = vk::ShaderModuleCreateInfo::default().code(program.fragment_words());
        // SAFETY: The fragment words have the same compiler-owned invariant.
        let fragment = match unsafe { device.create_shader_module(&fragment_info, None) } {
            Ok(module) => module,
            Err(source) => {
                // SAFETY: The vertex module is live and not submitted.
                unsafe { device.destroy_shader_module(vertex, None) };
                return Err(VulkanError::operation(
                    "create M2 ribbon fragment shader module",
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
        // SAFETY: Both modules are uniquely owned after pipeline creation.
        unsafe {
            self.device.destroy_shader_module(self.fragment, None);
            self.device.destroy_shader_module(self.vertex, None);
        }
    }
}
