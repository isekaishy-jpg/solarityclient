//! GameObject model references over the current resident map (`0x007C2E70`).

use glam::Vec3;
use solarity_asset::{TerrainChunkIndex, TerrainTileIndex};
use solarity_systems::{
    MovementBspCacheMode, MovementCollectionError, MovementCollisionBounds, PlacedM2Collision,
    PlacedWorldModelCollision, TerrainCollisionError, TerrainRegistrationPoint,
    WorldModelCollisionError, WorldModelRegistrationKind, WorldModelRegistrationQuery,
    WorldModelRegistrationSelection,
};
use thiserror::Error;

use super::{
    ResidentTerrainMap, RuntimeStaticMovementResidency, RuntimeTerrainCoordinator, SceneAddress,
    WorldModelReference,
};

/// One native list destination, scoped to the registration query's map.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RuntimeMovementReference {
    /// The dynamic list following an MCNK's authored M2 references.
    Terrain {
        /// Owning ADT.
        tile: TerrainTileIndex,
        /// Owning MCNK.
        chunk: TerrainChunkIndex,
    },
    /// The dynamic list following a WMO group's authored M2 references.
    WorldModel {
        /// MODF identity shared across resident ADTs.
        unique_id: u32,
        /// Root MOGI/group index.
        group: usize,
    },
}

