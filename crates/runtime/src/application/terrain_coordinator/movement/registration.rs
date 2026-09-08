//! GameObject model references over the current resident map (`0x007C2E70`).

use glam::Vec3;
use solarity_asset::{TerrainChunkIndex, TerrainMap, TerrainTileIndex};
use solarity_systems::{
    MovementBspCacheMode, MovementCollectionError, MovementCollisionBounds, PlacedM2Collision,
    PlacedWorldModelCollision, TerrainCollisionError, TerrainRegistrationPoint,
    WorldModelCollisionError, WorldModelRegistrationKind, WorldModelRegistrationQuery,
    WorldModelRegistrationSelection,
};
use thiserror::Error;

use super::{
    MovementRootReference, ResidentTerrainMap, RuntimeStaticMovementResidency,
    RuntimeTerrainCoordinator, RuntimeWorldModelMovementOwner, SceneAddress,
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
    /// A group list owned by an admitted replicated WMO root.
    GameObjectWorldModel {
        /// Exact replicated lifetime of the root.
        identity: solarity_ecs::WorldObjectIdentity,
        /// Root MOGI/group index.
        group: usize,
    },
}

/// Invalid geometry or an unresolved reference in an admitted generation.
#[derive(Debug, Error)]
pub enum RuntimeMovementRegistrationError {
    /// An attached MODD could not form its current placement.
    #[error(transparent)]
    M2(#[from] solarity_systems::M2CollisionError),
    /// Terrain point or height selection failed.
    #[error(transparent)]
    Terrain(#[from] TerrainCollisionError),
    /// WMO floor/portal selection failed.
    #[error(transparent)]
    WorldModel(#[from] WorldModelCollisionError),
    /// Native box transformation failed.
    #[error(transparent)]
    Collection(#[from] MovementCollectionError),
    /// A queried liquid group references unavailable behavior data.
    #[error(transparent)]
    Liquid(#[from] solarity_systems::SubmergedLiquidError),
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
    selection: Option<WorldModelRegistrationSelection<RuntimeWorldModelMovementOwner>>,
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
    pub const fn selection(
        &self,
    ) -> Option<WorldModelRegistrationSelection<RuntimeWorldModelMovementOwner>> {
        self.selection
    }

    pub(super) fn clear(&mut self) {
        self.map_id = None;
        self.references.clear();
        self.selection = None;
        self.groups.clear();
    }
}

impl ResidentTerrainMap {
    /// Shares Unit_C's point registration between location and ground sound queries.
    pub(super) fn unit_registration(
        &mut self,
        position: Vec3,
    ) -> Result<
        WorldModelRegistrationSelection<RuntimeWorldModelMovementOwner>,
        RuntimeMovementRegistrationError,
    > {
        let start = position + Vec3::Z * 0.1;
        let end = position - Vec3::Z * 1000.0;
        let address = TerrainRegistrationPoint::new(position.x, position.y)?;
        let terrain_height = self
            .tile_at(address.tile())
            .map(|tile| tile.collision.registration_height_at(address))
            .transpose()?
            .flatten();
        let terrain_fraction = terrain_height
            .map(|height| ((f64::from(start.z) - f64::from(height)) * f64::from(0.001_f32)) as f32)
            .filter(|fraction| *fraction >= 0.0);
        let cache = MovementBspCacheMode::Enabled;
        let mut selection = self.probe_registration_roots(start, end, start, cache)?;
        if terrain_fraction.is_none() && selection.selected().is_none() {
            selection = self.probe_registration_roots(
                position,
                position + Vec3::Z * 1000.0,
                position,
                cache,
            )?;
        }
        if let Some(fraction) = terrain_fraction {
            selection.occlude_by_terrain(fraction)?;
        }
        Ok(selection)
    }

    pub(super) fn registration_root_mut(
        &mut self,
        root: MovementRootReference,
    ) -> Result<&mut PlacedWorldModelCollision, RuntimeMovementRegistrationError> {
        let reference = match root {
            MovementRootReference::Static(reference) => reference,
            MovementRootReference::GameObject(identity) => {
                return self
                    .movement
                    .game_object_world_models
                    .collision_mut(identity)
                    .ok_or(RuntimeMovementRegistrationError::InvalidReference);
            }
        };
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
    ) -> Result<
        WorldModelRegistrationSelection<RuntimeWorldModelMovementOwner>,
        RuntimeMovementRegistrationError,
    > {
        let mut query = WorldModelRegistrationQuery::new(start, end, point)?;
        for index in 0..self.movement.roots.len() {
            let reference = self.movement.roots[index];
            query.probe_root(
                reference.owner(),
                match reference {
                    MovementRootReference::Static(_) => WorldModelRegistrationKind::Static,
                    MovementRootReference::GameObject(_) => WorldModelRegistrationKind::Transformed,
                },
                self.registration_root_mut(reference)?,
                cache,
            )?;
        }
        Ok(query.finish())
    }

    fn append_registration_root(
        &mut self,
        reference: MovementRootReference,
        render_bounds: MovementCollisionBounds,
        interior: Option<usize>,
        output: &mut RuntimeMovementRegistrationQuery,
    ) -> Result<(), RuntimeMovementRegistrationError> {
        output.groups.clear();
        self.registration_root_mut(reference)?
            .append_registration_groups(render_bounds, interior, &mut output.groups)?;
        output.references.extend(
            output
                .groups
                .iter()
                .map(|&group| reference.destination(group)),
        );
        Ok(())
    }

    pub(super) fn register_game_object_movement(
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
            } else {
                // 7C1660 returns no floor for an absent/loading ADT. This is
                // object registration, not 7A5A60's movement-query admission.
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
                .roots
                .iter()
                .find(|reference| reference.owner() == candidate.owner())
                .copied()
                .ok_or(RuntimeMovementRegistrationError::InvalidReference)?;
            self.append_registration_root(
                reference,
                render_bounds,
                Some(candidate.hit().group_index()),
                output,
            )?;
        } else {
            // 7C2040/7C2139 links only resident chunks and skips missing ADTs.
            // Keep the available destinations even when the box straddles an
            // unloaded tile; scene publication invalidates and rebinds them.
            for index in 0..self.movement.roots.len() {
                let reference = self.movement.roots[index];
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
    pub(in crate::application) fn model_light_revision(&self) -> u64 {
        self.active
            .as_ref()
            .map_or(0, |active| active.movement.lighting_revision)
    }

    /// Uses the native Unit/MapObject registration's fallback face for floor color.
    pub(in crate::application) fn model_floor_light(
        &mut self,
        position: Vec3,
        model: Option<&PlacedM2Collision>,
        scratch: &mut RuntimeMovementRegistrationQuery,
    ) -> Result<
        (bool, Option<solarity_systems::WorldModelFloorLight>),
        RuntimeMovementRegistrationError,
    > {
        let Some(active) = self.active.as_mut() else {
            return Ok((false, None));
        };
        let selection = if let Some(model) = model {
            scratch.clear();
            active.register_game_object_movement(model, MovementBspCacheMode::Enabled, scratch)?;
            let Some(selection) = scratch.selection() else {
                return Ok((false, None));
            };
            selection
        } else {
            active.unit_registration(position)?
        };
        if !selection
            .selected()
            .is_some_and(|candidate| candidate.hit().is_interior())
        {
            return Ok((false, None));
        }
        let Some(candidate) = selection.fallback().into_iter().flatten().next() else {
            return Ok((false, None));
        };
        let reference = active
            .movement
            .roots
            .iter()
            .find(|reference| reference.owner() == candidate.owner())
            .copied()
            .ok_or(RuntimeMovementRegistrationError::InvalidReference)?;
        let placement = active.registration_root_mut(reference)?;
        let hit = candidate.hit();
        let group = placement
            .model()
            .groups()
            .get(hit.group_index())
            .ok_or(RuntimeMovementRegistrationError::InvalidReference)?;
        let interior = group.flags() & 0x48 == 0;
        // 7C2A70/7C2E70 reconstruct XY at the entity center. The dominant-axis
        // interpolation consumes this point; the native Z omits the start bias.
        let (center, start_z) = model.map_or((position, position.z + 0.1), |model| {
            let center = model.collision_center();
            (
                center,
                (f64::from(center.z) + 4.)
                    .min(f64::from(model.render_bounds().maximum().z) + f64::from(0.1_f32))
                    as f32,
            )
        });
        let point = Vec3::new(
            center.x,
            center.y,
            (f64::from(center.z)
                - (f64::from(center.z - 1000.) - f64::from(start_z)) * f64::from(hit.fraction()))
                as f32,
        );
        Ok((
            interior,
            if interior {
                placement.sample_group_floor_light(hit.group_index(), hit.face(), point)?
            } else {
                None
            },
        ))
    }

    /// MODD remains exterior if any registered group is exterior (7BF7F0).
    pub(in crate::application) fn doodad_interior_lighting(
        &mut self,
        owner: RuntimeWorldModelMovementOwner,
        doodad_index: usize,
    ) -> Result<bool, RuntimeMovementRegistrationError> {
        let Some(active) = self.active.as_mut() else {
            return Ok(false);
        };
        let Some(reference) = active
            .movement
            .roots
            .iter()
            .find(|root| root.owner() == owner)
            .copied()
        else {
            return Ok(false);
        };
        let placement = active.registration_root_mut(reference)?;
        let mut interior = false;
        for group in placement.model().groups() {
            if group
                .doodad_references()
                .iter()
                .any(|&index| usize::from(index) == doodad_index)
            {
                if group.flags() & 0x48 != 0 {
                    return Ok(false);
                }
                interior = true;
            }
        }
        Ok(interior)
    }

    /// Resolves 7A1640's WMO tuple from the same floor/portal banks as Unit_C.
    pub(in crate::application) fn unit_world_model_location(
        &mut self,
        position: Vec3,
    ) -> Result<Option<UnitWorldModelLocation>, RuntimeMovementRegistrationError> {
        let Some(active) = self.active.as_mut() else {
            return Ok(None);
        };
        let selection = active.unit_registration(position)?;
        for candidate in selection.primary().into_iter().flatten() {
            let reference = active
                .movement
                .roots
                .iter()
                .find(|reference| reference.owner() == candidate.owner())
                .copied()
                .ok_or(RuntimeMovementRegistrationError::InvalidReference)?;
            // 77F090/7A1640 skip exterior groups on transformed roots (flag 0x400).
            let transformed = matches!(reference, MovementRootReference::GameObject(_));
            if transformed && !candidate.hit().is_interior() {
                continue;
            }
            let name_set = match reference {
                // 7BF120 initializes replicated roots' name-set word +0x104 to zero.
                MovementRootReference::GameObject(_) => 0,
                MovementRootReference::Static(reference) => {
                    let scene = match reference.scene {
                        SceneAddress::Global => {
                            &active
                                .global_world_model
                                .as_ref()
                                .ok_or(RuntimeMovementRegistrationError::InvalidReference)?
                                .world_models
                        }
                        SceneAddress::Tile(index) => {
                            &active
                                .tile_at(index)
                                .ok_or(RuntimeMovementRegistrationError::InvalidReference)?
                                .world_models
                        }
                    };
                    u32::from(
                        scene
                            .placements()
                            .get(reference.placement)
                            .ok_or(RuntimeMovementRegistrationError::InvalidReference)?
                            .name_set(),
                    )
                }
            };
            let placement = active.registration_root_mut(reference)?;
            let group = placement
                .model()
                .groups()
                .get(candidate.hit().group_index())
                .ok_or(RuntimeMovementRegistrationError::InvalidReference)?;
            return Ok(Some(UnitWorldModelLocation {
                key: solarity_asset::WorldModelAreaKey {
                    root_id: placement.model().world_model_id(),
                    name_set,
                    group_id: group.area_table_id() as i32,
                },
                world_model_only: candidate.hit().is_interior(),
                area_override: !transformed,
            }));
        }
        Ok(None)
    }

    /// Resolves Unit_C's ground sound type with native floor and texture selection.
    pub(in crate::application) fn unit_ground_sound_type(
        &mut self,
        position: Vec3,
        sounds: &solarity_asset::MovementSoundCatalog,
    ) -> Result<u32, RuntimeMovementRegistrationError> {
        let Some(active) = self.active.as_mut() else {
            return Ok(u32::MAX);
        };
        let selection = active.unit_registration(position)?;
        // 0x007C2A70 uses the corresponding fallback face, including its
        // absent-face sentinel after portal-only registration.
        if let Some(candidate) = selection.fallback().into_iter().flatten().next() {
            let reference = active
                .movement
                .roots
                .iter()
                .find(|reference| reference.owner() == candidate.owner())
                .copied()
                .ok_or(RuntimeMovementRegistrationError::InvalidReference)?;
            let placement = active.registration_root_mut(reference)?;
            let hit = candidate.hit();
            let material = hit
                .face()
                .and_then(|face| {
                    placement
                        .model()
                        .groups()
                        .get(hit.group_index())?
                        .polygons()
                        .get(usize::from(face))
                })
                .and_then(|polygon| {
                    placement
                        .model()
                        .materials()
                        .get(usize::from(polygon.material_id()))
                });
            return Ok(material.map_or(u32::MAX, |material| material.ground_type()));
        }
        let Some((index, chunk_index, [x, y])) =
            TerrainMap::sound_cell_at_world_position(position.x, position.y)
        else {
            return Ok(u32::MAX);
        };
        let Some(tile) = active.tile_at(index) else {
            return Ok(u32::MAX);
        };
        let effect = tile
            .decoded
            .chunks()
            .iter()
            .find(|chunk| chunk.index() == chunk_index)
            .and_then(|chunk| chunk.ground_effect_at(x, y));
        Ok(effect
            .and_then(|effect| sounds.ground_effect_terrain(effect))
            .unwrap_or(u32::MAX))
    }

    /// Resolves GameObject M2 destinations against the admitted map scene.
    ///
    /// Uses native floor/portal banks, upward retry, terrain occlusion, interior
    /// group selection, and direct chunk rounding. Replicated WMO roots enter
    /// native transformed-root banks after GameObject movement synchronization.
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

/// WMO identity and location gates, before resolving any DBC relationships.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::application) struct UnitWorldModelLocation {
    pub key: solarity_asset::WorldModelAreaKey,
    pub world_model_only: bool,
    /// 782560 excludes transformed roots when overriding the terrain AreaTable ID.
    pub area_override: bool,
}
