//! Replicated WMO roots share native append order with authored MODF owners.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::Mat4;
use solarity_ecs::{ActiveWorld, WorldObjectIdentity};
use solarity_systems::{
    MovementBspCacheMode, MovementCollisionBounds, PlacedM2Collision, PlacedWorldModelCollision,
};

use crate::application::game_object_coordinator::{
    GameObjectResource, GameObjectWorldModelSource, RuntimeGameObjectPresentation,
};

use super::{
    DynamicMovementContext, ResidentDynamicMovement, ResidentMovementScene, RuntimeMovementOwner,
    RuntimeMovementReference, RuntimeMovementRegistrationError, RuntimeStaticMovementError,
    RuntimeStaticMovementQuery, WorldModelReference,
};

/// Stable authored or replicated root identity across registration queries.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RuntimeWorldModelMovementOwner {
    /// A MODF identity shared by every resident ADT reference.
    Static {
        /// Authored unique ID.
        unique_id: u32,
    },
    /// One replicated WMO lifetime.
    GameObject {
        /// Exact object lifetime, distinct across GUID reuse and world replacement.
        identity: WorldObjectIdentity,
    },
}

#[derive(Clone, Copy)]
pub(super) enum MovementRootReference {
    Static(WorldModelReference),
    GameObject(WorldObjectIdentity),
}

impl MovementRootReference {
    pub(super) const fn owner(self) -> RuntimeWorldModelMovementOwner {
        match self {
            Self::Static(reference) => RuntimeWorldModelMovementOwner::Static {
                unique_id: reference.unique_id,
            },
            Self::GameObject(identity) => RuntimeWorldModelMovementOwner::GameObject { identity },
        }
    }

    pub(super) const fn destination(self, group: usize) -> RuntimeMovementReference {
        match self {
            Self::Static(reference) => RuntimeMovementReference::WorldModel {
                unique_id: reference.unique_id,
                group,
            },
            Self::GameObject(identity) => {
                RuntimeMovementReference::GameObjectWorldModel { identity, group }
            }
        }
    }
}

struct RootDoodad {
    index: usize,
    local_transform: Mat4,
    collision: PlacedM2Collision,
}

struct GameObjectRoot {
    display_id: u32,
    collision: PlacedWorldModelCollision,
    transform: Mat4,
    doodads: Vec<RootDoodad>,
    groups: Vec<Vec<usize>>,
}

impl GameObjectRoot {
    fn prepare(
        source: &GameObjectWorldModelSource,
        transform: Mat4,
        display_id: u32,
    ) -> Result<Self, RuntimeMovementRegistrationError> {
        let collision =
            PlacedWorldModelCollision::prepare_transform(Arc::clone(source.model()), transform)?;
        let doodads = source
            .doodads()
            .iter()
            .map(|doodad| {
                Ok(RootDoodad {
                    index: doodad.index,
                    local_transform: doodad.local_transform,
                    collision: PlacedM2Collision::prepare_transform(
                        Arc::clone(doodad.source.model()),
                        transform * doodad.local_transform,
                    )?,
                })
            })
            .collect::<Result<Vec<_>, RuntimeMovementRegistrationError>>()?;
        let indices: HashMap<_, _> = doodads
            .iter()
            .enumerate()
            .map(|(slot, doodad)| (doodad.index, slot))
            .collect();
        let groups = source
            .model()
            .groups()
            .iter()
            .map(|group| {
                group
                    .doodad_references()
                    .iter()
                    .filter_map(|&index| indices.get(&usize::from(index)).copied())
                    .collect()
            })
            .collect();
        Ok(Self {
            display_id,
            collision,
            transform,
            doodads,
            groups,
        })
    }

