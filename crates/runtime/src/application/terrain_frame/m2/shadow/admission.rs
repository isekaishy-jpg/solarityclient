//! Original root-unit admission and recursive attached-model shadow packets.

use super::super::super::shadow::{ModelShadowKind, SceneryShadowQueries, WorldShadowAdmission};
use super::super::distance::SceneryDistance;
use super::super::{
    M2GpuPlacement, M2GpuPlacementOwner, M2GpuSource, RuntimeTerrainFrameError,
    UnitSceneRegistration, placement_bounding_sphere,
};
use crate::application::terrain_coordinator::m2_residency::ResidentM2Owner;
use glam::Vec3;
use solarity_rendering::WorldShadowProjection;

/// 7BB9D0 admits dynamic unit roots; 834660 recursively visits their attachments.
pub(in super::super) fn admits_root(
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
pub(in super::super) fn environment_maps(
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
    if let Some(owner) = super::super::doodad_scene::owner_key(owner) {
        maps &= queries.doodads.get(&owner).copied().unwrap_or(0);
    }
    Ok(maps)
}
