//! Retained generic GameObject lists and the native first-visit callback.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use glam::Mat4;
use solarity_asset::DecodedM2Model;
use solarity_ecs::{ActiveWorld, WorldObjectIdentity};
use solarity_systems::{
    MovementBspCacheMode, MovementCollisionBounds, MovementCollisionTriangle,
    MovementIntervalBounds, MovementIntervalRequest, MovementTransportFrame,
};

use super::map_models::ResidentMapModels;
use crate::application::game_object_coordinator::RuntimeGameObjectPresentation;

use super::{
    ResidentTerrainMap, RuntimeMovementReference, RuntimeMovementRegistrationError,
    RuntimeMovementRegistrationQuery, RuntimeStaticMovementError, RuntimeStaticMovementOwner,
    RuntimeStaticMovementQuery, RuntimeStaticMovementResidency, RuntimeTerrainCoordinator,
};

/// Authored placement or replicated lifetime owning a movement candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeMovementOwner {
    /// Terrain, MODF, MDDF, or MODD placement, with native reported GUID zero.
    Static(RuntimeStaticMovementOwner),
    /// A generic GameObject callback (`0x004F6560`).
    GameObject {
        /// Exact object lifetime used for resolution and first-visit stamping.
        identity: WorldObjectIdentity,
        /// Parent transport GUID when present, otherwise this object's GUID.
        reported_guid: u64,
    },
    /// Faces of an admitted replicated WMO root; its native GUID is the owner GUID.
    GameObjectWorldModel {
        /// Exact replicated lifetime owning the root.
        identity: WorldObjectIdentity,
    },
    /// A transport CM2MapObject; its native reported GUID is this owner's GUID.
    GameObjectMapModel {
        /// Exact replicated lifetime owning the map model.
        identity: WorldObjectIdentity,
    },
    /// An authored MODD attached to a replicated root; native reported GUID is zero.
    GameObjectWorldModelDoodad {
        /// Exact replicated lifetime owning the root.
        identity: WorldObjectIdentity,
        /// Authored MODD index in that root's default set.
        doodad_index: usize,
    },
}

/// Retained static, replicated WMO, transport M2, and generic GameObject geometry.
///
/// Specialized GameObject behavior, transport-relative motion, and liquid/WDL
/// mode must also be resolved before committing ordinary player movement.
#[derive(Default)]
pub struct RuntimeMovementQuery {
    inner: RuntimeStaticMovementQuery,
    interval_bounds: Option<MovementIntervalBounds>,
}

impl RuntimeMovementQuery {
    /// Creates reusable query storage.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the map of the last complete query.
    #[must_use]
    pub const fn map_id(&self) -> Option<u32> {
        self.inner.map_id()
    }

    /// Returns candidates in native root, group, chunk, and reference order.
    /// A passenger geometry context converts these faces to its supplied frame.
    #[must_use]
    pub fn triangles(&self) -> &[MovementCollisionTriangle] {
        self.inner.triangles()
    }

    /// Resolves the collision solver's selected face to its owning lifetime.
    #[must_use]
    pub fn owner(&self, triangle: usize) -> Option<RuntimeMovementOwner> {
        self.inner.owners.get(triangle).copied()
    }

    /// Returns the world body and probe bounds from the last complete interval query.
    /// Explicit-box queries, pending residency, and errors clear this value.
    #[must_use]
    pub const fn interval_bounds(&self) -> Option<MovementIntervalBounds> {
        self.interval_bounds
    }

    /// Publishes complete interval coverage after optional passenger conversion.
    pub(super) fn set_interval_bounds(&mut self, bounds: MovementIntervalBounds) {
        self.interval_bounds = Some(bounds);
    }

    /// Converts complete candidates without changing their native owner ordering.
    /// A conversion failure retires the entire query, including metadata.
    pub(super) fn localize(
        &mut self,
        frame: MovementTransportFrame,
    ) -> Result<(), RuntimeStaticMovementError> {
        for triangle in &mut self.inner.triangles {
            match frame.local_triangle(triangle) {
                Ok(local) => *triangle = local,
                Err(error) => {
                    self.clear();
                    return Err(error.into());
                }
            }
        }
        Ok(())
    }

