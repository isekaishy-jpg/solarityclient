//! Resolve immutable caster commands before transferring exclusive recording ownership.

use super::super::command::{RecordContext, dynamic_offset};
use super::types::{CasterCommand, ShadowJob, ShadowPass};
use crate::{M2PreparedDraw, M2ShadowMaterial, VulkanError};
use ash::vk;
use solarity_cpu::{CpuStorageBudget, CpuStorageClass, CpuStorageKind};

impl ShadowJob {
    /// Captures one original pass without retaining references to renderer registries.
    pub(super) fn capture(
        &mut self,
        context: &RecordContext<'_>,
        pass_index: usize,
        command: vk::CommandBuffer,
        budget: &CpuStorageBudget,
    ) -> Result<bool, VulkanError> {
        self.draws.clear();
        self.result = None;
        self.pass = None;
        self.query_pool = None;
        self.initialize.fill(vk::Image::null());
        let Some(primary) = context.shadow_frame else {
            return Ok(false);
        };
        let (pass, caster_set, mask) = if pass_index == 0 {
            let resources = context.shadow_resources;
            let size = resources.size();
            self.query_pool = context.gpu_queries;
            if context.environment_frame.is_some() && !context.environment_images.initialized() {
                for (slot, image) in self
                    .initialize
                    .iter_mut()
                    .zip(context.environment_images.color_images())
                {
                    *slot = image;
                }
            }
            (
                ShadowPass {
                    color_image: resources.color_image(),
                    color_view: resources.color_view(),
                    depth_image: resources.depth_image(),
                    depth_view: resources.depth_view(),
                    rect: vk::Rect2D {
                        offset: vk::Offset2D::default(),
                        extent: vk::Extent2D {
                            width: size,
                            height: size,
                        },
                    },
                    primary: true,
                },
                resources.caster_set(),
                8,
            )
        } else {
            let index = pass_index - 1;
            let Some(pass) = context
                .environment_frame
                .and_then(|frame| frame.passes()[index])
            else {
                return Ok(false);
            };
            let update = pass.update();
            let (color_image, color_view) = context.environment_images.color(index, update.buffer);
            let (depth_image, depth_view) = context.environment_images.depth();
            let [x, y, width, height] = update.pixel_viewport(pass.projection().texture_size());
            (
                ShadowPass {
                    color_image,
                    color_view,
                    depth_image,
                    depth_view,
                    rect: vk::Rect2D {
                        offset: vk::Offset2D {
                            x: x as i32,
                            y: y as i32,
                        },
                        extent: vk::Extent2D { width, height },
                    },
                    primary: false,
                },
                context.shadow_resources.environment_caster_set(index),
                1 << index,
            )
        };
        let maximum = context
            .environment_frame
            .map_or(Some(0), |frame| {
                frame
                    .wmo_casters()
                    .len()
                    .checked_add(frame.m2_casters().len())
            })
            .and_then(|count| {
                count.checked_add(if pass.primary {
                    primary.casters().len()
                } else {
                    0
                })
            })
            .ok_or(VulkanError::WorldFrameCapacity)?;
        self.draws.reserve(
            budget,
            CpuStorageClass::Frame,
            CpuStorageKind::Result,
            maximum,
        )?;
        self.device = Some(context.device.clone());
        self.command = command;
        self.pass = Some(pass);
        // Preserve 7BBC50: scenery WMO, scenery M2, then primary unit silhouettes.
        if let Some(frame) = context.environment_frame {
            for (index, caster) in frame.wmo_casters().iter().enumerate() {
                if caster.maps & mask == 0 {
                    continue;
                }
                let draw = &caster.draw;
                let (pipeline, layout) = context
                    .shadow_pipeline
                    .wmo(caster.blend_mode == solarity_asset::WorldModelBlendMode::AlphaKey);
                let (vertex, indices) = context
                    .world_model_meshes
                    .buffers(draw.mesh())
                    .ok_or(VulkanError::UnknownWorldModelMeshHandle)?;
                let texture = context
                    .world_model_texture_sets
                    .raw(draw.texture_set())
                    .ok_or(VulkanError::UnknownWorldModelTextureSetHandle)?;
                let offset = dynamic_offset(
                    context.world_model_draws.len() + index,
                    context.world_model_material_stride,
                )?;
                let range = draw.index_range();
                self.draws.push(CasterCommand {
                    pipeline,
                    layout,
                    vertex,
                    indices,
                    sets: [
                        caster_set,
                        context.frame_sets[2],
                        texture,
                        vk::DescriptorSet::null(),
                    ],
                    set_count: 3,
                    dynamic_offset: offset,
                    index_type: vk::IndexType::UINT32,
                    first_index: range[0],
                    index_count: range[1],
                    first_instance: 0,
                    instance_count: 1,
                    push_constants: None,
                })?;
            }
            let material_base = context.m2_draws.len()
                + context.sky_models.map_or(0, |frame| frame.draw_count())
                + primary.casters().len();
            let casters = frame.m2_casters();
            let mut index = 0;
            while let Some(caster) = casters.get(index) {
                if caster.maps & mask == 0 {
                    index += 1;
                    continue;
                }
                let count = 1 + casters[index + 1..]
                    .iter()
                    .take_while(|next| {
                        next.maps & mask != 0 && caster.draw.can_instance_shadow_with(next.draw)
                    })
                    .count();
                if let Some(draw) = m2_command(
                    context,
                    material_base + index,
                    &caster.draw,
                    count,
                    caster_set,
                )? {
                    self.draws.push(draw)?;
                }
                index += count;
            }
        }
        if pass.primary {
            let material_base =
                context.m2_draws.len() + context.sky_models.map_or(0, |frame| frame.draw_count());
            for (index, draw) in primary.casters().iter().enumerate() {
                if let Some(draw) = m2_command(context, material_base + index, draw, 1, caster_set)?
                {
                    self.draws.push(draw)?;
                }
            }
        }
        Ok(true)
    }
}

