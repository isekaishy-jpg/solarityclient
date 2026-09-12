//! Animation and opacity binding at each population publication boundary.

use super::{ActiveWorld, RuntimePlayerPresentation, UnitAnimationInput};

impl RuntimePlayerPresentation {
    pub(super) fn synchronize_replicated_animations(
        &mut self,
        world: &ActiveWorld,
        kind: solarity_ecs::ObjectKind,
    ) {
        self.unit_animations.retain_world(world);
        let bodies = (kind == solarity_ecs::ObjectKind::Unit)
            .then_some(&self.creatures_resident)
            .into_iter()
            .flatten()
            .map(|resident| (resident.key.guid, &resident.model, resident.mount.is_some()))
            .chain(
                (kind == solarity_ecs::ObjectKind::Player)
                    .then_some(&self.remote_players)
                    .into_iter()
                    .flatten()
                    .map(|resident| (resident.guid, &resident.model, resident.mount.is_some())),
            );
        for (guid, model, mounted) in bodies {
            let Some(identity) = world.object_identity(guid) else {
                continue;
            };
            let Some(presentation) = world.unit_presentation(guid) else {
                continue;
            };
            let movement = world.movement_state(guid);
            self.unit_animations.bind(
                identity,
                model,
                &self.animations,
                UnitAnimationInput::new(
                    presentation.stand_state(),
                    presentation.animation_tier(),
                    mounted,
                    movement,
                )
                .with_orientation(world, guid, false, false),
            );
            self.synchronize_unit_opacity(world, guid);
        }
    }

    pub(super) fn synchronize_unit_opacity(&self, world: &ActiveWorld, guid: u64) {
        let (Some(owner), Some(presentation)) = (
            self.unit_animations.get(guid),
            world.unit_presentation(guid),
        ) else {
            return;
        };
        owner.synchronize_passenger(
            world,
            &self.vehicles,
            &self.passenger_frames,
            self.passenger_frames.admitted_parent(owner.identity()),
            self.unit_animations.scene_time_ms(),
        );
        let passenger = world
            .movement_state(guid)
            .and_then(|movement| movement.context().transport);
        let transport = passenger.map_or(0, |transport| transport.guid);
        owner.opacity_owner().set_transport_guid(transport);
        let player_hidden = world.object_kind(guid) == Some(solarity_ecs::ObjectKind::Player)
            && world
                .entity_by_guid(guid)
                .and_then(|entity| {
                    world
                        .storage()
                        .get::<&solarity_ecs::ObjectFields>(entity)
                        .ok()
                        .map(|fields| {
                            solarity_systems::player_flags_hide_model(
                                fields.get(150),
                                self.arena_map,
                            )
                        })
                })
                .unwrap_or(false);
        owner.opacity_owner().set_player_hidden(player_hidden);
        if owner.opacity_owner().has_model(presentation.display_id()) {
            return;
        }
        let flags = world.unit_flags(guid).unwrap_or_default();
        let bytes = world
            .entity_by_guid(guid)
            .and_then(|entity| {
                world
                    .storage()
                    .get::<&solarity_ecs::ObjectFields>(entity)
                    .ok()
                    .map(|fields| fields.get(74))
            })
            .unwrap_or(0);
        let parent_transitioning = world
            .object_kind(transport)
            .filter(|kind| {
                matches!(
                    kind,
                    solarity_ecs::ObjectKind::Unit | solarity_ecs::ObjectKind::Player
                )
            })
            .map(|_| {
                self.unit_animations
                    .get(transport)
                    .is_some_and(|parent| parent.opacity_owner().transitioning())
            });
        let duration = solarity_systems::EntityOpacity::unit_entry_duration(
            flags.primary(),
            flags.secondary(),
            bytes,
            transport,
            parent_transitioning,
            passenger.and_then(|passenger| {
                self.vehicles
                    .passenger_seat(
                        world.unit_vehicle(passenger.guid)?.definition_id(),
                        passenger.seat,
                    )
                    .map(|seat| seat.attachment_id())
            }),
        );
        let alpha = self
            .creatures
            .display(presentation.display_id())
            .map_or(255, |display| display.model_alpha());
        let target = (f64::from(alpha as i32) * f64::from(f32::from_bits(0x3b80_8081))) as f32;
        owner.opacity_owner().select_model(
            presentation.display_id(),
            target,
            duration,
            self.unit_animations.scene_time_ms(),
        );
    }
}