    pub(super) fn clear(&mut self) {
        self.inner.clear();
        self.interval_bounds = None;
    }
}

struct DynamicOwner {
    display_id: u32,
    model: Arc<DecodedM2Model>,
    transform: Mat4,
    references: Vec<RuntimeMovementReference>,
    residency: RuntimeStaticMovementResidency,
}

#[derive(Clone, Copy)]
pub(super) struct DynamicMovementContext<'a> {
    pub world: &'a ActiveWorld,
    pub objects: &'a RuntimeGameObjectPresentation,
    pub flags: u32,
}

/// Destination lists retain their order across frames without rebuilding them.
#[derive(Default)]
pub(super) struct ResidentDynamicMovement {
    map_models: ResidentMapModels,
    world_identity: Option<WorldObjectIdentity>,
    owners: HashMap<WorldObjectIdentity, DynamicOwner>,
    lists: HashMap<RuntimeMovementReference, VecDeque<WorldObjectIdentity>>,
    order: Vec<WorldObjectIdentity>,
    live: HashSet<WorldObjectIdentity>,
    registration: RuntimeMovementRegistrationQuery,
    invalidated: bool,
}

impl ResidentDynamicMovement {
    pub(super) fn invalidate(&mut self) {
        self.invalidated = true;
    }

    fn unlink(&mut self, identity: WorldObjectIdentity) -> Option<DynamicOwner> {
        let owner = self.owners.remove(&identity)?;
        for reference in &owner.references {
            if let Some(list) = self.lists.get_mut(reference) {
                list.retain(|&entry| entry != identity);
            }
        }
        // Keep empty buckets until this update finishes so moving the sole
        // owner within a destination reuses its list allocation.
        Some(owner)
    }

    fn synchronize(
        &mut self,
        map: &mut ResidentTerrainMap,
        world: &ActiveWorld,
        objects: &RuntimeGameObjectPresentation,
        cache: MovementBspCacheMode,
    ) -> Result<(), RuntimeMovementRegistrationError> {
        self.live.clear();
        for instance in objects.movement_instances() {
            let identity = instance.identity();
            if world.object_identity(identity.guid()) != Some(identity) {
                continue;
            }
            let Some(behavior) = instance.behavior() else {
                continue;
            };
            let Some(model) = behavior.collision() else {
                continue;
            };
            self.live.insert(identity);
            let new_owner = !self.owners.contains_key(&identity);
            let unchanged = self.owners.get(&identity).is_some_and(|owner| {
                owner.display_id == instance.display_id()
                    && Arc::ptr_eq(&owner.model, model.model())
                    && owner.transform == model.transform()
                    && owner.residency == RuntimeStaticMovementResidency::Ready
            });
            if unchanged && !self.invalidated {
                continue;
            }
            self.registration.clear();
            // 7C2F80 removes old destinations before a changed placement can
            // publish new ones. An unavailable ADT never preserves old faces.
            let mut previous = if unchanged {
                None
            } else {
                self.unlink(identity)
            };
            let residency =
                map.register_game_object_movement(&model, cache, &mut self.registration)?;
            if unchanged
                && residency == RuntimeStaticMovementResidency::Ready
                && self
                    .owners
                    .get(&identity)
                    .is_some_and(|owner| owner.references == self.registration.references())
            {
                continue;
            }
            if unchanged {
                previous = self.unlink(identity);
            }
            let mut references = previous.map_or_else(Vec::new, |owner| owner.references);
            references.clear();
            if residency == RuntimeStaticMovementResidency::Ready {
                references.extend_from_slice(self.registration.references());
            }
            for &reference in &references {
                // 7B5020 inserts collidable dynamic owners at the head. The
                // trailing non-collidable family is not admitted here.
                self.lists
                    .entry(reference)
                    .or_default()
                    .push_front(identity);
            }
            self.owners.insert(
                identity,
                DynamicOwner {
                    display_id: instance.display_id(),
                    model: Arc::clone(model.model()),
                    transform: model.transform(),
                    references,
                    residency,
                },
            );
            if new_owner {
                self.order.push(identity);
            }
        }
        for index in 0..self.order.len() {
            let identity = self.order[index];
            if !self.live.contains(&identity) {
                drop(self.unlink(identity));
            }
        }
        self.order.retain(|identity| self.live.contains(identity));
        self.lists.retain(|_, list| !list.is_empty());
        self.map_models
            .synchronize(map, world, objects, cache, self.invalidated)?;
        self.invalidated = false;
        self.world_identity = world
            .local_player_guid()
            .ok()
            .and_then(|guid| world.object_identity(guid));
        Ok(())
    }

