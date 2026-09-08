//! Native unit movement notifications and `$FSD` model sound callbacks.

use glam::Vec3;
use solarity_asset::{
    CharacterRaceCatalog, CreatureCatalog, CreatureMovementSounds, ItemDefinitionCatalog,
    MovementSoundCatalog,
};
use solarity_ecs::{ActiveWorld, ObjectFields, ObjectKind, PlayerEquipment, PlayerEquipmentSlot};
use solarity_media::{
    AdvancedSoundListener, SoundChannel, SoundConcurrencyMode, SoundEngineError, SoundLoadHandle,
    SoundLoopMode, SoundPlayRequest, SoundPlayback, SoundVariationMode, SoundVoicePriority,
};
use solarity_rendering::WorldCameraFrame;
use solarity_systems::{UnitMovementAnimationDecision, resolve_unit_landing_animation};

use crate::application::terrain_frame::RuntimeM2Event;
use crate::application::unit_animation::{
    UnitMovementAnimationEvent, UnitMovementAnimationEventKind,
};
use crate::random::BlizzardRand;

use super::{RuntimeSoundCoordinator, RuntimeSoundError, SoundCvarSource, boolean};

#[cfg(test)]
#[path = "../../../tests/application/movement_sound.rs"]
mod tests;

/// Per-frame borrowed sources; audio never owns another copy of unit state.
pub(in crate::application) struct UnitSoundContext<'a> {
    pub world: &'a ActiveWorld,
    pub creatures: &'a CreatureCatalog,
    pub items: &'a ItemDefinitionCatalog,
    pub races: &'a CharacterRaceCatalog,
    pub cvars: &'a dyn SoundCvarSource,
}

struct UnitSoundSource {
    sounds: Option<CreatureMovementSounds>,
    armor_material: u32,
    player: bool,
    local: bool,
}

pub(super) struct UnitSoundLoad {
    handle: SoundLoadHandle,
    entry_id: u32,
    position: Option<Vec3>,
    listener: AdvancedSoundListener,
}

#[derive(Clone, Copy)]
struct UnitSoundOptions {
    local: bool,
    footstep: bool,
}

impl UnitSoundOptions {
    fn request(self, entry: u32) -> SoundPlayRequest {
        let channel = if self.footstep {
            if self.local {
                SoundChannel::LOCAL_FOOTSTEP
            } else {
                SoundChannel::OTHER_FOOTSTEP
            }
        } else {
            SoundChannel::SFX
        };
        let priority = if self.local {
            SoundVoicePriority::new(if self.footstep { 115 } else { 110 })
        } else {
            SoundVoicePriority::DEFAULT
        };
        // 4C5990 initializes selection mode 2; 4C6A40 forces one shot when
        // the caller supplies no retained sound handle.
        SoundPlayRequest::new(
            entry,
            channel,
            SoundVariationMode::Random,
            SoundLoopMode::Once,
            SoundConcurrencyMode::Entry,
        )
        .with_priority(priority)
    }
}

impl UnitSoundContext<'_> {
    fn source(&self, guid: u64, sounds: &MovementSoundCatalog) -> Option<UnitSoundSource> {
        let presentation = self.world.unit_presentation(guid)?;
        let display = self.creatures.display(presentation.display_id())?;
        let model = self.creatures.model(display.model_id())?;
        // 0x0072DA42 first resolves the display override, then the model row.
        let base = sounds
            .creature(display.sound_id())
            .or_else(|| sounds.creature(model.sound_id()));
        let creature = if presentation.mount_display_id() != 0 {
            self.creatures
                .display(presentation.mount_display_id())
                .and_then(|display| {
                    sounds.creature(display.sound_id()).or_else(|| {
                        self.creatures
                            .model(display.model_id())
                            .and_then(|model| sounds.creature(model.sound_id()))
                    })
                })
                .or_else(|| {
                    base.and_then(|base| {
                        if base.child() > 0 {
                            sounds.creature(base.child())
                        } else {
                            Some(base)
                        }
                    })
                })
        } else {
            base
        };
        let player = self.world.object_kind(guid) == Some(ObjectKind::Player);
        let armor_material = if player {
            self.world
                .entity_by_guid(guid)
                .and_then(|entity| self.world.storage().get::<&PlayerEquipment>(entity).ok())
                .and_then(|equipment| {
                    self.items
                        .item(equipment.item(PlayerEquipmentSlot::Chest).entry_id())
                })
                .map_or(0, |item| item.material_id() as u32)
        } else {
            model.foley_material_id()
        };
        Some(UnitSoundSource {
            sounds: creature,
            armor_material,
            player,
            local: self.world.local_player_guid().ok() == Some(guid),
        })
    }
}

