//! Per-owner spatial samples and native light transitions, reused while stationary.

use super::{
    M2GpuPlacementOwner, ResidentM2Owner, RuntimeTerrainFrameError, UnitSceneRegistration,
};
use crate::application::terrain_coordinator::{
    RuntimeMovementRegistrationQuery, RuntimeTerrainCoordinator, RuntimeWorldModelMovementOwner,
};
use glam::Mat4;
use solarity_systems::{WorldEntityLightEnvironment, WorldEntityLightState, WorldModelFloorLight};

/// Ordinary terrain doodads never need an individual floor-light callback. Keep
/// their placement records compact while preserving lazy state for other owners.
#[derive(Default)]
pub(super) struct EntityLighting {
    retained: Option<Box<RetainedEntityLighting>>,
}

/// Spatial query scratch and transition history live only on sampled owners.
#[derive(Default)]
struct RetainedEntityLighting {
    cached: Option<(Mat4, u64, bool, Option<WorldModelFloorLight>, bool)>,
    state: Option<WorldEntityLightState>,
    last_time_ms: Option<f32>,
    scratch: RuntimeMovementRegistrationQuery,
    scene: Option<CachedModelScene>,
    scene_scratch: RuntimeMovementRegistrationQuery,
    indoor_fog: bool,
    fog_revision: Option<u64>,
}

struct CachedModelScene {
    transform: Mat4,
    model_bounds: solarity_asset::M2ModelBounds,
    registration: Option<UnitSceneRegistration>,
    query: UnitSceneRegistration,
    revision: u64,
    surface: Option<(f32, f32)>,
}

