//! Asynchronous model callbacks and their native retained loop handles.

#[cfg(test)]
#[path = "../../../tests/application/model_sound.rs"]
mod tests;

use glam::Vec3;
use solarity_media::{
    AdvancedSoundListener, SoundChannel, SoundConcurrencyMode, SoundEngine, SoundEngineError,
    SoundFade, SoundFadeDirection, SoundGain, SoundLoadHandle, SoundLoopMode, SoundPlayRequest,
    SoundPlayback, SoundVariationMode, SoundVoiceHandle, SoundVoiceState,
};
use solarity_rendering::WorldCameraFrame;

use crate::application::terrain_frame::RuntimeM2Event;
use crate::application::terrain_frame::m2::sound::{M2SoundKind, M2SoundOwner};
use crate::random::BlizzardRand;

use super::{RuntimeSoundCoordinator, RuntimeSoundError};

enum ModelPlayback {
    Loading(SoundLoadHandle),
    Playing(SoundVoiceHandle),
}

/// One reservation or physical voice; only loop callbacks retain a model owner.
pub(super) struct ModelSound {
    owner: Option<M2SoundOwner>,
    playback: ModelPlayback,
    entry_id: u32,
    position: Vec3,
    listener: AdvancedSoundListener,
    unit: Option<solarity_ecs::WorldObjectIdentity>,
    centered: bool,
    looping: bool,
}

impl ModelSound {
    fn is_live(&self, engine: &SoundEngine<'_>) -> Result<bool, SoundEngineError> {
        if self.owner.as_ref().is_some_and(|owner| !owner.is_live()) {
            return Ok(false);
        }
        match self.playback {
            ModelPlayback::Loading(load) => Ok(engine.is_load_pending(load)),
            ModelPlayback::Playing(voice) => match engine.voice_state(voice) {
                Ok(SoundVoiceState::Playing | SoundVoiceState::Paused) => Ok(true),
                Ok(SoundVoiceState::Stopped) | Err(SoundEngineError::UnknownVoice) => Ok(false),
                Err(error) => Err(error),
            },
        }
    }

    fn stop(self, engine: &mut SoundEngine<'_>) -> Result<(), SoundEngineError> {
        let voice = match self.playback {
            ModelPlayback::Loading(load) => {
                engine.cancel_load(load);
                return Ok(());
            }
            ModelPlayback::Playing(voice) => voice,
        };
        let seconds = self
            .owner
            .as_ref()
            .map_or(0.0, |owner| owner.kind().fade_out_seconds());
        let result = if seconds == 0.0 {
            engine.stop(voice)
        } else {
            // 879840 detaches the logical handle while its fade tail continues.
            match engine.voice_fade(voice) {
                Ok(mut fade) => {
                    fade.retarget_seconds(SoundFadeDirection::Out, seconds);
                    engine.set_voice_fade(voice, fade)
                }
                Err(error) => Err(error),
            }
        };
        match result {
            Ok(()) | Err(SoundEngineError::UnknownVoice) => Ok(()),
            Err(error) => Err(error),
        }
    }
}

