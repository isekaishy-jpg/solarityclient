//! Visible ribbon material passes retain stock emitter and scene ordering.

use super::super::super::RuntimeTerrainFrameError;
use super::super::super::{
    M2EffectOrder, M2ElementAlphaState, M2MaterialState, M2RibbonMeshPlan, M2RibbonPose,
    M2TransparentDrawIndex, M2TransparentElement, M2TransparentSortKey, scene_element_count,
};
use super::{GeometryContext, GeometryInput, GeometryJob, VisibleGeometryInput};

impl GeometryJob {
    /// Consumes only this job's owned state and immutable frame inputs.
    pub(super) fn prepare_ribbon_draws(
        &mut self,
        context: &GeometryContext,
        input: GeometryInput,
        visible: VisibleGeometryInput,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let source = &context.source;
        let clock = input.clock;
        let VisibleGeometryInput {
            instance_color,
            light_bank,
            scene_index,
            effect_retiring,
            model_liquid,
            instance_distance,
            instance_identity,
            ..
        } = visible;
        for (ribbon_index, ((emitter, trail), passes)) in source
            .model
            .animations()
            .ribbons()
            .iter()
            .zip(&self.ribbons)
            .zip(&source.ribbons)
            .enumerate()
        {
            // 6F87C0 removes normal model submission; 828A00 retains only
            // the particle pass while model bit 0x400 remains live.
            if effect_retiring {
                continue;
            }
            let Some(root_pass) = passes.first() else {
                continue;
            };
            if trail.sections().len() < 2 {
                continue;
            }
            let first_vertex = u32::try_from(self.ribbon_vertices.len())
                .map_err(|_source| solarity_rendering::VulkanError::M2RibbonDrawVertexRange)?;
            let vertex_count =
                M2RibbonMeshPlan::append(emitter, trail, &mut self.ribbon_vertices.writer())?;
            let ribbon_alpha = M2RibbonPose::sample(source.model.animations(), emitter, clock)?
                .color()
                .w
                * instance_color.w;
            // 821A20 registers one type-3 scene element for the emitter.
            // Its first material and owner/track alpha classify the whole
            // ribbon; 820F40/980B70 then submit every pass consecutively.
            let effect_order = u32::try_from(self.transparent_elements.len())
                .map_err(|_source| solarity_rendering::VulkanError::M2RibbonDrawVertexRange)?;
            let first_draw = self.ribbon_draws.len();
            let opaque = !M2MaterialState::from_material(root_pass.material).blend_enabled()
                && M2ElementAlphaState::classify(ribbon_alpha) == M2ElementAlphaState::Authored;
            let order = scene_element_count(
                self.visible_draws.len(),
                self.particle_draws.len(),
                first_draw,
            )?;
            for (pass_index, pass) in passes.iter().enumerate() {
                let prepared = pass
                    .template
                    .prepare(
                        pass.material,
                        M2EffectOrder::new(emitter.priority_plane(), effect_order),
                        first_vertex,
                        vertex_count,
                    )?
                    .with_light_bank(light_bank)
                    .with_scene_index(scene_index)
                    .with_first_material_pass(pass_index == 0);
                self.ribbon_draws.push(if opaque {
                    prepared.with_scene_order(
                        u32::try_from(order)
                            .map_err(|_| solarity_rendering::VulkanError::M2DrawIndexRange)?,
                    )
                } else {
                    prepared
                })?;
            }
            if !opaque {
                self.transparent_elements.push(M2TransparentElement {
                    pass: model_liquid.ribbon_pass(),
                    key: M2TransparentSortKey::new(
                        instance_distance,
                        false,
                        emitter.priority_plane(),
                        instance_distance,
                        instance_identity,
                        0,
                    )
                    .with_scene_element(3, effect_order),
                    draw: M2TransparentDrawIndex::Ribbon {
                        first: first_draw,
                        count: passes.len(),
                    },
                })?;
            }
            tracing::trace!(
                model = %source.model.path(),
                ribbon_index,
                vertex_count,
                pass_count = passes.len(),
                "placement-local ribbon entered unified world frame"
            );
        }
        Ok(())
    }
}