impl RuntimeSoundCoordinator {
    /// Queues 746720's race-authored splash independently of model callbacks.
    pub(in crate::application) fn notify_water_splash(
        &mut self,
        event: crate::application::unit_water::UnitWaterSplash,
    ) {
        self.water_splashes.push_back(event);
    }

    pub(crate) fn notify_unit_movement(&mut self, event: UnitMovementAnimationEvent) {
        if !matches!(event.kind, UnitMovementAnimationEventKind::Changed) {
            self.movement_events.push_back(event);
        }
    }

    /// Drains forced vocals once and processes footsteps at authored M2 times.
    pub(in crate::application) fn play_unit_events(
        &mut self,
        events: &[RuntimeM2Event],
        camera: WorldCameraFrame,
        context: UnitSoundContext<'_>,
        mut surface: impl FnMut(
            u64,
            Vec3,
            &MovementSoundCatalog,
        ) -> Result<(u32, bool), RuntimeSoundError>,
        random: &mut BlizzardRand,
    ) -> Result<(), RuntimeSoundError> {
        self.movement_voices.retain(|handle| {
            !matches!(
                self.engine
                    .with_engine(|engine| engine.voice_state(*handle)),
                Err(SoundEngineError::UnknownVoice)
            )
        });
        let listener = self
            .world_listener
            .unwrap_or_else(|| AdvancedSoundListener::from_world_camera(camera));
        let at_character = boolean(context.cvars, "Sound_ListenerAtCharacter")?;
        self.update_unit_vocals(context.world, listener)?;
        self.play_unit_effect_sounds(events, &context, listener, random)?;
        while let Some(event) = self.water_splashes.pop_front() {
            if context.world.object_identity(event.identity.guid()) != Some(event.identity) {
                continue;
            }
            let Some(race_id) = context
                .world
                .entity_by_guid(event.identity.guid())
                .and_then(|entity| {
                    context
                        .world
                        .storage()
                        .get::<&solarity_ecs::UnitIdentity>(entity)
                        .ok()
                        .map(|unit| u32::from(unit.race_id()))
                })
            else {
                continue;
            };
            let Some(race) = context.races.race(race_id) else {
                continue;
            };
            let local = context.world.local_player_guid().ok() == Some(event.identity.guid());
            // 746720 supplies the unit's actual position, with no vocal Z offset.
            self.play_unit_entry(
                race.splash_sound_id(),
                (!local || !at_character).then_some(event.position),
                UnitSoundOptions {
                    local,
                    footstep: false,
                },
                listener,
                random,
            )?;
        }
        while let Some(event) = self.movement_events.pop_front() {
            if context.world.object_identity(event.identity.guid()) != Some(event.identity) {
                continue;
            }
            let Some(source) = context.source(event.identity.guid(), &self.movement_sounds) else {
                continue;
            };
            let Some(sounds) = source.sounds else {
                continue;
            };
            let entry = match event.kind {
                UnitMovementAnimationEventKind::Jump => sounds.jump(),
                UnitMovementAnimationEventKind::Land {
                    previous_flags,
                    forced,
                    slow,
                } if matches!(
                    resolve_unit_landing_animation(
                        previous_flags,
                        event.movement.flags() as u32,
                        forced,
                        slow
                    ),
                    UnitMovementAnimationDecision::Select(39 | 187)
                ) =>
                {
                    sounds.land()
                }
                _ => 0,
            };
            let Some(transform) = context.world.object_transform(event.identity.guid()) else {
                continue;
            };
            let position = transform.position()
                + if source.player {
                    Vec3::Z * 2.0
                } else {
                    Vec3::ZERO
                };
            self.play_unit_entry(
                entry,
                (!source.local || !at_character).then_some(position),
                UnitSoundOptions {
                    local: source.local,
                    footstep: false,
                },
                listener,
                random,
            )?;
        }
        let footsteps = boolean(context.cvars, "FootstepSounds")?;
        for event in events {
            match &event.identifier() {
                b"$CSD" => {
                    self.play_unit_vocal(event, &context, listener, random)?;
                    continue;
                }
                b"$FSD" => {}
                _ => continue,
            }
            let Some(guid) = event.owner_guid() else {
                continue;
            };
            let Some(source) = context.source(guid, &self.movement_sounds) else {
                continue;
            };
            // Unit bytes 1, byte 2 bit 1 (hover), and Player flags bit 4 (ghost).
            // Native 74724C/747294 rejects these before either foley or steps.
            if context
                .world
                .entity_by_guid(guid)
                .and_then(|entity| context.world.storage().get::<&ObjectFields>(entity).ok())
                .is_some_and(|fields| {
                    fields.get(74) & 0x0002_0000 != 0
                        || source.player && fields.get(150) & 0x10 != 0
                })
            {
                continue;
            }
            // 0x00747259 suppresses movement's non-colliding flight mode.
            if context
                .world
                .movement_state(guid)
                .is_some_and(|movement| movement.flags() & 0x4000_0000 != 0)
            {
                continue;
            }
            let armor_enabled = boolean(
                context.cvars,
                if source.local {
                    "Sound_EnableArmorFoleySoundForSelf"
                } else {
                    "Sound_EnableArmorFoleySoundForOthers"
                },
            )?;
            if armor_enabled {
                let position = context
                    .world
                    .object_transform(guid)
                    .map_or(event.position(), |value| value.position())
                    + Vec3::Z * 2.0;
                self.play_unit_entry(
                    self.movement_sounds.armor(source.armor_material),
                    (!source.local || !at_character).then_some(position),
                    UnitSoundOptions {
                        local: source.local,
                        footstep: false,
                    },
                    listener,
                    random,
                )?;
            }
            if footsteps
                && let Some(sounds) = source.sounds.filter(|sounds| sounds.footsteps() != 0)
            {
                let (terrain, wet) = surface(guid, event.position(), &self.movement_sounds)?;
                let entry = self
                    .movement_sounds
                    .footstep(sounds.footsteps(), terrain, wet);
                self.play_unit_entry(
                    entry,
                    (!source.local || !at_character).then_some(event.position()),
                    UnitSoundOptions {
                        local: source.local,
                        footstep: true,
                    },
                    listener,
                    random,
                )?;
            }
        }
        Ok(())
    }