impl RuntimeSoundCoordinator {
    /// 6F9840 / 6F7B00 keep a CEffect sound handle and bind it to its unit.
    pub(super) fn play_unit_effect_sounds(
        &mut self,
        events: &[RuntimeM2Event],
        context: &super::UnitSoundContext<'_>,
        listener: AdvancedSoundListener,
        random: &mut BlizzardRand,
    ) -> Result<(), RuntimeSoundError> {
        self.collect_model_sounds()?;
        for sound in &mut self.model_sounds {
            let Some(identity) = sound.unit else {
                continue;
            };
            if sound.centered || context.world.object_identity(identity.guid()) != Some(identity) {
                continue;
            }
            if let Some(transform) = context.world.object_transform(identity.guid()) {
                sound.position = transform.position();
                self.engine.with_engine_mut(|engine| match sound.playback {
                    ModelPlayback::Playing(voice) => engine.set_voice_world_position(
                        voice,
                        sound.entry_id,
                        listener,
                        sound.position,
                    ),
                    ModelPlayback::Loading(load) => engine
                        .set_load_world_position(load, sound.entry_id, listener, sound.position)
                        .map(|_| ()),
                })?;
            }
        }
        for event in events.iter().filter(|event| {
            event.sound_kind() == Some(M2SoundKind::UnitEffect) && event.identifier() == *b"$SND"
        }) {
            let Some(owner) = event.sound_owner().filter(|owner| owner.is_live()) else {
                continue;
            };
            let Some(identity) = event
                .owner_guid()
                .and_then(|guid| context.world.object_identity(guid))
            else {
                continue;
            };
            let local = context.world.local_player_guid().ok() == Some(identity.guid());
            let centered = local && super::boolean(context.cvars, "Sound_ListenerAtCharacter")?;
            let request = unit_effect_request(event.data(), event.is_effect_kit_sound(), local)
                .with_gain_multiplier(unit_effect_gain(centered))?;
            let result = self.engine.with_engine_mut(|engine| {
                let looping =
                    !event.is_effect_kit_sound() && engine.sound_entry_loops(event.data());
                let load = if centered {
                    engine.begin_load(request, &mut || random.next_u32())?
                } else {
                    engine.begin_positioned_load(
                        request,
                        listener,
                        event.position(),
                        &mut || random.next_u32(),
                    )?
                };
                Ok::<_, SoundEngineError>((load, looping))
            });
            match result {
                Ok((Some(load), looping)) => {
                    // 4C6A40 checks admission before immediately replacing
                    // the retained handle (4C6390 / 879840), even a one-shot.
                    if let Some(index) = self.model_sounds.iter().position(|sound| {
                        sound
                            .owner
                            .as_ref()
                            .is_some_and(|current| current.same_model(owner))
                    }) {
                        let mut sound = self.model_sounds.remove(index);
                        sound.owner = None;
                        self.engine.with_engine_mut(|engine| sound.stop(engine))?;
                    }
                    self.model_sounds.push(ModelSound {
                        owner: Some(owner.clone()),
                        playback: ModelPlayback::Loading(load.handle()),
                        entry_id: event.data(),
                        position: event.position(),
                        listener,
                        unit: Some(identity),
                        centered,
                        looping,
                    });
                    self.loader.queue(load);
                }
                Ok((None, _))
                | Err(
                    SoundEngineError::ExclusiveEntryActive { .. }
                    | SoundEngineError::ChannelCapacity { .. },
                ) => {}
                Err(error) => {
                    tracing::warn!(sound_entry_id = event.data(), %error, "unit effect sound could not play")
                }
            }
        }
        Ok(())
    }

    /// Cancels pending bytes or retires the retained handle when its model dies.
    pub(super) fn collect_model_sounds(&mut self) -> Result<(), RuntimeSoundError> {
        self.engine.with_engine_mut(|engine| {
            let mut index = 0;
            while index < self.model_sounds.len() {
                let sound = &mut self.model_sounds[index];
                if !sound.looping
                    && sound.owner.as_ref().is_some_and(|owner| {
                        owner.kind() == M2SoundKind::UnitEffect && !owner.is_live()
                    })
                {
                    // 6F87C0 releases a one-shot handle without stopping its tail.
                    sound.owner = None;
                    sound.unit = None;
                }
                if self.model_sounds[index].is_live(engine)? {
                    index += 1;
                } else {
                    self.model_sounds.remove(index).stop(engine)?;
                }
            }
            Ok::<_, SoundEngineError>(())
        })?;
        Ok(())
    }

    pub(super) fn clear_model_sounds(&mut self) -> Result<(), RuntimeSoundError> {
        self.engine.with_engine_mut(|engine| {
            for sound in self.model_sounds.drain(..) {
                sound.stop(engine)?;
            }
            Ok::<_, SoundEngineError>(())
        })?;
        Ok(())
    }

    pub(super) fn complete_model_load(
        &mut self,
        handle: SoundLoadHandle,
        playback: SoundPlayback,
    ) -> Result<(), RuntimeSoundError> {
        let Some(index) = self.model_sounds.iter().position(
            |sound| matches!(sound.playback, ModelPlayback::Loading(load) if load == handle),
        ) else {
            return Ok(());
        };
        let SoundPlayback::Started(voice) = playback else {
            self.model_sounds.remove(index);
            return Ok(());
        };
        let sound = &mut self.model_sounds[index];
        sound.playback = ModelPlayback::Playing(voice);
        self.engine.with_engine_mut(|engine| {
            if sound.centered {
                return Ok(());
            }
            // 8793C0 copies the event's world position; it is not a bone pointer.
            engine.set_voice_world_position(
                voice,
                sound.entry_id,
                self.world_listener.unwrap_or(sound.listener),
                sound.position,
            )?;
            Ok::<_, SoundEngineError>(())
        })?;
        self.collect_model_sounds()
    }

