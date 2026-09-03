//! Descriptor layout, shader modules, and fixed-function M2 pipeline state.

#![allow(unsafe_code)]

use ash::{Device, vk};

use crate::device::VulkanError;
use crate::model::M2RenderVertex;
use crate::shader::{M2BlendFactor, M2MaterialState, M2SpirvProgram};

use super::M2_MATERIAL_DESCRIPTOR_TYPE;

const DESCRIPTOR_SET_COUNT: usize = 5;
const DRAW_PUSH_CONSTANT_BYTES: u32 = 16;

/// Common descriptor/push-constant ABI shared by every M2 variant.
#[derive(Default)]
pub(super) struct M2PipelineLayout {
    descriptor_sets: [vk::DescriptorSetLayout; DESCRIPTOR_SET_COUNT],
    handle: vk::PipelineLayout,
}

impl M2PipelineLayout {
    /// Lazily constructs the common ABI while covering partial failures.
    pub(super) fn ensure_created(&mut self, device: &Device) -> Result<(), VulkanError> {
        if self.handle != vk::PipelineLayout::null() {
            return Ok(());
        }
        let bindings = [
            vec![descriptor_binding(
                0,
                vk::DescriptorType::UNIFORM_BUFFER,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            )],
            vec![descriptor_binding(
                0,
                vk::DescriptorType::STORAGE_BUFFER,
                vk::ShaderStageFlags::VERTEX,
            )],
            vec![descriptor_binding(
                0,
                M2_MATERIAL_DESCRIPTOR_TYPE,
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
                descriptor_binding(
                    2,
                    vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    vk::ShaderStageFlags::FRAGMENT,
                ),
                descriptor_binding(
                    3,
                    vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    vk::ShaderStageFlags::FRAGMENT,
                ),
            ],
        ];
        for (index, set_bindings) in bindings.iter().enumerate() {
            let create_info =
                vk::DescriptorSetLayoutCreateInfo::default().bindings(set_bindings.as_slice());
            // SAFETY: Binding slices remain alive for the call and contain no
            // immutable-sampler pointers.
            self.descriptor_sets[index] =
                unsafe { device.create_descriptor_set_layout(&create_info, None) }.map_err(
                    |source| {
                        self.destroy(device);
                        VulkanError::operation("create M2 descriptor set layout", source)
                    },
                )?;
        }
        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(DRAW_PUSH_CONSTANT_BYTES);
        let push_ranges = [push_range];
        let create_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&self.descriptor_sets)
            .push_constant_ranges(&push_ranges);
        // SAFETY: Every descriptor layout is live and all slices remain valid.
        self.handle =
            unsafe { device.create_pipeline_layout(&create_info, None) }.map_err(|source| {
                self.destroy(device);
                VulkanError::operation("create M2 pipeline layout", source)
            })?;
        Ok(())
    }

    /// Returns the initialized common pipeline layout.
    pub(super) const fn handle(&self) -> vk::PipelineLayout {
        self.handle
    }

    /// Returns one initialized descriptor-set layout by fixed ABI index.
    pub(super) fn descriptor_set(&self, index: usize) -> Option<vk::DescriptorSetLayout> {
        self.descriptor_sets.get(index).copied()
    }

    /// Releases the parent layout before its child descriptor layouts.
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

/// Creates one descriptor binding with an explicit single-resource count.
fn descriptor_binding(
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

/// Translates compiled modules and immutable stock material state to Vulkan.
pub(super) fn create_pipeline(
    device: &Device,
    layout: vk::PipelineLayout,
    color_format: vk::Format,
    depth_format: vk::Format,
    material: M2MaterialState,
    program: &M2SpirvProgram,
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
    let binding = vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(M2RenderVertex::BYTE_SIZE as u32)
        .input_rate(vk::VertexInputRate::VERTEX);
    let bindings = [binding];
    let attributes = vertex_attributes();
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
        .cull_mode(if material.cull_enabled() {
            vk::CullModeFlags::BACK
        } else {
            vk::CullModeFlags::NONE
        })
        // Stock M2 indices remain counter-clockwise after the world viewport.
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
    let create_info = vk::GraphicsPipelineCreateInfo::default()
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
    // SAFETY: Every create-info pointer and module/layout handle remains live
    // for the call. Dynamic rendering intentionally supplies no render pass.
    unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), &[create_info], None) }
        .map_err(|(_partial, source)| {
            VulkanError::operation("create M2 graphics pipeline", source)
        })?
        .into_iter()
        .next()
        .ok_or_else(|| {
            VulkanError::operation("create M2 graphics pipeline", "driver returned none")
        })
}

/// Describes the decoder-independent 48-byte M2 vertex ABI exactly.
fn vertex_attributes() -> [vk::VertexInputAttributeDescription; 6] {
    [
        vertex_attribute(0, vk::Format::R32G32B32_SFLOAT, 0),
        vertex_attribute(1, vk::Format::R8G8B8A8_UNORM, 12),
        vertex_attribute(2, vk::Format::R16G16B16A16_UINT, 16),
        vertex_attribute(3, vk::Format::R32G32B32_SFLOAT, 24),
        vertex_attribute(4, vk::Format::R32G32_SFLOAT, 36),
        vertex_attribute(5, vk::Format::R32G32_SFLOAT, 44),
    ]
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

/// Maps the backend-independent stock blend table without changing factors.
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

/// Temporary shader modules destroyed immediately after pipeline creation.
struct ShaderModules<'device> {
    device: &'device Device,
    vertex: vk::ShaderModule,
    fragment: vk::ShaderModule,
}

impl<'device> ShaderModules<'device> {
    /// Creates both modules with cleanup if the fragment module fails.
    fn create(device: &'device Device, program: &M2SpirvProgram) -> Result<Self, VulkanError> {
        let vertex_info = vk::ShaderModuleCreateInfo::default().code(program.vertex_words());
        // SAFETY: shaderc produced aligned native SPIR-V words retained by the program.
        let vertex = unsafe { device.create_shader_module(&vertex_info, None) }
            .map_err(|source| VulkanError::operation("create M2 vertex shader module", source))?;
        let fragment_info = vk::ShaderModuleCreateInfo::default().code(program.fragment_words());
        // SAFETY: Same invariant as the vertex module.
        let fragment = match unsafe { device.create_shader_module(&fragment_info, None) } {
            Ok(module) => module,
            Err(source) => {
                // SAFETY: The vertex module was just created and is not submitted.
                unsafe { device.destroy_shader_module(vertex, None) };
                return Err(VulkanError::operation(
                    "create M2 fragment shader module",
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
    /// Releases modules after the driver has consumed them into a pipeline.
    fn drop(&mut self) {
        // SAFETY: Both modules are live, uniquely owned, and pipeline creation
        // does not retain their handles after returning.
        unsafe {
            self.device.destroy_shader_module(self.fragment, None);
            self.device.destroy_shader_module(self.vertex, None);
        }
    }
}