impl EntityLighting {
    /// Shares the owner's native registration between liquid and fog callbacks.
    /// Spatial results remain valid while stationary; fog selection follows each
    /// frame's ordered scene visits and keeps the bank across exterior draws.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn scene_state(
        &mut self,
        owner: M2GpuPlacementOwner,
        model: &std::sync::Arc<solarity_asset::DecodedM2Model>,
        transform: Mat4,
        registration: Option<UnitSceneRegistration>,
        terrain: &mut RuntimeTerrainCoordinator,
        liquids: &solarity_asset::LiquidTypeCatalog,
        view: Mat4,
    ) -> Result<(solarity_rendering::M2LiquidState, Option<bool>), RuntimeTerrainFrameError> {
        use solarity_rendering::M2LiquidState;
        if !ordinary_callback(owner) {
            return Ok((M2LiquidState::Above, None));
        }
        let retained = self.retained.get_or_insert_with(Box::default);
        let revision = terrain.model_light_revision();
        let model_bounds = model.bounds();
        if retained.scene.as_ref().is_none_or(|cached| {
            cached.transform != transform
                || cached.model_bounds != model_bounds
                || cached.registration != registration
                || cached.revision != revision
        }) {
            let collision = matches!(owner, M2GpuPlacementOwner::GameObject { .. })
                .then(|| {
                    solarity_systems::PlacedM2Collision::prepare_transform(
                        std::sync::Arc::clone(model),
                        transform,
                    )
                })
                .transpose()?;
            let query = match (collision.as_ref(), registration) {
                (Some(collision), _) => UnitSceneRegistration {
                    position: transform.w_axis.truncate(),
                    bounds: collision.render_bounds(),
                    sphere: UnitSceneRegistration::model_sphere(model, transform),
                },
                (None, Some(registration)) => registration,
                (None, None) => UnitSceneRegistration::new(model, transform)?,
            };
            let bounds = query.bounds;
            terrain.register_model_scene(
                query.position,
                bounds,
                collision.as_ref(),
                &mut retained.scene_scratch,
            )?;
            let height = terrain.model_liquid_height(
                query.position,
                bounds,
                liquids,
                &retained.scene_scratch,
            )?;
            retained.scene = Some(CachedModelScene {
                transform,
                model_bounds,
                registration,
                query,
                revision,
                surface: height.map(|height| (height, bounds.maximum().z)),
            });
            retained.fog_revision = None;
        }
        let scene_revision = terrain.model_scene_revision();
        if retained.fog_revision != Some(scene_revision)
            && let Some(cached) = &retained.scene
        {
            retained.indoor_fog = terrain.model_scene_fog(
                &retained.scene_scratch,
                cached.query.bounds,
                cached.query.sphere,
                retained.indoor_fog,
            )?;
            retained.fog_revision = Some(scene_revision);
        }
        Ok((
            match retained.scene.as_ref().and_then(|cached| cached.surface) {
                Some((height, maximum)) if height <= maximum => {
                    M2LiquidState::at_world_height(height, view)
                }
                Some(_) => M2LiquidState::Below,
                None => M2LiquidState::Above,
            },
            Some(retained.indoor_fog),
        ))
    }

    /// Samples the existing owner-specific callback without starting a transition
    /// clock before its first sample. Unsampled scenery allocates no retained state.
    #[allow(clippy::too_many_arguments)]
    pub fn sample(
        &mut self,
        owner: M2GpuPlacementOwner,
        model: &std::sync::Arc<solarity_asset::DecodedM2Model>,
        transform: Mat4,
        color: [u8; 4],
        time_ms: f32,
        terrain: &mut RuntimeTerrainCoordinator,
        environment: WorldEntityLightEnvironment,
    ) -> Result<Option<solarity_rendering::M2DirectionalLight>, RuntimeTerrainFrameError> {
        let revision = terrain.model_light_revision();
        let doodad = match owner {
            M2GpuPlacementOwner::Static(ResidentM2Owner::WorldModelDoodad {
                world_model_unique_id,
                doodad_index,
            }) => Some((
                RuntimeWorldModelMovementOwner::Static {
                    unique_id: world_model_unique_id,
                },
                doodad_index,
            )),
            M2GpuPlacementOwner::GameObjectWorldModelDoodad {
                identity,
                doodad_index,
                ..
            } => Some((
                RuntimeWorldModelMovementOwner::GameObject { identity },
                doodad_index,
            )),
            _ => None,
        };
        let sample = if let Some((root, index)) = doodad {
            let retained = self.retained.get_or_insert_with(Box::default);
            if retained
                .cached
                .is_none_or(|(_, old, _, _, _)| old != revision)
            {
                retained.cached = Some((
                    transform,
                    revision,
                    terrain.doodad_interior_lighting(root, index)?,
                    None,
                    false,
                ));
            }
            WorldEntityLightState::doodad(
                color,
                retained
                    .cached
                    .is_some_and(|(_, _, interior, _, _)| interior),
                environment,
            )
        } else if ordinary_callback(owner) {
            let retained = self.retained.get_or_insert_with(Box::default);
            if retained
                .cached
                .is_none_or(|(matrix, old, _, _, _)| old != revision || matrix != transform)
            {
                let collision = if matches!(owner, M2GpuPlacementOwner::GameObject { .. }) {
                    Some(solarity_systems::PlacedM2Collision::prepare_transform(
                        std::sync::Arc::clone(model),
                        transform,
                    )?)
                } else {
                    None
                };
                let (interior, floor, terrain_shadow) = terrain.model_floor_light(
                    transform.w_axis.truncate(),
                    collision.as_ref(),
                    &mut retained.scratch,
                )?;
                retained.cached = Some((transform, revision, interior, floor, terrain_shadow));
            }
            let state = retained
                .state
                .get_or_insert_with(|| WorldEntityLightState::new(environment));
            if let Some((_, _, interior, floor, terrain_shadow)) = retained.cached {
                state.set_floor(interior, floor, environment);
                state.set_terrain_shadow(terrain_shadow);
            }
            let seconds = retained
                .last_time_ms
                .replace(time_ms)
                .map_or(0., |previous| (time_ms - previous).max(0.) * 0.001);
            state.advance(seconds, environment);
            state.sample(environment)
        } else {
            return Ok(None);
        };
        Ok(Some(solarity_rendering::M2DirectionalLight::new(
            sample.ray(),
            sample.ambient(),
            sample.diffuse(),
        )))
    }
}

fn ordinary_callback(owner: M2GpuPlacementOwner) -> bool {
    matches!(
        owner,
        M2GpuPlacementOwner::PlayerBody { .. }
            | M2GpuPlacementOwner::PlayerMount { .. }
            | M2GpuPlacementOwner::RemotePlayerBody { .. }
            | M2GpuPlacementOwner::RemotePlayerMount { .. }
            | M2GpuPlacementOwner::CreatureBody { .. }
            | M2GpuPlacementOwner::CreatureMount { .. }
            | M2GpuPlacementOwner::GameObject { .. }
    )
}

pub(super) fn is_doodad(owner: M2GpuPlacementOwner) -> bool {
    matches!(
        owner,
        M2GpuPlacementOwner::Static(ResidentM2Owner::WorldModelDoodad { .. })
            | M2GpuPlacementOwner::GameObjectWorldModelDoodad { .. }
    )
}
