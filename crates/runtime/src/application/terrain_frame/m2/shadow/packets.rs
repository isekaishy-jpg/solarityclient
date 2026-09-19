//! Shadow packet sampling accepts only frozen scalar inputs and immutable resources.

use super::super::{M2GpuSource, RuntimeTerrainFrameError};
use glam::{Mat4, Vec4};
use solarity_rendering::{
    M2AnimationClock, M2MaterialPose, M2MaterialUniform, M2PreparedDraw, M2ShadowMaterial,
};

/// Captured after ordered animation callbacks; no Rc or live placement crosses workers.
#[derive(Clone, Copy)]
pub(in super::super) struct ShadowInput {
    pub transform: Mat4,
    pub model_view: Mat4,
    pub clock: M2AnimationClock,
    pub instance_color: Vec4,
    pub retiring: bool,
    pub bone_offset: u32,
}

/// Samples the same material clock and pose used by the ordinary model queue.
/// The returned packets use the caller's shared palette offset even off screen.
pub(in super::super) fn append_packets(
    source: &M2GpuSource,
    input: ShadowInput,
    material_poses: &mut Vec<Option<M2MaterialPose>>,
    mut destination: impl FnMut(M2PreparedDraw) -> Result<(), RuntimeTerrainFrameError>,
) -> Result<(), RuntimeTerrainFrameError> {
    let ShadowInput {
        transform,
        model_view,
        clock,
        instance_color,
        retiring,
        bone_offset,
    } = input;
    if source.mesh.is_none() {
        return Ok(());
    }
    if retiring {
        return Ok(());
    }
    material_poses.resize(source.draws.len(), None);
    for (draw_index, resources) in source.draws.iter().enumerate() {
        let Some(resources) = resources else {
            continue;
        };
        let pose = M2MaterialPose::sample(&source.model, &source.plan, draw_index, clock)?;
        material_poses[draw_index] = Some(pose);
        let draw = &source.plan.draws()[draw_index];
        let color = pose.mesh_color() * instance_color;
        if M2ShadowMaterial::select(
            draw.batch(),
            draw.material().flags(),
            draw.material().blend_mode(),
            color.w,
        )
        .is_none()
        {
            continue;
        }
        // ShadowMapSL uses raw UV0 and its own fixed alpha threshold. These
        // ordinary material fields preserve the shared descriptor ABI.
        let material = M2MaterialUniform::new(
            transform,
            pose.texture_transforms(),
            model_view,
            color,
            Vec4::ZERO,
            Vec4::ZERO,
        );
        destination(resources.template.instantiate(material, bone_offset, 0)?)?;
    }
    Ok(())
}
