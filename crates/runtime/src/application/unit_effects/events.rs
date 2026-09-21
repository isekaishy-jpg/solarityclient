//! Authored unit callbacks retain their original environmental effect ordering.

use super::RuntimeUnitEffects;
use crate::application::RuntimePlayerPresentation;
use crate::application::terrain_frame::RuntimeM2Event;
use crate::application::terrain_frame::m2::unit_effects::{
    UnitEffectBinding, UnitEffectRequest, UnitEffectResource,
};
use crate::application::unit_animation::UnitAnimationBehavior;
use glam::{Mat4, Vec3};
use solarity_asset::{CreatureCatalog, DecodedM2Model};
use solarity_ecs::{ActiveWorld, ObjectFields, ObjectKind};
use solarity_systems::{
    UnitWaterSprayInput, resolve_unit_movement_speed_extended, unit_player_inebriation,
    unit_world_effect_factor,
};
use std::rc::Rc;

impl RuntimeUnitEffects {
    pub(in crate::application) fn environmental_impact(
        &mut self,
        impact: &crate::application::gameplay_coordinator::environmental_damage::RuntimeEnvironmentalDamageSnapshot,
        world: &ActiveWorld,
        player: &RuntimePlayerPresentation,
        creatures: &CreatureCatalog,
    ) -> Vec<UnitEffectRequest> {
        if world.object_identity(impact.identity.guid()) != Some(impact.identity) {
            return Vec::new();
        }
        let Some(kit) = self.environmental.visual_kit(impact.packet.kind) else {
            return Vec::new();
        };
        let state = self.units.entry(impact.identity).or_default();
        for (kind, parameters) in kit.special_effects() {
            if kind == 13 {
                state.tint.apply(impact.timestamp_ms, parameters);
            }
        }
        let Some(owner) = player.unit_effect_owner(impact.identity) else {
            return Vec::new();
        };
        owner.set_model_color(state.tint.sample(impact.timestamp_ms, u32::MAX));
        if let Ok(animation) = u16::try_from(kit.animation()) {
            owner.request_environmental_animation(
                animation,
                impact.attack_target_guid,
                impact.template_flags,
            );
        }
        let definition = world
            .unit_presentation(impact.identity.guid())
            .and_then(|unit| creatures.display(unit.display_id()))
            .and_then(|display| creatures.model(display.model_id()));
        kit.effects()
            .enumerate()
            .filter_map(|(index, (attachment, id))| {
                let attachment = u32::try_from(attachment).ok()?;
                Some(UnitEffectRequest {
                    identity: impact.identity,
                    lifetime: Rc::downgrade(&state.lifetime),
                    kind: UnitEffectResource::Visual(id),
                    kit: Some(kit.id()),
                    sound_entry: if index == 0 { kit.sound_entry_id() } else { 0 },
                    binding: UnitEffectBinding::Attached {
                        owner: Rc::downgrade(owner),
                        model_scale: definition.map_or(1.0, |model| model.attached_effect_scale()),
                        attachment,
                    },
                })
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::application) fn request(
        &self,
        event: &RuntimeM2Event,
        owner: &Rc<UnitAnimationBehavior>,
        model: &DecodedM2Model,
        transform: Mat4,
        world: &ActiveWorld,
        creatures: &CreatureCatalog,
        camera: Vec3,
        enabled: bool,
    ) -> Option<UnitEffectRequest> {
        let marker = event.identifier();
        if marker != *b"$BTH" && !foot_contact(marker) {
            return None;
        }
        let guid = event.owner_guid()?;
        let identity = world.object_identity(guid)?;
        let state = self.units.get(&identity)?;
        let presentation = world.unit_presentation(guid)?;
        let definition = creatures
            .display(presentation.display_id())
            .and_then(|display| creatures.model(display.model_id()));
        let fields = world
            .entity_by_guid(guid)
            .and_then(|entity| world.storage().get::<&ObjectFields>(entity).ok())?;
        let player = world.object_kind(guid) == Some(ObjectKind::Player);
        let (kind, binding) = if marker == *b"$BTH" {
            let kind = state.breath.effect(
                0,
                definition.map(|model| model.flags()),
                player.then(|| {
                    unit_player_inebriation((fields.get(155) >> 8) as u8, fields.get(322))
                }),
            )?;
            let attachment = if model.attachment(17).is_some() {
                17
            } else {
                19
            };
            (
                kind,
                UnitEffectBinding::Attached {
                    owner: Rc::downgrade(owner),
                    model_scale: definition.map_or(1., |model| model.attached_effect_scale()),
                    attachment,
                },
            )
        } else {
            let sample = state.sample?;
            let liquid = sample.liquid?;
            let (kind, position) = UnitWaterSprayInput {
                foot: event.position(),
                camera,
                origin_z: sample.transform.position().z,
                model_height: sample.height,
                liquid_id: liquid.liquid_type,
                liquid_surface: liquid.surface_height,
                movement_flags: sample.movement.flags() as u32,
                mount_display_id: presentation.mount_display_id(),
                unit_bytes1: fields.get(74),
                player_flags: player.then(|| fields.get(150)),
                transport_guid: sample.movement.transport_guid().unwrap_or(0),
                model_flags: definition?.flags(),
                enabled,
                speed: resolve_unit_movement_speed_extended(sample.movement),
                walk_speed: sample.movement.speeds().walk(),
            }
            .resolve()?;
            let bounds = model.bounds();
            (
                kind,
                UnitEffectBinding::Positioned {
                    position,
                    world_factor: unit_world_effect_factor(
                        bounds.minimum(),
                        bounds.maximum(),
                        definition?.world_effect_scale(),
                    ),
                    unit_scale: transform.x_axis.truncate().length(),
                },
            )
        };
        Some(UnitEffectRequest {
            identity,
            lifetime: Rc::downgrade(&state.lifetime),
            kind: kind.into(),
            kit: None,
            sound_entry: 0,
            binding,
        })
    }
}

/// 732650 dispatches all forty left/right contact variants to 723A50.
fn foot_contact(marker: [u8; 4]) -> bool {
    matches!(
        marker,
        [
            b'$',
            b'B' | b'F' | b'R' | b'S' | b'W',
            b'L' | b'R',
            b'0'..=b'3'
        ]
    )
}

#[cfg(test)]
#[path = "../../../tests/application/unit_effect_contacts.rs"]
mod tests;
