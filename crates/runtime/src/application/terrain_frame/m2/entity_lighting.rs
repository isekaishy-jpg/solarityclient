//! Per-owner spatial samples and native light transitions, reused while stationary.

use super::{M2GpuPlacementOwner, ResidentM2Owner, RuntimeTerrainFrameError};
use crate::application::terrain_coordinator::{
    RuntimeMovementRegistrationQuery, RuntimeTerrainCoordinator, RuntimeWorldModelMovementOwner,
};
use glam::Mat4;
use solarity_systems::{WorldEntityLightEnvironment, WorldEntityLightState, WorldModelFloorLight};

#[derive(Default)]
pub(super) struct EntityLighting {
    cached: Option<(Mat4, u64, bool, Option<WorldModelFloorLight>)>,
    state: Option<WorldEntityLightState>,
    last_time_ms: Option<f32>,
    scratch: RuntimeMovementRegistrationQuery,
}

impl EntityLighting {
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
            if self.cached.is_none_or(|(_, old, _, _)| old != revision) {
                self.cached = Some((
                    transform,
                    revision,
                    terrain.doodad_interior_lighting(root, index)?,
                    None,
                ));
            }
            WorldEntityLightState::doodad(
                color,
                self.cached.is_some_and(|(_, _, interior, _)| interior),
                environment,
            )
        } else if matches!(
            owner,
            M2GpuPlacementOwner::PlayerBody { .. }
                | M2GpuPlacementOwner::PlayerMount { .. }
                | M2GpuPlacementOwner::RemotePlayerBody { .. }
                | M2GpuPlacementOwner::RemotePlayerMount { .. }
                | M2GpuPlacementOwner::CreatureBody { .. }
                | M2GpuPlacementOwner::GameObject { .. }
        ) {
            if self
                .cached
                .is_none_or(|(matrix, old, _, _)| old != revision || matrix != transform)
            {
                let collision = if matches!(owner, M2GpuPlacementOwner::GameObject { .. }) {
                    Some(solarity_systems::PlacedM2Collision::prepare_transform(
                        std::sync::Arc::clone(model),
                        transform,
                    )?)
                } else {
                    None
                };
                let (interior, floor) = terrain.model_floor_light(
                    transform.w_axis.truncate(),
                    collision.as_ref(),
                    &mut self.scratch,
                )?;
                self.cached = Some((transform, revision, interior, floor));
            }
            let state = self
                .state
                .get_or_insert_with(|| WorldEntityLightState::new(environment));
            if let Some((_, _, interior, floor)) = self.cached {
                state.set_floor(interior, floor, environment);
            }
            let seconds = self
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

pub(super) fn is_doodad(owner: M2GpuPlacementOwner) -> bool {
    matches!(
        owner,
        M2GpuPlacementOwner::Static(ResidentM2Owner::WorldModelDoodad { .. })
            | M2GpuPlacementOwner::GameObjectWorldModelDoodad { .. }
    )
}
