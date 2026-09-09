//! Ordered static and replicated geometry over resident ADT/WMO generations.

mod cache;
mod camera;
mod dynamic;
mod liquid;
mod map_models;
mod registration;
mod scene;
mod world_model;

pub use cache::{RuntimeMovementGeometry, RuntimeMovementGeometryFailure};
pub(super) use camera::CameraGeometry;
use dynamic::{DynamicMovementContext, ResidentDynamicMovement};
pub use dynamic::{RuntimeMovementOwner, RuntimeMovementQuery};
pub use world_model::RuntimeWorldModelMovementOwner;
use world_model::{MovementRootReference, ResidentGameObjectWorldModels};

pub(in crate::application) use registration::UnitWorldModelLocation;
pub use registration::{
    RuntimeMovementReference, RuntimeMovementRegistrationError, RuntimeMovementRegistrationQuery,
};

use std::collections::{HashMap, HashSet};

use solarity_asset::{TerrainChunkIndex, TerrainTileIndex};
use solarity_systems::{
    MovementBspCacheMode, MovementCollectionError, MovementCollisionBounds,
    MovementCollisionTriangle,
};
use thiserror::Error;

use super::m2_residency::{ResidentM2Owner, ResidentM2Scene};
use super::{ResidentTerrainMap, RuntimeTerrainCoordinator};

/// Authored static owner of a selected movement face, scoped to its query map.
///
/// These are placement identities, not network GUIDs. Stock's two-word object
/// identifier is zero for static terrain, MDDF, MODF, and MODD geometry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeStaticMovementOwner {
    /// One MCNK selected in native world-grid order.
    Terrain {
        /// Owning ADT address.
        tile: TerrainTileIndex,
        /// Owning MCNK address.
        chunk: TerrainChunkIndex,
    },
    /// One MDDF identity shared by all of its resident MCRF references.
    TerrainDoodad {
        /// Authored MDDF unique ID.
        unique_id: u32,
    },
    /// One MODF identity shared by neighboring ADTs.
    WorldModel {
        /// Authored MODF unique ID.
        unique_id: u32,
    },
    /// One MODD record selected by its WMO's active set and group MODR list.
    WorldModelDoodad {
        /// Owning MODF unique ID.
        world_model_unique_id: u32,
        /// Authored root MODD index.
        doodad_index: usize,
    },
}

impl From<ResidentM2Owner> for RuntimeStaticMovementOwner {
    fn from(owner: ResidentM2Owner) -> Self {
        match owner {
            ResidentM2Owner::TerrainDoodad { unique_id } => Self::TerrainDoodad { unique_id },
            ResidentM2Owner::WorldModelDoodad {
                world_model_unique_id,
                doodad_index,
            } => Self::WorldModelDoodad {
                world_model_unique_id,
                doodad_index,
            },
        }
    }
}

/// Residency outcome for the static portion of an ordinary movement query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeStaticMovementResidency {
    /// Every required static generation was available; the query may be empty.
    Ready,
    /// The requested map's complete static owner has not been admitted.
    PendingMap,
    /// WDT declares an ADT that has not completed preparation.
    PendingTile {
        /// The first unavailable ADT in native query traversal order.
        tile: TerrainTileIndex,
    },
}