    /// Preserves callback order while archive reads and decoding run on workers.
    pub(crate) fn play_m2_events(
        &mut self,
        events: &[RuntimeM2Event],
        camera: WorldCameraFrame,
        random: &mut BlizzardRand,
    ) -> Result<usize, RuntimeSoundError> {
        self.collect_model_sounds()?;
        let listener = self
            .world_listener
            .unwrap_or_else(|| AdvancedSoundListener::from_world_camera(camera));
        let mut dispatched = 0;
        for event in events {
            if event.sound_kind() == Some(M2SoundKind::UnitEffect) {
                continue;
            }
            let owner = match &event.identifier() {
                b"$DSL" => {
                    let Some(owner) = event.sound_owner().filter(|owner| owner.is_live()) else {
                        continue;
                    };
                    if self.model_sounds.iter().any(|sound| {
                        sound
                            .owner
                            .as_ref()
                            .is_some_and(|current| current.same_model(owner))
                    }) || self.engine.with_engine(|engine| {
                        engine.has_nearby_entry(event.data(), event.position())
                    })? {
                        continue;
                    }
                    Some(owner.clone())
                }
                b"$DSE" => {
                    if let Some(owner) = event
                        .sound_owner()
                        .filter(|owner| owner.kind() == M2SoundKind::Doodad)
                        && let Some(index) = self.model_sounds.iter().position(|sound| {
                            sound
                                .owner
                                .as_ref()
                                .is_some_and(|current| current.same_model(owner))
                        })
                    {
                        let sound = self.model_sounds.remove(index);
                        self.engine.with_engine_mut(|engine| sound.stop(engine))?;
                        dispatched += 1;
                    }
                    continue;
                }
                b"$SND" if event.sound_kind() != Some(M2SoundKind::Doodad) => None,
                b"$DSO" => None,
                _ => continue,
            };
            let request = callback_request(event.data(), owner.as_ref().map(M2SoundOwner::kind));
            let result = self.engine.with_engine_mut(|engine| {
                let load = engine.begin_positioned_load(
                    request,
                    listener,
                    event.position(),
                    &mut || random.next_u32(),
                )?;
                if let (Some(load), Some(owner)) = (&load, &owner) {
                    // Start muted inside admission, before the audio thread can
                    // mix a frame from the newly completed resource.
                    let mut fade = SoundFade::new(SoundGain::MUTED);
                    fade.retarget_seconds(SoundFadeDirection::In, owner.kind().fade_in_seconds());
                    engine.set_load_fade(load.handle(), fade);
                }
                Ok::<_, SoundEngineError>(load)
            });
            match result {
                Ok(Some(load)) => {
                    self.model_sounds.push(ModelSound {
                        looping: owner.is_some(),
                        owner,
                        playback: ModelPlayback::Loading(load.handle()),
                        entry_id: event.data(),
                        position: event.position(),
                        listener,
                        unit: None,
                        centered: false,
                    });
                    self.loader.queue(load);
                }
                Ok(None) => {}
                // Native model callbacks ignore ordinary admission rejection.
                // Repeated animation events for an exclusive ambient loop are
                // expected and must not produce a warning on every attempt.
                Err(
                    SoundEngineError::ExclusiveEntryActive { .. }
                    | SoundEngineError::ChannelCapacity { .. },
                ) => {}
                Err(error) => {
                    // These callbacks ignore native play status; failed content
                    // or channel admission must not terminate model presentation.
                    tracing::warn!(sound_entry_id = event.data(), %error, "model sound could not play");
                }
            }
            dispatched += 1;
        }
        Ok(dispatched)
    }
}

fn callback_request(entry_id: u32, kind: Option<M2SoundKind>) -> SoundPlayRequest {
    // 70C282 / 7BD741 write options +0x1c = 1 (force looping),
    // while +0x18 keeps 4C5990's default random variation selector.
    // 4C6CB2 forces Once for callbacks without a retained handle.
    SoundPlayRequest::new(
        entry_id,
        SoundChannel::SFX,
        SoundVariationMode::Random,
        if kind.is_some() {
            SoundLoopMode::Loop
        } else {
            SoundLoopMode::Once
        },
        SoundConcurrencyMode::Entry,
    )
}

fn unit_effect_gain(centered: bool) -> f32 {
    if centered { 0.65 } else { 1.0 }
}

fn unit_effect_request(entry: u32, kit: bool, local: bool) -> SoundPlayRequest {
    SoundPlayRequest::new(
        entry,
        SoundChannel::SFX,
        SoundVariationMode::Random,
        if kit {
            SoundLoopMode::Once
        } else {
            SoundLoopMode::Entry
        },
        SoundConcurrencyMode::Entry,
    )
    .with_priority(if local {
        solarity_media::SoundVoicePriority::new(110)
    } else {
        solarity_media::SoundVoicePriority::DEFAULT
    })
}