    fn set_transform(&mut self, transform: Mat4) -> Result<(), RuntimeMovementRegistrationError> {
        self.collision.set_transform(transform)?;
        for doodad in &mut self.doodads {
            doodad
                .collision
                .set_transform(transform * doodad.local_transform)?;
        }
        self.transform = transform;
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct ResidentGameObjectWorldModels {
    roots: HashMap<WorldObjectIdentity, GameObjectRoot>,
    live: HashSet<WorldObjectIdentity>,
}

impl ResidentGameObjectWorldModels {
    pub(super) fn collision_mut(
        &mut self,
        identity: WorldObjectIdentity,
    ) -> Option<&mut PlacedWorldModelCollision> {
        self.roots
            .get_mut(&identity)
            .map(|root| &mut root.collision)
    }

    pub(super) fn append(
        &mut self,
        identity: WorldObjectIdentity,
        context: DynamicMovementContext<'_>,
        bounds: MovementCollisionBounds,
        cache: MovementBspCacheMode,
        dynamic: &ResidentDynamicMovement,
        output: &mut RuntimeStaticMovementQuery,
    ) -> Result<(), RuntimeStaticMovementError> {
        if context.world.object_identity(identity.guid()) != Some(identity) {
            return Ok(());
        }
        let Some(instance) = context.objects.movement_instance(identity) else {
            return Ok(());
        };
        let (Some(GameObjectResource::WorldModel(source)), Some(placement)) =
            (instance.resource(), instance.placement())
        else {
            return Ok(());
        };
        let root = self
            .roots
            .get_mut(&identity)
            .ok_or(RuntimeStaticMovementError::InvalidReference)?;
        if !Arc::ptr_eq(root.collision.model(), source.model())
            || root.transform != placement.matrix()
        {
            return Err(RuntimeStaticMovementError::InvalidReference);
        }
        if !root.collision.movement_intersects(bounds) {
            return Ok(());
        }
        if context.flags & 0xf0 != 0 {
            root.collision
                .append_movement(bounds, cache, &mut output.triangles)?;
            output.owners.resize(
                output.triangles.len(),
                RuntimeMovementOwner::GameObjectWorldModel { identity },
            );
        }
        for (group, references) in root.groups.iter().enumerate() {
            if !root.collision.movement_group_intersects(group, bounds) {
                continue;
            }
            if context.flags & 0xf != 0 {
                for &slot in references {
                    let doodad = &root.doodads[slot];
                    if output
                        .visited_dynamic_doodads
                        .insert((identity, doodad.index))
                        && doodad.collision.movement_intersects(bounds)
                    {
                        doodad
                            .collision
                            .append_movement(bounds, &mut output.triangles)?;
                        output.owners.resize(
                            output.triangles.len(),
                            RuntimeMovementOwner::GameObjectWorldModelDoodad {
                                identity,
                                doodad_index: doodad.index,
                            },
                        );
                    }
                }
            }
            dynamic.append(
                RuntimeMovementReference::GameObjectWorldModel { identity, group },
                context,
                bounds,
                output,
            )?;
        }
        Ok(())
    }
}

impl ResidentMovementScene {
    pub(super) fn clear_game_object_roots(&mut self) {
        self.game_object_world_models = ResidentGameObjectWorldModels::default();
        self.roots
            .retain(|root| matches!(root, MovementRootReference::Static(_)));
        self.dynamic.invalidate();
    }
    pub(super) fn synchronize_static_roots(&mut self) {
        let mut current: HashMap<_, _> = self
            .world_models
            .iter()
            .map(|&reference| (reference.unique_id, reference))
            .collect();
        self.roots.retain_mut(|root| match root {
            MovementRootReference::GameObject(_) => true,
            MovementRootReference::Static(reference) => {
                if let Some(updated) = current.remove(&reference.unique_id) {
                    *reference = updated;
                    true
                } else {
                    false
                }
            }
        });
        for &reference in &self.world_models {
            if current.remove(&reference.unique_id).is_some() {
                self.roots.push(MovementRootReference::Static(reference));
            }
        }
    }

    pub(super) fn synchronize_game_object_roots(
        &mut self,
        world: &ActiveWorld,
        objects: &RuntimeGameObjectPresentation,
    ) -> Result<(), RuntimeMovementRegistrationError> {
        let result = self.update_game_object_roots(world, objects);
        if result.is_err() {
            self.clear_game_object_roots();
        }
        result
    }

    fn update_game_object_roots(
        &mut self,
        world: &ActiveWorld,
        objects: &RuntimeGameObjectPresentation,
    ) -> Result<(), RuntimeMovementRegistrationError> {
        let models = &mut self.game_object_world_models;
        models.live.clear();
        for instance in objects.movement_instances() {
            let identity = instance.identity();
            if world.object_identity(identity.guid()) != Some(identity) {
                continue;
            }
            let (Some(GameObjectResource::WorldModel(source)), Some(placement)) =
                (instance.resource(), instance.map_placement())
            else {
                continue;
            };
            models.live.insert(identity);
            let transform = placement.matrix();
            if let Some(root) = models.roots.get_mut(&identity)
                && root.display_id == instance.display_id()
                && Arc::ptr_eq(root.collision.model(), source.model())
            {
                if root.transform != transform {
                    root.set_transform(transform)?;
                    self.dynamic.invalidate();
                }
                continue;
            }
            // A replacement source is a new native root allocation, appended
            // after retained MODF and replicated roots. Motion retains its slot.
            self.roots.retain(|root| !matches!(root, MovementRootReference::GameObject(owner) if *owner == identity));
            models.roots.insert(
                identity,
                GameObjectRoot::prepare(source, transform, instance.display_id())?,
            );
            self.roots.push(MovementRootReference::GameObject(identity));
            self.dynamic.invalidate();
        }
        let count = models.roots.len();
        models
            .roots
            .retain(|identity, _| models.live.contains(identity));
        if models.roots.len() != count {
            self.dynamic.invalidate();
        }
        self.roots.retain(|root| match root {
            MovementRootReference::Static(_) => true,
            MovementRootReference::GameObject(identity) => models.live.contains(identity),
        });
        Ok(())
    }
}
