//! UnitSound_C's retained `$CSD` voice, emote gates, and GUID position binding.

use glam::Vec3;
use solarity_ecs::{ActiveWorld, ObjectFields, ObjectKind, WorldObjectIdentity};
use solarity_media::{
    AdvancedSoundListener, SoundChannel, SoundConcurrencyMode, SoundEngine, SoundEngineError,
    SoundLoadHandle, SoundLoopMode, SoundPlayRequest, SoundPlayback, SoundVariationMode,
    SoundVoiceHandle, SoundVoicePriority, SoundVoiceState,
};

use crate::application::terrain_frame::RuntimeM2Event;
use crate::random::BlizzardRand;

use super::{RuntimeSoundCoordinator, RuntimeSoundError, UnitSoundContext, boolean};

/// Admission or backend ownership of the unit's current emote voice.
enum VocalPlayback {
    Loading(SoundLoadHandle),
    Playing(SoundVoiceHandle),
}

/// One native unit +0x934 handle, separate from footsteps and forced vocals.
pub(super) struct UnitVocal {
    identity: WorldObjectIdentity,
    playback: VocalPlayback,
    entry_id: u32,
    position: Option<Vec3>,
    listener: AdvancedSoundListener,
}

impl UnitVocal {
    /// Releases either admission or playback when this unit voice is replaced.
    fn stop(self, engine: &mut SoundEngine<'_>) -> Result<(), SoundEngineError> {
        match self.playback {
            VocalPlayback::Loading(load) => {
                engine.cancel_load(load);
                Ok(())
            }
            VocalPlayback::Playing(voice) => match engine.stop(voice) {
                Ok(()) | Err(SoundEngineError::UnknownVoice) => Ok(()),
                Err(error) => Err(error),
            },
        }
    }

    /// Recognizes completion, cancellation, and backend retirement alike.
    fn is_live(&self, engine: &SoundEngine<'_>) -> Result<bool, SoundEngineError> {
        match self.playback {
            VocalPlayback::Loading(load) => Ok(engine.is_load_pending(load)),
            VocalPlayback::Playing(voice) => match engine.voice_state(voice) {
                Ok(SoundVoiceState::Playing | SoundVoiceState::Paused) => Ok(true),
                Ok(SoundVoiceState::Stopped) | Err(SoundEngineError::UnknownVoice) => Ok(false),
                Err(error) => Err(error),
            },
        }
    }
}

impl RuntimeSoundCoordinator {
    /// Retires all retained unit vocals at world teardown.
    pub(super) fn clear_unit_vocals(&mut self) -> Result<(), RuntimeSoundError> {
        self.engine.with_engine_mut(|engine| {
            for vocal in self.unit_vocals.drain(..) {
                vocal.stop(engine)?;
            }
            Ok::<_, SoundEngineError>(())
        })?;
        Ok(())
    }

    /// Follows current object origins and retires voices from removed generations.
    pub(super) fn update_unit_vocals(
        &mut self,
        world: &ActiveWorld,
        listener: AdvancedSoundListener,
    ) -> Result<(), RuntimeSoundError> {
        self.engine.with_engine_mut(|engine| {
            let mut index = 0;
            while index < self.unit_vocals.len() {
                let vocal = &mut self.unit_vocals[index];
                if world.object_identity(vocal.identity.guid()) != Some(vocal.identity)
                    || !vocal.is_live(engine)?
                {
                    self.unit_vocals.remove(index).stop(engine)?;
                    continue;
                }
                // 879F70's callback 4C5D60 resolves the object's current origin,
                // replacing the initial attachment position (not adding an offset).
                if vocal.position.is_some()
                    && let Some(transform) = world.object_transform(vocal.identity.guid())
                {
                    vocal.position = Some(transform.position());
                    match vocal.playback {
                        VocalPlayback::Playing(voice) => engine.set_voice_world_position(
                            voice,
                            vocal.entry_id,
                            listener,
                            transform.position(),
                        )?,
                        VocalPlayback::Loading(load) => {
                            engine.set_load_world_position(
                                load,
                                vocal.entry_id,
                                listener,
                                transform.position(),
                            )?;
                        }
                    }
                }
                index += 1;
            }
            Ok::<_, SoundEngineError>(())
        })?;
        Ok(())
    }

