//! Original root-unit admission and recursive attached-model shadow packets.

use super::super::shadow::{ModelShadowKind, SceneryShadowQueries, WorldShadowAdmission};
use super::distance::SceneryDistance;
use super::{
    M2GpuPlacement, M2GpuPlacementOwner, M2GpuSource, RuntimeTerrainFrameError,
    UnitSceneRegistration, placement_bounding_sphere, placement_color, placement_mesh_color,
};
use crate::application::terrain_coordinator::m2_residency::ResidentM2Owner;
use glam::{Mat4, Vec3, Vec4};
use solarity_rendering::{
    M2AnimationClock, M2MaterialPose, M2MaterialUniform, M2PreparedDraw, M2ShadowMaterial,
    VulkanRenderer, WorldShadowProjection,
};

/// 7BB9D0 admits dynamic unit roots; 834660 recursively visits their attachments.
pub(super) fn admits_root(
    projection: WorldShadowProjection,
    source: &M2GpuSource,
    placement: &M2GpuPlacement,
    environment: Option<&WorldShadowAdmission>,
) -> Result<bool, RuntimeTerrainFrameError> {
    if !matches!(
        placement
            .retirement
            .as_ref()
            .map_or(placement.owner, |retired| retired.original_owner),
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
    if let Some(environment) = environment {
        return Ok(environment.admits_primary_unit(registration.bounds, radius));
    }
    Ok(projection.admits_unit(
        registration.bounds.minimum(),
        registration.bounds.maximum(),
        radius,
    ))
}

/// Classifies the original root registration, with attachment inheritance left
/// to the caller. Ordinary scenery must pass the fade-start distance cutoff.
pub(super) fn environment_maps(
    queries: SceneryShadowQueries<'_>,
    source: &M2GpuSource,
    placement: &M2GpuPlacement,
    camera: Vec3,
    detail: f32,
) -> Result<u8, RuntimeTerrainFrameError> {
    if !placement.placement_valid
        || placement
            .entity_opacity
            .as_ref()
            .is_some_and(|owner| owner.hidden())
    {
        return Ok(0);
    }
    let owner = placement
        .retirement
        .as_ref()
        .map_or(placement.owner, |retired| retired.original_owner);
    let kind = match owner {
        M2GpuPlacementOwner::PlayerBody { .. }
        | M2GpuPlacementOwner::PlayerMount { .. }
        | M2GpuPlacementOwner::RemotePlayerBody { .. }
        | M2GpuPlacementOwner::RemotePlayerMount { .. }
        | M2GpuPlacementOwner::CreatureBody { .. }
        | M2GpuPlacementOwner::CreatureMount { .. } => ModelShadowKind::Unit,
        M2GpuPlacementOwner::Static(
            ResidentM2Owner::TerrainDoodad { .. } | ResidentM2Owner::WorldModelDoodad { .. },
        ) => {
            if source.animated_shadow_caster {
                ModelShadowKind::AnimatedScenery
            } else {
                ModelShadowKind::StaticScenery
            }
        }
        M2GpuPlacementOwner::GameObject { .. } => {
            if source.animated_shadow_caster {
                ModelShadowKind::AnimatedGameObject
            } else {
                ModelShadowKind::StaticGameObject
            }
        }
        M2GpuPlacementOwner::GameObjectWorldModelDoodad { .. } => {
            ModelShadowKind::MovingWorldModelDoodad
        }
        _ => return Ok(0),
    };
    let static_spatial = matches!(placement.owner, M2GpuPlacementOwner::Static(_))
        .then_some(placement.static_spatial)
        .flatten();
    let radius = static_spatial.map_or_else(
        || placement_bounding_sphere(&source.model, placement.transform).1,
        |spatial| spatial.sphere().1,
    );
    let possible_maps = queries
        .admission
        .model_maps(kind, radius, queries.admission.active_maps());
    if possible_maps == 0 {
        return Ok(0);
    }
    if matches!(
        kind,
        ModelShadowKind::StaticScenery
            | ModelShadowKind::AnimatedScenery
            | ModelShadowKind::MovingWorldModelDoodad
    ) {
        let scenery = static_spatial.map_or_else(
            || {
                let bounds = source.model.bounds();
                SceneryDistance::new(bounds.minimum(), bounds.maximum(), placement.transform)
            },
            |spatial| spatial.scenery(),
        );
        if !scenery.admits_shadow(camera, detail) {
            return Ok(0);
        }
    }
    let bounds = if matches!(
        kind,
        ModelShadowKind::StaticScenery
            | ModelShadowKind::AnimatedScenery
            | ModelShadowKind::MovingWorldModelDoodad
    ) {
        let authored = source.model.bounds();
        let (minimum, maximum) = SceneryDistance::world_bounds(
            authored.minimum(),
            authored.maximum(),
            placement.transform,
        );
        solarity_systems::MovementCollisionBounds::new(minimum, maximum)
            .map_err(crate::application::RuntimeMovementRegistrationError::from)?
    } else {
        UnitSceneRegistration::new(&source.model, placement.transform)?.bounds
    };
    let mut maps = queries.admission.admitted_maps(bounds) & possible_maps;
    if let Some(owner) = super::doodad_scene::owner_key(owner) {
        maps &= queries.doodads.get(&owner).copied().unwrap_or(0);
    }
    Ok(maps)
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
    mut destination: impl FnMut(M2PreparedDraw),
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
    } else if let Some(pose) = placement
        .retirement
        .as_ref()
        .and_then(|retired| retired.unit_pose)
    {
        instance_color *= placement_color(pose.color.to_le_bytes());
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
        destination(renderer.prepare_m2_draw(
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
