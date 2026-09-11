//! Dedicated detail shader and native alpha/blend/depth state ownership.

#![allow(unsafe_code)]

use ash::{Device, vk};

use crate::VulkanError;

/// Renderer-lifetime ownership of baked/primary-shadow detail pipelines and their ABI.
#[derive(Default)]
pub(in crate::device) struct DetailPipeline {
    pipelines: [vk::Pipeline; 2],
    layout: vk::PipelineLayout,
    descriptor: vk::DescriptorSetLayout,
    scene: vk::DescriptorSetLayout,
    shadow: vk::DescriptorSetLayout,
}

impl DetailPipeline {
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

    /// Keeps partial handles in the owner so prepare can unwind any failed stage.
    fn create(
        &mut self,
        device: &Device,
        color: vk::Format,
        depth: vk::Format,
    ) -> Result<(), VulkanError> {
        let bindings = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)];
        let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
        // SAFETY: Binding storage remains live and has no immutable samplers.
        self.descriptor = unsafe { device.create_descriptor_set_layout(&info, None) }
            .map_err(|source| VulkanError::operation("create detail descriptor layout", source))?;
        let bindings = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)];
        let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
        // SAFETY: This scene layout exactly matches the existing terrain frame descriptor ABI.
        self.scene = unsafe { device.create_descriptor_set_layout(&info, None) }
            .map_err(|source| VulkanError::operation("create detail scene layout", source))?;
        let bindings = crate::device::vulkan_shadow::receiver_bindings();
        let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
        // SAFETY: This is the slot-retained terrain/WMO receiver descriptor ABI.
        self.shadow = unsafe { device.create_descriptor_set_layout(&info, None) }
            .map_err(|source| VulkanError::operation("create detail shadow layout", source))?;
        let sets = [self.scene, self.descriptor, self.shadow];
        let pushes = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .size(32)];
        let info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&sets)
            .push_constant_ranges(&pushes);
        // SAFETY: All descriptor layouts are live and the 32-byte push bank fits the device.
        self.layout = unsafe { device.create_pipeline_layout(&info, None) }
            .map_err(|source| VulkanError::operation("create detail pipeline layout", source))?;
        for (index, has_primary_shadow) in [false, true].into_iter().enumerate() {
            let modules = ShaderModules::create(device, has_primary_shadow)?;
            self.pipelines[index] = create_pipeline(device, self.layout, color, depth, &modules)?;
        }
        Ok(())
    }

    pub(in crate::device) const fn descriptor_layout(&self) -> vk::DescriptorSetLayout {
        self.descriptor
    }

    pub(in crate::device) const fn raw(
        &self,
        has_primary_shadow: bool,
    ) -> (vk::Pipeline, vk::PipelineLayout) {
        (self.pipelines[has_primary_shadow as usize], self.layout)
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
            if self.scene != vk::DescriptorSetLayout::null() {
                device.destroy_descriptor_set_layout(self.scene, None);
            }
            if self.descriptor != vk::DescriptorSetLayout::null() {
                device.destroy_descriptor_set_layout(self.descriptor, None);
            }
            if self.shadow != vk::DescriptorSetLayout::null() {
                device.destroy_descriptor_set_layout(self.shadow, None);
            }
        }
        *self = Self::default();
    }
}

/// Defines the dedicated native vertex, alpha-blend, and depth-write contract.
fn create_pipeline(
    device: &Device,
    layout: vk::PipelineLayout,
    color_format: vk::Format,
    depth_format: vk::Format,
    modules: &ShaderModules<'_>,
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
        .stride(36)
        .input_rate(vk::VertexInputRate::VERTEX)];
    let attributes = [
        attribute(0, vk::Format::R32G32B32_SFLOAT, 0),
        attribute(1, vk::Format::R32G32B32_SFLOAT, 12),
        attribute(2, vk::Format::R8G8B8A8_UNORM, 24),
        attribute(3, vk::Format::R32G32_SFLOAT, 28),
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
        .depth_write_enable(true)
        .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL);
    // 7B2D30 selects GX blend 2, no culling, and enabled depth writes.
    let destination = vk::BlendFactor::ONE_MINUS_SRC_ALPHA;
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
            VulkanError::operation("create detail graphics pipeline", source)
        })?
        .into_iter()
        .next()
        .ok_or_else(|| {
            VulkanError::operation("create detail graphics pipeline", "driver returned none")
        })
}

/// Describes one field of the native 36-byte detail vertex without Rust ABI assumptions.
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
    /// Holds both compiled stages only until pipeline creation has consumed them.
    fn create(device: &'a Device, has_primary_shadow: bool) -> Result<Self, VulkanError> {
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
                .map_err(|source| VulkanError::operation("create detail shader module", source))
        };
        let vertex_bytes: &[u8] = if has_primary_shadow {
            include_bytes!(concat!(env!("OUT_DIR"), "/detail-shadow.vert.spv"))
        } else {
            include_bytes!(concat!(env!("OUT_DIR"), "/detail.vert.spv"))
        };
        let fragment_bytes: &[u8] = if has_primary_shadow {
            include_bytes!(concat!(env!("OUT_DIR"), "/detail-shadow.frag.spv"))
        } else {
            include_bytes!(concat!(env!("OUT_DIR"), "/detail.frag.spv"))
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