    fn residency(&self) -> RuntimeStaticMovementResidency {
        self.order
            .iter()
            .find_map(|identity| {
                self.owners
                    .get(identity)
                    .map(|owner| owner.residency)
                    .filter(|&residency| residency != RuntimeStaticMovementResidency::Ready)
            })
            .unwrap_or_else(|| self.map_models.residency())
    }

    pub(super) fn append(
        &self,
        reference: RuntimeMovementReference,
        context: DynamicMovementContext<'_>,
        bounds: MovementCollisionBounds,
        output: &mut RuntimeStaticMovementQuery,
    ) -> Result<(), RuntimeStaticMovementError> {
        self.map_models.append(reference, context, bounds, output)?;
        let DynamicMovementContext {
            world,
            objects,
            flags,
        } = context;
        if flags & 0xf00000 == 0 {
            return Ok(());
        }
        let Some(list) = self.lists.get(&reference) else {
            return Ok(());
        };
        for &identity in list {
            // 7A5240 stamps even when the callback rejects this owner or its
            // box misses; later references must not invoke it a second time.
            if !output.visited_dynamic.insert(identity) {
                continue;
            }
            if world.object_identity(identity.guid()) != Some(identity) {
                continue;
            }
            let owner = self
                .owners
                .get(&identity)
                .ok_or(RuntimeStaticMovementError::InvalidReference)?;
            let Some(fields) = world.game_object_presentation(identity.guid()) else {
                continue;
            };
            let Some(behavior) = objects
                .movement_instance(identity)
                .and_then(|instance| instance.behavior())
            else {
                continue;
            };
            if !behavior.collision_eligible(fields.object_type(), flags) {
                continue;
            }
            let Some(model) = behavior.collision() else {
                continue;
            };
            if !Arc::ptr_eq(&owner.model, model.model()) || owner.transform != model.transform() {
                return Err(RuntimeStaticMovementError::InvalidReference);
            }
            if model.movement_intersects(bounds) {
                model.append_movement(bounds, &mut output.triangles)?;
                let reported_guid = world
                    .game_object_movement(identity.guid())
                    .and_then(|movement| movement.transport().map(|parent| parent.guid))
                    .filter(|&guid| guid != 0)
                    .unwrap_or(identity.guid());
                output.owners.resize(
                    output.triangles.len(),
                    RuntimeMovementOwner::GameObject {
                        identity,
                        reported_guid,
                    },
                );
            }
        }
        Ok(())
    }
}

impl RuntimeTerrainCoordinator {
    /// Collects one movement interval's complete expanded candidate region.
    ///
    /// The request must already be in world coordinates. Ground queries include
    /// private step/fall probes, even at zero travel. Current collection uses
    /// ordinary terrain/WMO/M2 material policy; liquid/WDL and transport-space
    /// conversion still belong to their respective movement providers.
    ///
    /// # Errors
    /// Invalid interval inputs, geometry, or stale references clear all output.
    pub fn collect_movement_interval(
        &mut self,
        world: &ActiveWorld,
        objects: &RuntimeGameObjectPresentation,
        request: MovementIntervalRequest,
        flags: u32,
        cache: MovementBspCacheMode,
        output: &mut RuntimeMovementQuery,
    ) -> Result<RuntimeStaticMovementResidency, RuntimeStaticMovementError> {
        output.clear();
        let bounds = request.collection_bounds()?;
        let residency =
            self.collect_movement(world, objects, bounds.query(), flags, cache, output)?;
        if residency == RuntimeStaticMovementResidency::Ready {
            output.interval_bounds = Some(bounds);
        }
        Ok(residency)
    }

