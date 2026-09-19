//! Visible material sampling and ordered model draw packets.

use super::super::super::RuntimeTerrainFrameError;
use super::super::super::{
    M2ElementAlphaState, M2MaterialPose, M2MaterialState, M2MaterialUniform,
    M2TransparentDrawIndex, M2TransparentElement, M2TransparentPass, M2TransparentSortKey,
    scene_element_count, section_distance_key,
};
use super::{GeometryContext, GeometryInput, GeometryJob, VisibleGeometryInput};

impl GeometryJob {
    /// Consumes only this job's owned state and immutable frame inputs.
    pub(super) fn prepare_meshes(
        &mut self,
        context: &GeometryContext,
        input: GeometryInput,
        visible: VisibleGeometryInput,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let source = &context.source;
        let clock = input.clock;
        let model_view = input.model_view;
        let bone_pose = &self.pose;
        let VisibleGeometryInput {
            instance_color,
            placement_fog_color,
            light_bank,
            scene_index,
            effect_retiring,
            model_liquid,
            instance_distance,
            instance_identity,
            ..
        } = visible;
        if source.mesh.is_some() && !effect_retiring {
            for (draw_index, resources) in source.draws.iter().enumerate() {
                let Some(resources) = resources else {
                    continue;
                };
                let pose = self
                    .material_poses
                    .get(draw_index)
                    .copied()
                    .flatten()
                    .map_or_else(
                        || M2MaterialPose::sample(&source.model, &source.plan, draw_index, clock),
                        Ok,
                    )?;
                let draw = &source.plan.draws()[draw_index];
                let material_state = M2MaterialState::from_material(draw.material());
                let element_alpha = pose.mesh_color().w * instance_color.w;
                let alpha_state = M2ElementAlphaState::classify(element_alpha);
                if alpha_state == M2ElementAlphaState::Hidden {
                    continue;
                }
                let runtime_alpha_fade = alpha_state == M2ElementAlphaState::Translucent
                    && !material_state.blend_enabled();
                let material = M2MaterialUniform::new(
                    input.transform,
                    pose.texture_transforms(),
                    model_view,
                    pose.mesh_color() * instance_color,
                    placement_fog_color.extend(1.0),
                    glam::Vec4::new(
                        material_state.alpha_reference(instance_color.w),
                        material_state.fog_mode().shader_code(),
                        0.0,
                        0.0,
                    ),
                );
                let template = if runtime_alpha_fade {
                    resources.fade_template.ok_or(
                        RuntimeTerrainFrameError::M2RuntimeFadePipeline {
                            model: source.model.path().clone(),
                            draw_index,
                        },
                    )?
                } else {
                    resources.template
                };
                let prepared = template
                    .instantiate(material, 0, 0)?
                    .with_light_bank(light_bank)
                    .with_scene_index(scene_index);
                if draw.transparent_sort_unit() || alpha_state == M2ElementAlphaState::Translucent {
                    let section_distance = section_distance_key(draw, bone_pose, model_view)?;
                    let primary_distance = if visible.model_distance_sort {
                        instance_distance
                    } else {
                        section_distance
                    };
                    let producer_order = u32::try_from(self.transparent_elements.len())
                        .map_err(|_source| solarity_rendering::VulkanError::M2DrawIndexRange)?;
                    let key = M2TransparentSortKey::new(
                        primary_distance,
                        false,
                        i16::from(draw.batch().priority_plane),
                        section_distance,
                        instance_identity,
                        draw.batch().material_layer,
                    )
                    .with_scene_element(0, producer_order);
                    for (pass, admitted) in [
                        (M2TransparentPass::One, model_liquid.above()),
                        (M2TransparentPass::Two, model_liquid.below()),
                    ] {
                        if !admitted {
                            continue;
                        }
                        let prepared_index = self.visible_draws.len();
                        self.visible_draws
                            .push(prepared.with_liquid_clip_plane(model_liquid.clip_plane(pass)))?;
                        self.transparent_elements.push(M2TransparentElement {
                            pass,
                            key,
                            draw: M2TransparentDrawIndex::Mesh(prepared_index),
                        })?;
                    }
                } else {
                    let scene_order = u32::try_from(scene_element_count(
                        self.visible_draws.len(),
                        self.particle_draws.len(),
                        self.ribbon_draws.len(),
                    )?)
                    .map_err(|_source| solarity_rendering::VulkanError::M2DrawIndexRange)?;
                    self.visible_draws
                        .push(prepared.with_scene_order(scene_order))?;
                }
            }
        } else {
            debug_assert!(effect_retiring || source.draws.iter().all(Option::is_none));
        }
        Ok(())
    }
}