/// Invalid geometry or a broken reference in an admitted static generation.
#[derive(Debug, Error)]
pub enum RuntimeStaticMovementError {
    /// A sweep cannot produce valid world collection bounds.
    #[error(transparent)]
    Sweep(#[from] solarity_systems::MovementSweepError),
    /// The movement owner supplied an invalid interval or generated query bounds.
    #[error(transparent)]
    IntervalBounds(#[from] solarity_systems::MovementIntervalBoundsError),
    /// A collector rejected the bounds or selected source geometry.
    #[error(transparent)]
    Collection(#[from] MovementCollectionError),
    /// A prepared scene reference no longer resolves to its owning generation.
    #[error("resident movement reference is invalid or out of date")]
    InvalidReference,
}

/// Caller-retained static query output and first-visit storage.
///
/// Pending or failed queries clear both arrays. A successful static query must
/// still be joined to dynamic objects, transports, and the resolved liquid/WDL
/// mode before a local movement interval is committed.
#[derive(Default)]
pub struct RuntimeStaticMovementQuery {
    map_id: Option<u32>,
    triangles: Vec<MovementCollisionTriangle>,
    owners: Vec<RuntimeMovementOwner>,
    visited: HashSet<ResidentM2Owner>,
    visited_dynamic: HashSet<solarity_ecs::WorldObjectIdentity>,
    visited_dynamic_doodads: HashSet<(solarity_ecs::WorldObjectIdentity, usize)>,
}

impl RuntimeStaticMovementQuery {
    /// Creates reusable query storage.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the map of the last complete static query.
    #[must_use]
    pub const fn map_id(&self) -> Option<u32> {
        self.map_id
    }

    /// Returns ordered candidates from the last complete static query.
    #[must_use]
    pub fn triangles(&self) -> &[MovementCollisionTriangle] {
        &self.triangles
    }

    /// Resolves a collision solver's selected triangle to its authored owner.
    #[must_use]
    pub fn owner(&self, triangle: usize) -> Option<RuntimeStaticMovementOwner> {
        match self.owners.get(triangle)? {
            RuntimeMovementOwner::Static(owner) => Some(*owner),
            _ => None,
        }
    }

    fn clear(&mut self) {
        self.map_id = None;
        self.triangles.clear();
        self.owners.clear();
        self.visited.clear();
        self.visited_dynamic.clear();
        self.visited_dynamic_doodads.clear();
    }

    fn record(&mut self, owner: RuntimeStaticMovementOwner) {
        self.owners
            .resize(self.triangles.len(), RuntimeMovementOwner::Static(owner));
    }

    fn append_m2(
        &mut self,
        scene: &ResidentM2Scene,
        collision: &solarity_systems::M2CollisionScene,
        index: usize,
        bounds: MovementCollisionBounds,
    ) -> Result<(), RuntimeStaticMovementError> {
        let placement = scene
            .placements()
            .get(index)
            .ok_or(RuntimeStaticMovementError::InvalidReference)?;
        if self.visited.insert(placement.owner()) {
            let geometry = collision
                .instance(index)
                .ok_or(RuntimeStaticMovementError::InvalidReference)?;
            if geometry.movement_intersects(bounds) {
                geometry.append_movement(bounds, &mut self.triangles)?;
                self.record(placement.owner().into());
            }
        }
        Ok(())
    }
}

/// Source address survives primary-tile promotion without reordering owners.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SceneAddress {
    Global,
    Tile(TerrainTileIndex),
}

#[derive(Clone, Copy)]
struct WorldModelReference {
    unique_id: u32,
    scene: SceneAddress,
    placement: usize,
}

/// Native append-order MODF registrations, independent of the primary tile.
#[derive(Default)]
pub(super) struct ResidentMovementScene {
    lighting_revision: u64,
    world_models: Vec<WorldModelReference>,
    dynamic: ResidentDynamicMovement,
    roots: Vec<MovementRootReference>,
    game_object_world_models: ResidentGameObjectWorldModels,
}

/// Worker-prepared MCRF/MODR references into the corresponding M2 scene.
#[derive(Default)]
pub(super) struct ResidentMovementReferences {
    chunks: Vec<Vec<usize>>,
    world_models: Vec<Vec<Vec<usize>>>,
}

impl ResidentMovementReferences {
    pub(super) fn prepare(
        tile: Option<&solarity_asset::DecodedTerrainTile>,
        m2: &ResidentM2Scene,
        world_models: &super::world_model_residency::ResidentWorldModelScene,
    ) -> Self {
        let indices: HashMap<_, _> = m2
            .placements()
            .iter()
            .enumerate()
            .map(|(index, placement)| (placement.owner(), index))
            .collect();
        let chunks = tile.map_or_else(Vec::new, |tile| {
            tile.chunks()
                .iter()
                .map(|chunk| {
                    chunk
                        .doodad_references()
                        .iter()
                        .map(|&reference| {
                            let unique_id = tile.doodads()[reference as usize].unique_id();
                            indices[&ResidentM2Owner::TerrainDoodad { unique_id }]
                        })
                        .collect()
                })
                .collect()
        });
        let world_models = world_models
            .placements()
            .iter()
            .map(|placement| {
                world_models.sources()[placement.source_index()]
                    .model()
                    .groups()
                    .iter()
                    .map(|group| {
                        group
                            .doodad_references()
                            .iter()
                            .filter_map(|&reference| {
                                indices
                                    .get(&ResidentM2Owner::WorldModelDoodad {
                                        world_model_unique_id: placement.unique_id(),
                                        doodad_index: usize::from(reference),
                                    })
                                    .copied()
                            })
                            .collect()
                    })
                    .collect()
            })
            .collect();
        Self {
            chunks,
            world_models,
        }
    }
}

impl ResidentTerrainMap {
    /// Reconciles references only when a complete static generation changes.
    pub(super) fn synchronize_movement_owners(&mut self) {
        self.movement.lighting_revision = self.movement.lighting_revision.wrapping_add(1);
        self.movement.dynamic.invalidate();
        let scenes = self
            .global_world_model
            .iter()
            .map(|global| (SceneAddress::Global, &global.world_models))
            .chain(
                self.tile
                    .iter()
                    .chain(&self.nearby)
                    .map(|tile| (SceneAddress::Tile(tile.index()), &tile.world_models)),
            );
        let mut registered = HashMap::new();
        let mut additions = Vec::new();
        for (scene, models) in scenes {
            for (placement, model) in models.placements().iter().enumerate() {
                let reference = WorldModelReference {
                    unique_id: model.unique_id(),
                    scene,
                    placement,
                };
                if let std::collections::hash_map::Entry::Vacant(entry) =
                    registered.entry(reference.unique_id)
                {
                    entry.insert(reference);
                    additions.push(reference);
                }
            }
        }
        self.movement.world_models.retain_mut(|reference| {
            if let Some(current) = registered.remove(&reference.unique_id) {
                *reference = current;
                true
            } else {
                false
            }
        });
        self.movement.world_models.extend(
            additions
                .into_iter()
                .filter(|reference| registered.remove(&reference.unique_id).is_some()),
        );
        self.movement.synchronize_static_roots();
    }

    fn collect_static_movement(
        &mut self,
        bounds: MovementCollisionBounds,
        cache_mode: MovementBspCacheMode,
        output: &mut RuntimeStaticMovementQuery,
        dynamic: Option<DynamicMovementContext<'_>>,
    ) -> Result<RuntimeStaticMovementResidency, RuntimeStaticMovementError> {
        // 0x007A5A60: a missing WDT declaration is empty space; an unavailable
        // declared owner returns false. Prove completeness before exposing any
        // candidates, including WMO faces collected before the terrain loop.
        if self.terrain.global_world_model().is_some() {
            if self.global_world_model.is_none() {
                return Ok(RuntimeStaticMovementResidency::PendingMap);
            }
        } else {
            for (tile, _) in bounds.terrain_chunks()? {
                if self.terrain.tile(tile).exists() && self.tile_at(tile).is_none() {
                    return Ok(RuntimeStaticMovementResidency::PendingTile { tile });
                }
            }
        }
        // 0x007A55E0 consumes root geometry, then each group's referenced M2s.
        // 0x007BF1A5 appends roots through 0x006DED60; promotion cannot change it.
        let flags = dynamic.map_or(u32::MAX, |context| context.flags);
        for root in &self.movement.roots {
            let reference = match *root {
                MovementRootReference::Static(reference) => reference,
                MovementRootReference::GameObject(identity) => {
                    if let Some(context) = dynamic {
                        self.movement.game_object_world_models.append(
                            identity,
                            context,
                            bounds,
                            cache_mode,
                            &self.movement.dynamic,
                            output,
                        )?;
                    }
                    continue;
                }
            };
            let (m2, collision, models, references) = match reference.scene {
                SceneAddress::Global => {
                    let global = self
                        .global_world_model
                        .as_mut()
                        .ok_or(RuntimeStaticMovementError::InvalidReference)?;
                    (
                        &global.m2_scene,
                        &global.m2_collision,
                        &mut global.world_model_collision,
                        &global.movement_references,
                    )
                }
                SceneAddress::Tile(index) => {
                    let tile = self
                        .tile
                        .iter_mut()
                        .chain(&mut self.nearby)
                        .find(|tile| tile.index() == index)
                        .ok_or(RuntimeStaticMovementError::InvalidReference)?;
                    (
                        &tile.m2_scene,
                        &tile.m2_collision,
                        &mut tile.world_model_collision,
                        &tile.movement_references,
                    )
                }
            };
            let model = models
                .instance_mut(reference.placement)
                .ok_or(RuntimeStaticMovementError::InvalidReference)?;
            if !model.movement_intersects(bounds) {
                continue;
            }
            if flags & 0xf0 != 0 {
                model.append_movement(bounds, cache_mode, &mut output.triangles)?;
                output.record(RuntimeStaticMovementOwner::WorldModel {
                    unique_id: reference.unique_id,
                });
            }
            let groups = references
                .world_models
                .get(reference.placement)
                .ok_or(RuntimeStaticMovementError::InvalidReference)?;
            for (group, doodads) in groups.iter().enumerate() {
                if model.movement_group_intersects(group, bounds) {
                    if flags & 0xf != 0 {
                        for &index in doodads {
                            output.append_m2(m2, collision, index, bounds)?;
                        }
                    }
                    if let Some(context) = dynamic {
                        self.movement.dynamic.append(
                            RuntimeMovementReference::WorldModel {
                                unique_id: reference.unique_id,
                                group,
                            },
                            context,
                            bounds,
                            output,
                        )?;
                    }
                }
            }
        }
        if self.terrain.global_world_model().is_none() {
            for (tile_index, chunk) in bounds.terrain_chunks()? {
                let Some(tile) = self.tile_at(tile_index) else {
                    continue;
                };
                if flags & 0x100 != 0 {
                    tile.collision
                        .append_movement_chunk(chunk, bounds, &mut output.triangles)?;
                    output.record(RuntimeStaticMovementOwner::Terrain {
                        tile: tile_index,
                        chunk,
                    });
                }
                let references = &tile.movement_references.chunks
                    [usize::from(chunk.y()) * 16 + usize::from(chunk.x())];
                // 0x007C6150 appends MDDF references in their authored MCRF
                // order. 0x007A50C0 stamps each placement once per query.
                if flags & 0xf != 0 {
                    for &index in references {
                        output.append_m2(&tile.m2_scene, &tile.m2_collision, index, bounds)?;
                    }
                }
                if let Some(context) = dynamic {
                    self.movement.dynamic.append(
                        RuntimeMovementReference::Terrain {
                            tile: tile_index,
                            chunk,
                        },
                        context,
                        bounds,
                        output,
                    )?;
                }
            }
        }
        Ok(RuntimeStaticMovementResidency::Ready)
    }
}

impl RuntimeTerrainCoordinator {
    /// Collects ordinary static movement geometry from the matching resident map.
    ///
    /// WMO roots precede native world-grid chunks; M2 references retain their
    /// first visit across all groups, chunks, and ADTs. This is the static
    /// portion of the query; dynamic/transport and liquid/WDL providers remain
    /// the responsibility of the movement interval owner.
    ///
    /// # Errors
    /// Returns an error for invalid selected geometry or an invalid resident
    /// reference. Pending and failed results always clear the supplied output.
    pub fn collect_static_movement(
        &mut self,
        map_id: u32,
        bounds: MovementCollisionBounds,
        cache_mode: MovementBspCacheMode,
        output: &mut RuntimeStaticMovementQuery,
    ) -> Result<RuntimeStaticMovementResidency, RuntimeStaticMovementError> {
        output.clear();
        let Some(active) = self
            .active
            .as_mut()
            .filter(|active| active.map_id() == map_id)
        else {
            return Ok(RuntimeStaticMovementResidency::PendingMap);
        };
        let result = active.collect_static_movement(bounds, cache_mode, output, None);
        if matches!(result, Ok(RuntimeStaticMovementResidency::Ready)) {
            output.map_id = Some(map_id);
        } else {
            output.clear();
        }
        result
    }
}