    /// Updates generic GameObject and transport map-model references after CPU
    /// model and behavior synchronization. Destination allocations are retained.
    ///
    /// # Errors
    /// Invalid registration geometry clears all dynamic references before
    /// returning the error. A pending map has no dynamic scene to publish.
    pub fn synchronize_game_object_movement(
        &mut self,
        world: Option<&ActiveWorld>,
        objects: &RuntimeGameObjectPresentation,
        cache: MovementBspCacheMode,
    ) -> Result<RuntimeStaticMovementResidency, RuntimeMovementRegistrationError> {
        let Some(map) = self.active.as_mut() else {
            return Ok(RuntimeStaticMovementResidency::PendingMap);
        };
        let Some(world) = world.filter(|world| world.map_id().value() == map.map_id()) else {
            map.movement.clear_game_object_roots();
            map.movement.dynamic = ResidentDynamicMovement::default();
            return Ok(RuntimeStaticMovementResidency::PendingMap);
        };
        let mut dynamic = std::mem::take(&mut map.movement.dynamic);
        map.movement.synchronize_game_object_roots(world, objects)?;
        if map.movement.dynamic.invalidated {
            dynamic.invalidate();
        }
        // Failure drops the detached registry, leaving no partially updated
        // or stale destinations attached to the map.
        dynamic.synchronize(map, world, objects, cache)?;
        let residency = dynamic.residency();
        map.movement.dynamic = dynamic;
        Ok(residency)
    }

    /// Collects static geometry, admitted WMO roots, and registered GameObjects.
    ///
    /// Call after synchronizing current object models and movement references.
    /// Query flags go to the live native eligibility gate, including door bit
    /// 0x8000. Family gates are authored M2 0xF, WMO 0xF0, terrain 0x100, and
    /// GameObjects/transport M2 0xF00000. Faces use ordinary material selection;
    /// liquid/WDL collection and specialized GameObject behavior are separate.
    ///
    /// # Errors
    /// Invalid admitted geometry or a stale placement clears the output.
    pub fn collect_movement(
        &mut self,
        world: &ActiveWorld,
        objects: &RuntimeGameObjectPresentation,
        bounds: MovementCollisionBounds,
        flags: u32,
        cache: MovementBspCacheMode,
        output: &mut RuntimeMovementQuery,
    ) -> Result<RuntimeStaticMovementResidency, RuntimeStaticMovementError> {
        output.clear();
        let Some(map) = self
            .active
            .as_mut()
            .filter(|map| map.map_id() == world.map_id().value())
        else {
            return Ok(RuntimeStaticMovementResidency::PendingMap);
        };
        let residency = map.movement.dynamic.residency();
        if map.movement.dynamic.invalidated
            || map.movement.dynamic.world_identity
                != world
                    .local_player_guid()
                    .ok()
                    .and_then(|guid| world.object_identity(guid))
        {
            return Err(RuntimeStaticMovementError::InvalidReference);
        }
        if residency != RuntimeStaticMovementResidency::Ready {
            return Ok(residency);
        }
        let result = map.collect_static_movement(
            bounds,
            cache,
            &mut output.inner,
            Some(DynamicMovementContext {
                world,
                objects,
                flags,
            }),
        );
        if matches!(result, Ok(RuntimeStaticMovementResidency::Ready)) {
            output.inner.map_id = Some(map.map_id());
        } else {
            output.inner.clear();
        }
        result
    }
}
