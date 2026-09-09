//! Native WDL opaque face banks (`7D5E70`) and their descriptor-free shader ABI.

#![allow(unsafe_code)]

use ash::{Device, vk};

use crate::VulkanError;

/// Renderer-lifetime ownership of both WDL face-bank pipelines and their push ABI.
#[derive(Default)]
pub(in crate::device) struct LowDetailPipelines {
    pipelines: [vk::Pipeline; 2],
    layout: vk::PipelineLayout,
}

impl LowDetailPipelines {
    /// Creates the complete pipeline before any frame publishes descriptors.
    pub(in crate::device) fn prepare(
        &mut self,
        device: &Device,
        color: vk::Format,
        depth: vk::Format,
    ) -> Result<(), VulkanError> {
        if self.pipelines[0] != vk::Pipeline::null() {
            return Ok(());
        }
        let result = self.create(device, color, depth);
        if result.is_err() {
            self.destroy(device);
        }
        result
    }

    /// Creates both face-bank pipelines transactionally.
    fn create(
        &mut self,
        device: &Device,
        color: vk::Format,
        depth: vk::Format,
    ) -> Result<(), VulkanError> {
        let pushes = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .size(96)];
        let info = vk::PipelineLayoutCreateInfo::default().push_constant_ranges(&pushes);
        // SAFETY: The 96-byte push bank fits Vulkan's required minimum.
        self.layout = unsafe { device.create_pipeline_layout(&info, None) }.map_err(|source| {
            VulkanError::operation("create low-detail pipeline layout", source)
        })?;
        let modules = ShaderModules::create(device)?;
        for (index, culling) in [vk::CullModeFlags::NONE, vk::CullModeFlags::BACK]
            .into_iter()
            .enumerate()
        {
            self.pipelines[index] =
                create_pipeline(device, self.layout, color, depth, &modules, culling)?;
        }
        Ok(())
    }

    pub(in crate::device) const fn raw(&self, bank: usize) -> (vk::Pipeline, vk::PipelineLayout) {
        (self.pipelines[bank], self.layout)
    }

    /// Releases the owned objects after every world-frame slot has retired.
    pub(in crate::device) fn destroy(&mut self, device: &Device) {
        // SAFETY: Renderer teardown or failed creation excludes all GPU users.
        unsafe {
            for pipeline in self.pipelines {
                if pipeline != vk::Pipeline::null() {
                    device.destroy_pipeline(pipeline, None);
                }
            }
            if self.layout != vk::PipelineLayout::null() {
                device.destroy_pipeline_layout(self.layout, None);
            }
        }
        *self = Self::default();
    }
}

/// Materializes the fixed WDL triangle/depth/color state.
fn create_pipeline(
    device: &Device,
    layout: vk::PipelineLayout,
    color_format: vk::Format,
    depth_format: vk::Format,
    modules: &ShaderModules<'_>,
    culling: vk::CullModeFlags,
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
        .stride(12)
        .input_rate(vk::VertexInputRate::VERTEX)];
    let attributes = [attribute(0, vk::Format::R32G32B32_SFLOAT, 0)];
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
        .cull_mode(culling)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let depth = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(true)
        .depth_write_enable(true)
        .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL);
    let attachments = [vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(false)
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
            VulkanError::operation("create low-detail graphics pipeline", source)
        })?
        .into_iter()
        .next()
        .ok_or_else(|| {
            VulkanError::operation(
                "create low-detail graphics pipeline",
                "driver returned none",
            )
        })
}

/// Declares the sole world-position input at binding zero.
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
    /// Validates embedded SPIR-V and releases partial shader creation.
    fn create(device: &'a Device) -> Result<Self, VulkanError> {
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
                .map_err(|source| VulkanError::operation("create low-detail shader module", source))
        };
        let vertex_bytes = include_bytes!(concat!(env!("OUT_DIR"), "/low-detail.vert.spv"));
        let fragment_bytes = include_bytes!(concat!(env!("OUT_DIR"), "/low-detail.frag.spv"));
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
