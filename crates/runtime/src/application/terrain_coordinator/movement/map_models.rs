//! Transport map-model references in native CM2MapObject tail order.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::Mat4;
use solarity_asset::DecodedM2Model;
use solarity_ecs::{ActiveWorld, WorldObjectIdentity};
use solarity_systems::{MovementBspCacheMode, MovementCollisionBounds};

use crate::application::game_object_coordinator::RuntimeGameObjectPresentation;

use super::{
    DynamicMovementContext, ResidentTerrainMap, RuntimeMovementOwner, RuntimeMovementReference,
    RuntimeMovementRegistrationError, RuntimeMovementRegistrationQuery, RuntimeStaticMovementError,
    RuntimeStaticMovementQuery, RuntimeStaticMovementResidency,
};

/// Publication snapshot rejects geometry changed after spatial registration.
struct RegisteredModel {
    display_id: u32,
    model: Arc<DecodedM2Model>,
    transform: Mat4,
    placement_revision: u64,
    references: Vec<RuntimeMovementReference>,
    residency: RuntimeStaticMovementResidency,
}

/// 7B5740 appends references on every map-matrix write, including station samples.
/// Empty/next-map routes retain their slot until their next valid matrix update.
#[derive(Default)]
pub(super) struct ResidentMapModels {
    owners: HashMap<WorldObjectIdentity, RegisteredModel>,
    order: Vec<WorldObjectIdentity>,
    updated: Vec<WorldObjectIdentity>,
    touched: HashSet<WorldObjectIdentity>,
    live: HashSet<WorldObjectIdentity>,
    lists: HashMap<RuntimeMovementReference, Vec<WorldObjectIdentity>>,
    registration: RuntimeMovementRegistrationQuery,
    residency: Option<RuntimeStaticMovementResidency>,
}

impl ResidentMapModels {
    /// Map models use the same 7C2E70 registration probes because 783500 sets
    /// owner+7C bit 0x2000, but they never enter generic GO callback eligibility.
    pub(super) fn synchronize(
        &mut self,
        map: &mut ResidentTerrainMap,
        world: &ActiveWorld,
        objects: &RuntimeGameObjectPresentation,
        cache: MovementBspCacheMode,
        invalidated: bool,
    ) -> Result<(), RuntimeMovementRegistrationError> {
        for list in self.lists.values_mut() {
            list.clear();
        }
        self.residency = None;
        self.live.clear();
        self.updated.clear();
        self.touched.clear();
        for instance in objects.movement_instances() {
            let identity = instance.identity();
            if world.object_identity(identity.guid()) != Some(identity) {
                continue;
            }
            let Some(transport) = instance.transport_model() else {
                continue;
            };
            let Some(model) = transport.collision() else {
                continue;
            };
            self.live.insert(identity);
            let changed = self.owners.get(&identity).is_none_or(|registered| {
                registered.display_id != instance.display_id()
                    || !Arc::ptr_eq(&registered.model, model.model())
                    || registered.transform != model.transform()
                    || registered.placement_revision != transport.placement_revision()
            });
            if changed {
                self.touched.insert(identity);
                self.updated.push(identity);
            } else if !invalidated
                && self.owners.get(&identity).is_some_and(|registered| {
                    registered.residency == RuntimeStaticMovementResidency::Ready
                })
            {
                continue;
            }
            self.registration.clear();
            let residency =
                map.register_game_object_movement(&model, cache, &mut self.registration)?;
            let mut references = self
                .owners
                .remove(&identity)
                .map_or_else(Vec::new, |registered| registered.references);
            references.clear();
            if residency == RuntimeStaticMovementResidency::Ready {
                references.extend_from_slice(self.registration.references());
            }
            self.owners.insert(
                identity,
                RegisteredModel {
                    display_id: instance.display_id(),
                    model: Arc::clone(model.model()),
                    transform: model.transform(),
                    placement_revision: transport.placement_revision(),
                    references,
                    residency,
                },
            );
        }
        self.owners
            .retain(|identity, _| self.live.contains(identity));
        // Batch unlinking keeps the per-frame work linear in admitted owners.
        self.order
            .retain(|identity| self.live.contains(identity) && !self.touched.contains(identity));
        self.order.extend_from_slice(&self.updated);
        for identity in &self.order {
            let Some(registered) = self.owners.get(identity) else {
                continue;
            };
            for &reference in &registered.references {
                // 6DED60 appends M2 references; generic callbacks use a separate
                // 7B5020 list. Retained owners preserve their relative order.
                self.lists.entry(reference).or_default().push(*identity);
            }
            if registered.residency != RuntimeStaticMovementResidency::Ready
                && self.residency.is_none()
            {
                self.residency = Some(registered.residency);
            }
        }
        self.lists.retain(|_, list| !list.is_empty());
        Ok(())
    }

    pub(super) fn residency(&self) -> RuntimeStaticMovementResidency {
        self.residency
            .unwrap_or(RuntimeStaticMovementResidency::Ready)
    }

    /// 7A50C0 chooses the upper query mask for an M2 with a nonzero native GUID.
    /// These faces precede 7A5240's generic GameObject callback candidates.
    pub(super) fn append(
        &self,
        reference: RuntimeMovementReference,
        context: DynamicMovementContext<'_>,
        bounds: MovementCollisionBounds,
        output: &mut RuntimeStaticMovementQuery,
    ) -> Result<(), RuntimeStaticMovementError> {
        if context.flags & 0xf00000 == 0 {
            return Ok(());
        }
        let Some(list) = self.lists.get(&reference) else {
            return Ok(());
        };
        for &identity in list {
            if output.visited_dynamic.contains(&identity)
                || context.world.object_identity(identity.guid()) != Some(identity)
            {
                continue;
            }
            let Some(instance) = context.objects.movement_instance(identity) else {
                continue;
            };
            let Some(transport) = instance.transport_model() else {
                continue;
            };
            let Some(model) = transport.collision() else {
                continue;
            };
            let registered = self
                .owners
                .get(&identity)
                .ok_or(RuntimeStaticMovementError::InvalidReference)?;
            if registered.display_id != instance.display_id()
                || !Arc::ptr_eq(&registered.model, model.model())
                || registered.transform != model.transform()
                || registered.placement_revision != transport.placement_revision()
            {
                return Err(RuntimeStaticMovementError::InvalidReference);
            }
            if model.movement_intersects(bounds) {
                model.append_movement(bounds, &mut output.triangles)?;
                output.owners.resize(
                    output.triangles.len(),
                    RuntimeMovementOwner::GameObjectMapModel { identity },
                );
            }
            output.visited_dynamic.insert(identity);
        }
        Ok(())
    }
}
