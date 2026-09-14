//! Changed object inputs publish one retained instance image in admission order.

use super::{
    EntityOpacityOwner, GameObjectInstance, RuntimeGameObjectError, RuntimeGameObjectPresentation,
    valid_transform,
};
use solarity_ecs::{ActiveWorld, ObjectKind};
use std::cell::RefCell;
use std::rc::Rc;

impl RuntimeGameObjectPresentation {
    pub(super) fn refresh_instances(
        &mut self,
        world: Option<&ActiveWorld>,
    ) -> Result<(), RuntimeGameObjectError> {
        let _profile = solarity_profiling::profile!("world.game_objects.refresh");
        let sampled = solarity_profiling::detail_enabled();
        let mut visited = 0_u64;
        let mut unchanged = 0_u64;
        self.admit_world(world);
        let Some(world) = world else {
            return Ok(());
        };
        self.behaviors
            .retain(|identity, _| world.object_identity(identity.guid()) == Some(*identity));
        self.transport_behaviors
            .retain(|identity, _| world.object_identity(identity.guid()) == Some(*identity));
        self.transport_guid = world.local_player_transport_guid();
        self.transport_identity = self
            .transport_guid
            .and_then(|guid| world.object_identity(guid));

        let previous_revision = self.scene_revision;
        let before = self.instances.len();
        self.instances.retain(|instance| {
            let keep = world.object_identity(instance.guid()) == Some(instance.identity)
                && world.object_kind(instance.guid()) == Some(ObjectKind::GameObject);
            if !keep {
                instance.opacity.mark_removed(self.scene_time_ms.get());
            }
            keep
        });
        if self.instances.len() != before {
            self.indices.clear();
            self.indices.extend(
                self.instances
                    .iter()
                    .enumerate()
                    .map(|(index, instance)| (instance.identity, index)),
            );
            self.scene_revision = self.scene_revision.wrapping_add(1);
        }

        for identity in world.visible_game_objects() {
            if sampled {
                visited += 1;
            }
            let presentation = world
                .game_object_presentation(identity.guid())
                .unwrap_or_default();
            let index = self.indices.get(&identity).copied();
            let display_changed = index.is_none_or(|index| {
                self.instances[index].display_id() != presentation.display_id()
            });
            let request = if display_changed {
                self.request_for(presentation.display_id())?
            } else {
                None
            };
            let placement = self.placement_resolver.resolve(world, identity.guid());
            let transform = world
                .object_transform(identity.guid())
                .filter(valid_transform);
            let object = world.object_presentation(identity.guid());
            let entry = object.map_or(0, solarity_ecs::ObjectPresentation::entry_id);
            let scale = object
                .map(solarity_ecs::ObjectPresentation::scale)
                .filter(|scale| scale.is_finite() && *scale > 0.0);
            if let Some(index) = index {
                let instance = &self.instances[index];
                if instance.presentation == presentation
                    && instance.entry == entry
                    && instance.placement == placement
                    && instance.passenger_placement == placement
                    && instance.transform == transform
                    && instance.scale == scale
                {
                    if sampled {
                        unchanged += 1;
                    }
                    continue;
                }
            }
            // Resource completion and behavior clocks have their own owners.
            // An unchanged projection does not reassign those handles or publish
            // another instance image. Resolve moving-parent placement above so
            // passenger and map-handle lifetimes retain their native distinction.
            let behavior = self.behavior_for(identity, presentation);
            let transport = self.transport_for(identity, presentation);
            if let Some(index) = index {
                let instance = &mut self.instances[index];
                if display_changed {
                    instance.world_model_state.borrow_mut().take();
                    if let Some(behavior) = instance.behavior() {
                        behavior.detach_model();
                    }
                    if let Some(transport) = &instance.transport {
                        transport.detach_model();
                    }
                    instance.resource = request
                        .as_ref()
                        .and_then(|request| self.resources.get(request))
                        .cloned();
                    instance.request = request;
                    instance.failed = false;
                    self.scene_revision = self.scene_revision.wrapping_add(1);
                }
                if instance.placement.is_ok() != placement.is_ok() {
                    self.scene_revision = self.scene_revision.wrapping_add(1);
                }
                instance.presentation = presentation;
                instance.entry = entry;
                instance.placement = placement;
                instance.passenger_placement = placement;
                instance.transform = transform;
                instance.scale = scale;
                instance.behavior = behavior;
                instance.transport = transport;
            } else {
                let resource = request
                    .as_ref()
                    .and_then(|request| self.resources.get(request))
                    .cloned();
                self.indices.insert(identity, self.instances.len());
                self.instances.push(GameObjectInstance {
                    entry,
                    template: None,
                    identity,
                    presentation,
                    transform,
                    scale,
                    placement,
                    passenger_placement: placement,
                    request,
                    resource,
                    failed: false,
                    behavior,
                    transport,
                    world_model_state: RefCell::new(None),
                    opacity: Rc::new(EntityOpacityOwner::default()),
                });
                self.scene_revision = self.scene_revision.wrapping_add(1);
            }
        }
        if sampled {
            solarity_profiling::profile_value!("world.game_objects.projected", visited);
            solarity_profiling::profile_value!(
                "world.game_objects.projection_unchanged",
                unchanged
            );
            solarity_profiling::profile_value!(
                "world.game_objects.projection_published",
                visited - unchanged
            );
        }
        if self.scene_revision != previous_revision {
            self.collect_unused();
            self.placement_resolver.retain_world(world);
        }
        Ok(())
    }
}