    /// Transfers admission ownership to the completed backend voice.
    pub(super) fn complete_vocal_load(
        &mut self,
        handle: SoundLoadHandle,
        playback: SoundPlayback,
    ) -> Result<(), RuntimeSoundError> {
        let Some(index) = self.unit_vocals.iter().position(
            |vocal| matches!(vocal.playback, VocalPlayback::Loading(load) if load == handle),
        ) else {
            return Ok(());
        };
        let SoundPlayback::Started(voice) = playback else {
            self.unit_vocals.remove(index);
            return Ok(());
        };
        let vocal = &mut self.unit_vocals[index];
        vocal.playback = VocalPlayback::Playing(voice);
        if let Some(position) = vocal.position {
            self.engine.with_engine_mut(|engine| {
                engine.set_voice_world_position(
                    voice,
                    vocal.entry_id,
                    self.world_listener.unwrap_or(vocal.listener),
                    position,
                )
            })?;
        }
        Ok(())
    }

    /// 732BCD routes $CSD to 746D60 with the emote gate enabled and no forced
    /// file slot. This is a retained unit voice, not the generic model callback.
    pub(super) fn play_unit_vocal(
        &mut self,
        event: &RuntimeM2Event,
        context: &UnitSoundContext<'_>,
        listener: AdvancedSoundListener,
        random: &mut BlizzardRand,
    ) -> Result<(), RuntimeSoundError> {
        let Some(guid) = event.owner_guid().filter(|_| event.sound_kind().is_none()) else {
            return Ok(());
        };
        let kind = context.world.object_kind(guid);
        if !matches!(kind, Some(ObjectKind::Unit | ObjectKind::Player))
            || !boolean(context.cvars, "Sound_EnableEmoteSounds")?
        {
            return Ok(());
        }
        // 746D80 tests the player's signed health; 746E08 tests UNIT_PETNUMBER.
        if kind == Some(ObjectKind::Player)
            && context
                .world
                .unit_vitals(guid)
                .is_none_or(|vitals| (vitals.health() as i32) <= 0)
        {
            return Ok(());
        }
        let pet = context
            .world
            .entity_by_guid(guid)
            .and_then(|entity| context.world.storage().get::<&ObjectFields>(entity).ok())
            .is_some_and(|fields| fields.get(75) != 0);
        if pet && !boolean(context.cvars, "Sound_EnablePetSounds")? {
            return Ok(());
        }
        let Some(identity) = context.world.object_identity(guid) else {
            return Ok(());
        };
        let local = context.world.local_player_guid().ok() == Some(guid);
        let centered = local && boolean(context.cvars, "Sound_ListenerAtCharacter")?;
        // 746F44 opts+0x24=2 clears entry exclusivity; the unit handle supplies
        // replacement. opts+0x1c stays Entry and opts+0x20 gives local priority110.
        let request = SoundPlayRequest::new(
            event.data(),
            SoundChannel::SFX,
            SoundVariationMode::Random,
            SoundLoopMode::Entry,
            SoundConcurrencyMode::Concurrent,
        )
        .with_priority(if local {
            SoundVoicePriority::new(110)
        } else {
            SoundVoicePriority::DEFAULT
        })
        .with_gain_multiplier(if centered { 0.65 } else { 1.0 })?;
        let result = self.engine.with_engine_mut(|engine| {
            let load = if centered {
                engine.begin_load(request, &mut || random.next_u32())?
            } else {
                engine.begin_positioned_load(request, listener, event.position(), &mut || {
                    random.next_u32()
                })?
            };
            Ok::<_, SoundEngineError>(load)
        });
        match result {
            Ok(Some(load)) => {
                if let Some(index) = self
                    .unit_vocals
                    .iter()
                    .position(|vocal| vocal.identity == identity)
                {
                    let previous = self.unit_vocals.remove(index);
                    self.engine
                        .with_engine_mut(|engine| previous.stop(engine))?;
                }
                self.unit_vocals.push(UnitVocal {
                    identity,
                    playback: VocalPlayback::Loading(load.handle()),
                    entry_id: event.data(),
                    position: (!centered).then_some(event.position()),
                    listener,
                });
                self.loader.queue(load);
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(sound_entry_id = event.data(), guid, %error, "unit vocal could not play")
            }
        }
        Ok(())
    }
}
