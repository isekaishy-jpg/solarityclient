//! The six ShadowMap/ShadowMapSL skeletal and alpha-test permutations.

#![allow(unsafe_code)]

use crate::device::VulkanError;
use crate::{M2RenderVertex, WorldModelRenderVertex};
use ash::{Device, vk};

/// Owns the caster layout and pipelines for one rendering device.
#[derive(Default)]
pub(in crate::device) struct ShadowPipelines {
    scene_layout: vk::DescriptorSetLayout,
    layout: vk::PipelineLayout,
    handles: [[vk::Pipeline; 2]; 3],
    wmo_layout: vk::PipelineLayout,
    wmo_handles: [vk::Pipeline; 2],
}

impl ShadowPipelines {
    /// Creates the closed caster domain once, borrowing compatible M2 set layouts.
    pub(in crate::device) fn ensure(
        &mut self,
        device: &Device,
        m2_layouts: [vk::DescriptorSetLayout; 3],
        wmo_layouts: [vk::DescriptorSetLayout; 2],
        depth_format: vk::Format,
    ) -> Result<(), VulkanError> {
        if self.layout != vk::PipelineLayout::null() {
            return Ok(());
        }
        let result = (|| {
            let binding = [vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX)];
            let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&binding);
            // SAFETY: The single binding has no borrowed immutable sampler.
            self.scene_layout = unsafe { device.create_descriptor_set_layout(&info, None) }
                .map_err(|source| {
                    VulkanError::operation("create shadow caster scene layout", source)
                })?;
            let layouts = [
                self.scene_layout,
                m2_layouts[0],
                m2_layouts[1],
                m2_layouts[2],
            ];
            let ranges = [vk::PushConstantRange::default()
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
                .size(16)];
            let info = vk::PipelineLayoutCreateInfo::default()
                .set_layouts(&layouts)
                .push_constant_ranges(&ranges);
            // SAFETY: All descriptor layouts remain live through renderer teardown.
            self.layout =
                unsafe { device.create_pipeline_layout(&info, None) }.map_err(|source| {
                    VulkanError::operation("create shadow caster pipeline layout", source)
                })?;
            for bone_class in 0..3 {
                for alpha in 0..2 {
                    self.handles[bone_class][alpha] = create_pipeline(
                        device,
                        self.layout,
                        depth_format,
                        bone_class as u32,
                        alpha as u32,
                        CasterGeometry::M2,
                    )?;
                }
            }
            let layouts = [self.scene_layout, wmo_layouts[0], wmo_layouts[1]];
            let info = vk::PipelineLayoutCreateInfo::default().set_layouts(&layouts);
            // SAFETY: Shared WMO material and texture layouts outlive these pipelines.
            self.wmo_layout =
                unsafe { device.create_pipeline_layout(&info, None) }.map_err(|source| {
                    VulkanError::operation("create WMO shadow caster layout", source)
                })?;
            for alpha in 0..2 {
                self.wmo_handles[alpha] = create_pipeline(
                    device,
                    self.wmo_layout,
                    depth_format,
                    0,
                    alpha as u32,
                    CasterGeometry::Wmo,
                )?;
            }
            Ok(())
        })();
        if result.is_err() {
            self.destroy(device);
        }
        result
    }

    pub(in crate::device) const fn scene_layout(&self) -> vk::DescriptorSetLayout {
        self.scene_layout
    }
    pub(in crate::device) const fn layout(&self) -> vk::PipelineLayout {
        self.layout
    }
    pub(in crate::device) fn wmo(&self, alpha: bool) -> (vk::Pipeline, vk::PipelineLayout) {
        (self.wmo_handles[usize::from(alpha)], self.wmo_layout)
    }

    /// Resolves a validated stock influence class and material queue.
    pub(in crate::device) fn raw(&self, bone_class: usize, alpha: bool) -> Option<vk::Pipeline> {
        self.handles
            .get(bone_class)
            .map(|entry| entry[usize::from(alpha)])
            .filter(|handle| *handle != vk::Pipeline::null())
    }

    /// Releases owned layouts after all frame-slot work has retired.
    pub(in crate::device) fn destroy(&mut self, device: &Device) {
        // SAFETY: The renderer waits for its slots before destroying shared pipelines.
        unsafe {
            for handle in &mut self.wmo_handles {
                if *handle != vk::Pipeline::null() {
                    device.destroy_pipeline(*handle, None);
                }
                *handle = vk::Pipeline::null();
            }
            if self.wmo_layout != vk::PipelineLayout::null() {
                device.destroy_pipeline_layout(self.wmo_layout, None);
            }
            for pair in &mut self.handles {
                for handle in pair {
                    if *handle != vk::Pipeline::null() {
                        device.destroy_pipeline(*handle, None);
                    }
                    *handle = vk::Pipeline::null();
                }
            }
            if self.layout != vk::PipelineLayout::null() {
                device.destroy_pipeline_layout(self.layout, None);
            }
            if self.scene_layout != vk::DescriptorSetLayout::null() {
                device.destroy_descriptor_set_layout(self.scene_layout, None);
            }
        }
        self.layout = vk::PipelineLayout::null();
        self.wmo_layout = vk::PipelineLayout::null();
        self.scene_layout = vk::DescriptorSetLayout::null();
    }
}

/// Builds native two-sided, depth-writing silhouettes with direct floating depth.
#[derive(Clone, Copy)]
enum CasterGeometry {
    M2,
    Wmo,
}