/// Invalid geometry or an unresolved reference in an admitted generation.
#[derive(Debug, Error)]
pub enum RuntimeMovementRegistrationError {
    /// Terrain point or height selection failed.
    #[error(transparent)]
    Terrain(#[from] TerrainCollisionError),
    /// WMO floor/portal selection failed.
    #[error(transparent)]
    WorldModel(#[from] WorldModelCollisionError),
    /// Native box transformation failed.
    #[error(transparent)]
    Collection(#[from] MovementCollectionError),
    /// An admitted MODF no longer resolves to its complete collision generation.
    #[error("resident movement registration reference is invalid")]
    InvalidReference,
}

/// Reusable result and scratch for one GameObject model's spatial registration.
///
/// References are emitted in native allocation order. The caller inserts each
/// GameObject at the front of its destination's dynamic list. Pending and failed
/// registrations clear all results so a former placement cannot remain active.
#[derive(Default)]
pub struct RuntimeMovementRegistrationQuery {
    map_id: Option<u32>,
    references: Vec<RuntimeMovementReference>,
    selection: Option<WorldModelRegistrationSelection<u32>>,
    groups: Vec<usize>,
}

impl RuntimeMovementRegistrationQuery {
    /// Creates retained registration storage.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the map of the last complete registration.
    #[must_use]
    pub const fn map_id(&self) -> Option<u32> {
        self.map_id
    }

    /// Returns destinations in native reference-allocation order.
    #[must_use]
    pub fn references(&self) -> &[RuntimeMovementReference] {
        &self.references
    }

    /// Retains native floor/fallback channels for interior and support state.
    #[must_use]
    pub const fn selection(&self) -> Option<WorldModelRegistrationSelection<u32>> {
        self.selection
    }

    fn clear(&mut self) {
        self.map_id = None;
        self.references.clear();
        self.selection = None;
        self.groups.clear();
    }
}

impl ResidentTerrainMap {
    fn registration_root_mut(
        &mut self,
        reference: WorldModelReference,
    ) -> Result<&mut PlacedWorldModelCollision, RuntimeMovementRegistrationError> {
        let models = match reference.scene {
            SceneAddress::Global => {
                &mut self
                    .global_world_model
                    .as_mut()
                    .ok_or(RuntimeMovementRegistrationError::InvalidReference)?
                    .world_model_collision
            }
            SceneAddress::Tile(index) => {
                &mut self
                    .tile
                    .iter_mut()
                    .chain(&mut self.nearby)
                    .find(|tile| tile.index() == index)
                    .ok_or(RuntimeMovementRegistrationError::InvalidReference)?
                    .world_model_collision
            }
        };
        models
            .instance_mut(reference.placement)
            .ok_or(RuntimeMovementRegistrationError::InvalidReference)
    }

    fn probe_registration_roots(
        &mut self,
        start: Vec3,
        end: Vec3,
        point: Vec3,
        cache: MovementBspCacheMode,
    ) -> Result<WorldModelRegistrationSelection<u32>, RuntimeMovementRegistrationError> {
        let mut query = WorldModelRegistrationQuery::new(start, end, point)?;
        for index in 0..self.movement.world_models.len() {
            let reference = self.movement.world_models[index];
            query.probe_root(
                reference.unique_id,
                WorldModelRegistrationKind::Static,
                self.registration_root_mut(reference)?,
                cache,
            )?;
        }
        Ok(query.finish())
    }

    fn append_registration_root(
        &mut self,
        reference: WorldModelReference,
        render_bounds: MovementCollisionBounds,
        interior: Option<usize>,
        output: &mut RuntimeMovementRegistrationQuery,
    ) -> Result<(), RuntimeMovementRegistrationError> {
        output.groups.clear();
        self.registration_root_mut(reference)?
            .append_registration_groups(render_bounds, interior, &mut output.groups)?;
        output.references.extend(output.groups.iter().map(|&group| {
            RuntimeMovementReference::WorldModel {
                unique_id: reference.unique_id,
                group,
            }
        }));
        Ok(())
    }

    fn register_game_object_movement(
        &mut self,
        model: &PlacedM2Collision,
        cache: MovementBspCacheMode,
        output: &mut RuntimeMovementRegistrationQuery,
    ) -> Result<RuntimeStaticMovementResidency, RuntimeMovementRegistrationError> {
        let center = model.collision_center();
        let render_bounds = model.render_bounds();
        let start = Vec3::new(
            center.x,
            center.y,
            (f64::from(center.z) + 4.)
                .min(f64::from(render_bounds.maximum().z) + f64::from(0.1_f32)) as f32,
        );
        let end = Vec3::new(center.x, center.y, center.z - 1000.);
        let point = Vec3::new(center.x, center.y, center.z + 0.15);
        let terrain_height = if self.terrain.global_world_model().is_some() {
            if self.global_world_model.is_none() {
                return Ok(RuntimeStaticMovementResidency::PendingMap);
            }
            None
        } else {
            let address = TerrainRegistrationPoint::new(start.x, start.y)?;
            if let Some(tile) = self.tile_at(address.tile()) {
                tile.collision.registration_height_at(address)?
            } else if self.terrain.tile(address.tile()).exists() {
                return Ok(RuntimeStaticMovementResidency::PendingTile {
                    tile: address.tile(),
                });
            } else {
                None
            }
        };
        // 7C28F0 stores this fraction before accepting terrain; height above the
        // segment start does not suppress the fallback upward WMO probe.
        let terrain_fraction = terrain_height
            .map(|height| ((f64::from(start.z) - f64::from(height)) * f64::from(0.001_f32)) as f32)
            .filter(|fraction| *fraction >= 0.);
        let mut selection = self.probe_registration_roots(start, end, point, cache)?;
        if terrain_fraction.is_none() && selection.selected().is_none() {
            selection = self.probe_registration_roots(
                center,
                Vec3::new(center.x, center.y, center.z + 1000.),
                point,
                cache,
            )?;
        }
        // 781A10 maps the GameObject creation flags (0xB) to +7C & 0x2000.
        selection.clear_secondary_bank();
        if let Some(fraction) = terrain_fraction {
            selection.occlude_by_terrain(fraction)?;
        }
        if let Some(candidate) = selection
            .selected()
            .filter(|candidate| candidate.hit().is_interior())
        {
            let reference = self
                .movement
                .world_models
                .iter()
                .find(|reference| reference.unique_id == candidate.owner())
                .copied()
                .ok_or(RuntimeMovementRegistrationError::InvalidReference)?;
            self.append_registration_root(
                reference,
                render_bounds,
                Some(candidate.hit().group_index()),
                output,
            )?;
        } else {
            // Prove every destination before publishing even the earlier WMO
            // references. Undeclared WDT tiles remain empty space.
            if self.terrain.global_world_model().is_none() {
                for (tile, _) in render_bounds.registration_terrain_chunks()? {
                    if self.terrain.tile(tile).exists() && self.tile_at(tile).is_none() {
                        return Ok(RuntimeStaticMovementResidency::PendingTile { tile });
                    }
                }
            }
            for index in 0..self.movement.world_models.len() {
                let reference = self.movement.world_models[index];
                self.append_registration_root(reference, render_bounds, None, output)?;
            }
            if self.terrain.global_world_model().is_none() {
                for (tile_index, chunk) in render_bounds.registration_terrain_chunks()? {
                    if let Some(tile) = self.tile_at(tile_index)
                        && tile.collision.chunk_bounds(chunk).minimum().z
                            <= render_bounds.maximum().z
                    {
                        output.references.push(RuntimeMovementReference::Terrain {
                            tile: tile_index,
                            chunk,
                        });
                    }
                }
            }
        }
        output.selection = Some(selection);
        Ok(RuntimeStaticMovementResidency::Ready)
    }
}

impl RuntimeTerrainCoordinator {
    /// Resolves GameObject M2 destinations against the complete static map scene.
    ///
    /// Uses native floor/portal banks, upward retry, terrain occlusion, interior
    /// group selection, and direct chunk rounding. A dynamic scene must consume
    /// these destinations and include dynamic WMO roots before committing a
    /// complete ordinary movement query.
    ///
    /// # Errors
    /// Returns [`RuntimeMovementRegistrationError`] for invalid geometry or an
    /// invalid admitted reference. All pending/error results clear the output.
    pub fn register_game_object_movement(
        &mut self,
        map_id: u32,
        model: &PlacedM2Collision,
        cache: MovementBspCacheMode,
        output: &mut RuntimeMovementRegistrationQuery,
    ) -> Result<RuntimeStaticMovementResidency, RuntimeMovementRegistrationError> {
        output.clear();
        let Some(active) = self
            .active
            .as_mut()
            .filter(|active| active.map_id() == map_id)
        else {
            return Ok(RuntimeStaticMovementResidency::PendingMap);
        };
        let result = active.register_game_object_movement(model, cache, output);
        if matches!(result, Ok(RuntimeStaticMovementResidency::Ready)) {
            output.map_id = Some(map_id);
        } else {
            output.clear();
        }
        result
    }
}