/// Validates the same joins as the former direct caster recorder, with no full packet copy.
fn m2_command(
    context: &RecordContext<'_>,
    material_index: usize,
    draw: &M2PreparedDraw,
    count: usize,
    caster_set: vk::DescriptorSet,
) -> Result<Option<CasterCommand>, VulkanError> {
    let Some(material) = draw.shadow_material() else {
        return Ok(None);
    };
    let info = context
        .m2_pipelines
        .info(draw.pipeline())
        .ok_or(VulkanError::UnknownM2PipelineHandle)?;
    let bone_class = info.permutation().vertex_index() / 10 % 3;
    let pipeline = context
        .shadow_pipeline
        .raw(bone_class, material == M2ShadowMaterial::AlphaTest)
        .ok_or(VulkanError::M2ShadowResourcesUnavailable)?;
    let (vertex, indices) = context
        .m2_meshes
        .buffers(draw.mesh())
        .ok_or(VulkanError::UnknownM2MeshHandle)?;
    let texture = context
        .m2_texture_sets
        .raw(draw.texture_set())
        .ok_or(VulkanError::UnknownM2TextureSetHandle)?;
    Ok(Some(CasterCommand {
        pipeline,
        layout: context.shadow_pipeline.layout(),
        vertex,
        indices,
        sets: [
            caster_set,
            context.frame_sets[6],
            context.frame_sets[7],
            texture,
        ],
        set_count: 4,
        dynamic_offset: 0,
        index_type: vk::IndexType::UINT16,
        first_index: draw.first_index(),
        index_count: draw.index_count(),
        first_instance: u32::try_from(material_index)
            .map_err(|_| VulkanError::WorldFrameCapacity)?,
        instance_count: u32::try_from(count).map_err(|_| VulkanError::WorldFrameCapacity)?,
        push_constants: Some(draw.push_constants().to_bytes()),
    }))
}
