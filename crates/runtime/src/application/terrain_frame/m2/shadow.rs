//! Original root-unit admission and recursive attached-model shadow packets.

use super::{
    M2GpuPlacement, M2GpuPlacementOwner, M2GpuSource, RuntimeTerrainFrameError,
    UnitSceneRegistration, placement_bounding_sphere, placement_color, placement_mesh_color,
};
use glam::{Mat4, Vec4};
use solarity_rendering::{
    M2AnimationClock, M2MaterialPose, M2MaterialUniform, M2PreparedDraw, M2ShadowMaterial,
    VulkanRenderer, WorldShadowProjection,
};

/// 7BB9D0 admits dynamic unit roots; 834660 recursively visits their attachments.
pub(super) fn admits_root(
    projection: WorldShadowProjection,
    source: &M2GpuSource,
    placement: &M2GpuPlacement,
) -> Result<bool, RuntimeTerrainFrameError> {
    if !matches!(
        placement.owner,
        M2GpuPlacementOwner::PlayerBody { .. }
            | M2GpuPlacementOwner::PlayerMount { .. }
            | M2GpuPlacementOwner::RemotePlayerBody { .. }
            | M2GpuPlacementOwner::RemotePlayerMount { .. }
            | M2GpuPlacementOwner::CreatureBody { .. }
            | M2GpuPlacementOwner::CreatureMount { .. }
    ) {
        return Ok(false);
    }
    let registration = UnitSceneRegistration::new(&source.model, placement.transform)?;
    let (_, radius) = placement_bounding_sphere(&source.model, placement.transform);
    Ok(projection.admits_unit(
        registration.bounds.minimum(),
        registration.bounds.maximum(),
        radius,
    ))
}

/// Samples the same material clock and pose used by the ordinary model queue.
/// The returned packets use the caller's shared palette offset even off screen.
#[allow(clippy::too_many_arguments)]
pub(super) fn append_packets(
    renderer: &VulkanRenderer,
    source: &M2GpuSource,
    placement: &M2GpuPlacement,
    clock: M2AnimationClock,
    model_view: Mat4,
    opacity: f32,
    bone_offset: u32,
    destination: &mut Vec<M2PreparedDraw>,
) -> Result<(), RuntimeTerrainFrameError> {
    let Some(mesh) = source.mesh else {
        return Ok(());
    };
    if placement
        .unit_effect
        .as_ref()
        .is_some_and(|effect| effect.retiring())
    {
        return Ok(());
    }
    let mut instance_color = placement_mesh_color(placement.owner, placement.color);
    if let Some(animation) = &placement.unit_animation {
        instance_color *= placement_color(animation.model_color().to_le_bytes());
    }
    instance_color.w *= opacity;
    for (draw_index, resources) in source.draws.iter().enumerate() {
        let Some(resources) = resources else {
            continue;
        };
        let pose = M2MaterialPose::sample(&source.model, &source.plan, draw_index, clock)?;
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
            placement.transform,
            pose.texture_transforms(),
            model_view,
            color,
            Vec4::ZERO,
            Vec4::ZERO,
        );
        destination.push(renderer.prepare_m2_draw(
            mesh,
            resources.pipeline,
            resources.texture_set,
            &source.plan,
            draw_index,
            false,
            material,
            bone_offset,
            0,
        )?);
    }
    Ok(())
}