fn create_pipeline(
    device: &Device,
    layout: vk::PipelineLayout,
    depth_format: vk::Format,
    bone_class: u32,
    alpha_test: u32,
    geometry: CasterGeometry,
) -> Result<vk::Pipeline, VulkanError> {
    let modules = ShaderModules::new(device, geometry)?;
    let vertex_entries = [vk::SpecializationMapEntry {
        constant_id: 0,
        offset: 0,
        size: 4,
    }];
    let fragment_entries = [vk::SpecializationMapEntry {
        constant_id: 1,
        offset: 0,
        size: 4,
    }];
    let vertex_data = bone_class.to_le_bytes();
    let fragment_data = alpha_test.to_le_bytes();
    let vertex_specialization = vk::SpecializationInfo::default()
        .map_entries(&vertex_entries)
        .data(&vertex_data);
    let fragment_specialization = vk::SpecializationInfo::default()
        .map_entries(&fragment_entries)
        .data(&fragment_data);
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
        .stride(match geometry {
            CasterGeometry::M2 => M2RenderVertex::BYTE_SIZE,
            CasterGeometry::Wmo => WorldModelRenderVertex::BYTE_SIZE,
        } as u32)
        .input_rate(vk::VertexInputRate::VERTEX)];
    let m2_attributes = [
        (0, vk::Format::R32G32B32_SFLOAT, 0),
        (1, vk::Format::R8G8B8A8_UNORM, 12),
        (2, vk::Format::R16G16B16A16_UINT, 16),
        (4, vk::Format::R32G32_SFLOAT, 36),
    ];
    let wmo_attributes = [
        (0, vk::Format::R32G32B32_SFLOAT, 0),
        (4, vk::Format::R32G32_SFLOAT, 24),
    ];
    let attributes: Vec<_> = match geometry {
        CasterGeometry::M2 => m2_attributes.as_slice(),
        CasterGeometry::Wmo => wmo_attributes.as_slice(),
    }
    .iter()
    .copied()
    .map(|(location, format, offset)| {
        vk::VertexInputAttributeDescription::default()
            .location(location)
            .binding(0)
            .format(format)
            .offset(offset)
    })
    .collect();
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&bindings)
        .vertex_attribute_descriptions(&attributes);
    let assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    // Original 7BBC50 explicitly disables GX culling (state 17), including mirrored equipment.
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
    let attachments = [vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::R)];
    let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
    let formats = [vk::Format::R32_SFLOAT];
    let mut rendering = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(&formats)
        .depth_attachment_format(depth_format)
        .stencil_attachment_format(depth_format);
    let info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&assembly)
        .viewport_state(&viewport)
        .rasterization_state(&rasterization)
        .multisample_state(&multisample)
        .depth_stencil_state(&depth)
        .color_blend_state(&blend)
        .dynamic_state(&dynamic)
        .layout(layout)
        .push_next(&mut rendering);
    // SAFETY: Modules, layout and all borrowed creation slices remain live.
    match unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), &[info], None) } {
        Ok(handles) => handles.into_iter().next().ok_or_else(|| {
            VulkanError::operation("create shadow caster pipeline", "driver returned none")
        }),
        Err((partial, source)) => {
            // SAFETY: Partial results are newly created and have never been submitted.
            for handle in partial {
                unsafe { device.destroy_pipeline(handle, None) };
            }
            Err(VulkanError::operation(
                "create shadow caster pipeline",
                source,
            ))
        }
    }
}

/// Temporary modules survive pipeline creation and cover every failure path.
struct ShaderModules<'a> {
    device: &'a Device,
    vertex: vk::ShaderModule,
    fragment: vk::ShaderModule,
}

impl<'a> ShaderModules<'a> {
    /// Loads build-generated native shader translations into temporary driver objects.
    fn new(device: &'a Device, geometry: CasterGeometry) -> Result<Self, VulkanError> {
        let mut result = Self {
            device,
            vertex: vk::ShaderModule::null(),
            fragment: vk::ShaderModule::null(),
        };
        for (destination, bytes) in [
            (
                &mut result.vertex,
                match geometry {
                    CasterGeometry::M2 => {
                        include_bytes!(concat!(env!("OUT_DIR"), "/shadow.vert.spv")).as_slice()
                    }
                    CasterGeometry::Wmo => {
                        include_bytes!(concat!(env!("OUT_DIR"), "/shadow-wmo.vert.spv")).as_slice()
                    }
                },
            ),
            (
                &mut result.fragment,
                match geometry {
                    CasterGeometry::M2 => {
                        include_bytes!(concat!(env!("OUT_DIR"), "/shadow.frag.spv")).as_slice()
                    }
                    CasterGeometry::Wmo => {
                        include_bytes!(concat!(env!("OUT_DIR"), "/shadow-wmo.frag.spv")).as_slice()
                    }
                },
            ),
        ] {
            let words = bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|word| u32::from_le_bytes(*word))
                .collect::<Vec<_>>();
            let info = vk::ShaderModuleCreateInfo::default().code(&words);
            // SAFETY: Cargo generated valid aligned SPIR-V; the words live through the call.
            *destination =
                unsafe { device.create_shader_module(&info, None) }.map_err(|source| {
                    VulkanError::operation("create shadow caster shader module", source)
                })?;
        }
        Ok(result)
    }
}

impl Drop for ShaderModules<'_> {
    fn drop(&mut self) {
        // SAFETY: Modules are uniquely owned and no longer needed by completed pipeline creation.
        unsafe {
            if self.fragment != vk::ShaderModule::null() {
                self.device.destroy_shader_module(self.fragment, None);
            }
            if self.vertex != vk::ShaderModule::null() {
                self.device.destroy_shader_module(self.vertex, None);
            }
        }
    }
}