    fn play_unit_entry(
        &mut self,
        entry: u32,
        position: Option<Vec3>,
        options: UnitSoundOptions,
        listener: AdvancedSoundListener,
        random: &mut BlizzardRand,
    ) -> Result<(), RuntimeSoundError> {
        if entry == 0 {
            return Ok(());
        }
        let request = options
            .request(entry)
            .with_gain_multiplier(if position.is_none() { 0.65 } else { 1.0 })?;
        let result = self.engine.with_engine_mut(|engine| {
            let load = match position {
                Some(position) => {
                    engine.begin_positioned_load(request, listener, position, &mut || {
                        random.next_u32()
                    })?
                }
                None => engine.begin_load(request, &mut || random.next_u32())?,
            };
            Ok::<_, SoundEngineError>(load)
        });
        match result {
            Ok(Some(load)) => {
                self.movement_loads.push(UnitSoundLoad {
                    handle: load.handle(),
                    entry_id: entry,
                    position,
                    listener,
                });
                self.loader.queue(load);
            }
            Ok(None) => {}
            Err(
                solarity_media::SoundEngineError::ChannelCapacity { .. }
                | solarity_media::SoundEngineError::ExclusiveEntryActive { .. },
            ) => {
                tracing::trace!(
                    sound_entry_id = entry,
                    "unit sound declined by stock voice admission"
                );
            }
            Err(error) => {
                // UnitSound_C ignores the ordinary play status. A missing HD
                // override or rejected voice must not terminate world presentation.
                tracing::warn!(sound_entry_id = entry, %error, "unit movement sound could not play");
            }
        }
        Ok(())
    }

    pub(super) fn complete_unit_load(
        &mut self,
        handle: SoundLoadHandle,
        playback: SoundPlayback,
    ) -> Result<(), RuntimeSoundError> {
        let Some(index) = self
            .movement_loads
            .iter()
            .position(|load| load.handle == handle)
        else {
            return Ok(());
        };
        let load = self.movement_loads.remove(index);
        if let SoundPlayback::Started(voice) = playback {
            self.engine.with_engine_mut(|engine| {
                if let Some(position) = load.position {
                    engine.set_voice_world_position(
                        voice,
                        load.entry_id,
                        self.world_listener.unwrap_or(load.listener),
                        position,
                    )
                } else {
                    Ok(())
                }
            })?;
            self.movement_voices.push(voice);
        }
        Ok(())
    }

    pub(super) fn clear_unit_sounds(&mut self) -> Result<(), RuntimeSoundError> {
        self.engine.with_engine_mut(|engine| {
            for load in self.movement_loads.drain(..) {
                engine.cancel_load(load.handle);
            }
            for voice in self.movement_voices.drain(..) {
                match engine.stop(voice) {
                    Ok(()) | Err(SoundEngineError::UnknownVoice) => {}
                    Err(error) => return Err(error),
                }
            }
            Ok::<_, SoundEngineError>(())
        })?;
        Ok(())
    }
}
