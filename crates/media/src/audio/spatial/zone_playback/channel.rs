//! Build 12340 0x004c7140 current/previous generation transitions.

use super::super::ZoneMusicCue;
use crate::audio::backend::{SoundVoiceHandle, SoundVoicePriority, SoundVoiceState};
use crate::audio::engine::{
    SoundChannel, SoundConcurrencyMode, SoundEngine, SoundEngineError, SoundFade,
    SoundFadeDirection, SoundGain, SoundLoadHandle, SoundLoadRequest, SoundLoopMode,
    SoundPlayRequest, SoundPlayback,
};
use crate::audio::selection::SoundVariationMode;
use std::time::Duration;

/// Channel-specific loop policy and transition durations from 0x004c9910.
pub(super) enum ZoneChannelKind {
    Music,
    Ambience,
}

/// Pending or admitted sound; metadata survives replacement until completion.
struct Generation {
    entry_id: u32,
    cue: Option<ZoneMusicCue>,
    load: Option<SoundLoadHandle>,
    voice: Option<SoundVoiceHandle>,
    fade_in: Duration,
    retiring: bool,
}

/// Two revivable slots plus older generations whose fade callbacks still live.
pub(super) struct ZoneChannel {
    kind: ZoneChannelKind,
    current: Option<Generation>,
    previous: Option<Generation>,
    retiring: Vec<Generation>,
}

impl ZoneChannel {
    pub(super) fn new(kind: ZoneChannelKind) -> Self {
        Self {
            kind,
            current: None,
            previous: None,
            retiring: Vec::new(),
        }
    }

    pub(super) fn active_cue(&self) -> Option<ZoneMusicCue> {
        self.current.as_ref().and_then(|generation| generation.cue)
    }

    pub(super) fn active_voice(&self) -> Option<SoundVoiceHandle> {
        self.current
            .as_ref()
            .and_then(|generation| generation.voice)
    }

    /// Uses the stock music four-second and ambience five-second fade-out.
    fn fade_duration(&self) -> Duration {
        Duration::from_secs(match self.kind {
            ZoneChannelKind::Music => 4,
            ZoneChannelKind::Ambience => 5,
        })
    }

    /// Retires completed slots without losing their original callback identity.
    pub(super) fn collect_finished(
        &mut self,
        engine: &mut SoundEngine<'_>,
        finished: &mut Vec<ZoneMusicCue>,
    ) -> Result<(), SoundEngineError> {
        for slot in [&mut self.current, &mut self.previous] {
            if let Some(generation) = slot.as_ref()
                && !generation.is_live(engine)?
                && let Some(generation) = slot.take()
            {
                generation.finish(engine, finished)?;
            }
        }
        let mut index = 0;
        while index < self.retiring.len() {
            if self.retiring[index].is_live(engine)? {
                index += 1;
            } else {
                self.retiring.remove(index).finish(engine, finished)?;
            }
        }
        Ok(())
    }

    /// Fades the current slot when no layer remains; a later return may revive it.
    pub(super) fn silence(&mut self, engine: &mut SoundEngine<'_>) -> Result<(), SoundEngineError> {
        if let Some(mut current) = self.current.take() {
            current.fade_out(engine, self.fade_duration())?;
            if let Some(previous) = self.previous.replace(current) {
                self.retiring.push(previous);
            }
        }
        Ok(())
    }

    /// Reuses a current/previous entry or reserves exactly one selected payload.
    pub(super) fn select(
        &mut self,
        engine: &mut SoundEngine<'_>,
        entry_id: u32,
        cue: Option<ZoneMusicCue>,
        immediate: bool,
        random: &mut impl FnMut() -> u32,
    ) -> Result<Option<SoundLoadRequest>, SoundEngineError> {
        if immediate {
            self.clear(engine)?;
        }
        if self
            .current
            .as_ref()
            .is_some_and(|current| current.entry_id == entry_id)
        {
            return Ok(None);
        }
        let fade_in = match self.kind {
            ZoneChannelKind::Music if self.current.is_none() => Duration::from_millis(10),
            _ if immediate => Duration::ZERO,
            _ => self.fade_duration(),
        };
        if self
            .previous
            .as_ref()
            .is_some_and(|previous| previous.entry_id == entry_id)
        {
            if let Some(current) = self.current.as_mut() {
                let duration = match self.kind {
                    ZoneChannelKind::Music => Duration::from_secs(4),
                    ZoneChannelKind::Ambience => Duration::from_secs(5),
                };
                current.fade_out(engine, duration)?;
            }
            if let Some(previous) = self.previous.as_mut() {
                previous.fade_in(engine, fade_in)?;
            }
            std::mem::swap(&mut self.current, &mut self.previous);
            return Ok(None);
        }
        self.silence(engine)?;
        let (channel, looping) = match self.kind {
            ZoneChannelKind::Music => (SoundChannel::MUSIC, SoundLoopMode::Entry),
            ZoneChannelKind::Ambience => (SoundChannel::AMBIENCE, SoundLoopMode::Loop),
        };
        let request = SoundPlayRequest::new(
            entry_id,
            channel,
            SoundVariationMode::Random,
            looping,
            SoundConcurrencyMode::Entry,
        )
        .with_priority(SoundVoicePriority::new(100));
        let load = engine.begin_load(request, random)?;
        if let Some(load) = &load {
            if !fade_in.is_zero() {
                let mut fade = SoundFade::new(SoundGain::MUTED);
                fade.retarget(SoundFadeDirection::In, fade_in);
                engine.set_load_fade(load.handle(), fade);
            }
            self.current = Some(Generation {
                entry_id,
                cue,
                load: Some(load.handle()),
                voice: None,
                fade_in,
                retiring: false,
            });
        }
        Ok(load)
    }

    /// Applies initial gain before the newly admitted voice reaches the next mix.
    pub(super) fn complete_load(
        &mut self,
        engine: &mut SoundEngine<'_>,
        load: SoundLoadHandle,
        playback: SoundPlayback,
    ) -> Result<(), SoundEngineError> {
        let duration = self.fade_duration();
        for generation in self
            .current
            .iter_mut()
            .chain(self.previous.iter_mut())
            .chain(self.retiring.iter_mut())
        {
            if generation.load != Some(load) {
                continue;
            }
            generation.load = None;
            if let SoundPlayback::Started(voice) = playback {
                generation.voice = Some(voice);
                if generation.retiring {
                    generation.fade_out(engine, duration)?;
                } else if !generation.fade_in.is_zero() {
                    let mut fade = SoundFade::new(SoundGain::MUTED);
                    fade.retarget(SoundFadeDirection::In, generation.fade_in);
                    engine.set_voice_fade(voice, fade)?;
                }
            }
        }
        Ok(())
    }

    /// Teardown owns both pending cancellation and admitted voice retirement.
    pub(super) fn clear(&mut self, engine: &mut SoundEngine<'_>) -> Result<(), SoundEngineError> {
        for generation in self
            .current
            .take()
            .into_iter()
            .chain(self.previous.take())
            .chain(self.retiring.drain(..))
        {
            if let Some(load) = generation.load {
                engine.cancel_load(load);
            }
            if let Some(voice) = generation.voice {
                stop_if_owned(engine, voice)?;
            }
        }
        Ok(())
    }
}

impl Generation {
    /// A pending resource remains a generation until canceled or completed.
    fn is_live(&self, engine: &SoundEngine<'_>) -> Result<bool, SoundEngineError> {
        if self.load.is_some() {
            return Ok(true);
        }
        let Some(voice) = self.voice else {
            return Ok(false);
        };
        match engine.voice_state(voice) {
            Ok(SoundVoiceState::Playing | SoundVoiceState::Paused) => Ok(true),
            Ok(SoundVoiceState::Stopped) | Err(SoundEngineError::UnknownVoice) => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Retargets the envelope from its current gain without restarting playback.
    fn fade_in(
        &mut self,
        engine: &mut SoundEngine<'_>,
        duration: Duration,
    ) -> Result<(), SoundEngineError> {
        self.retiring = false;
        self.fade_in = duration;
        if let Some(voice) = self.voice {
            let mut fade = engine.voice_fade(voice)?;
            fade.retarget(SoundFadeDirection::In, duration);
            engine.set_voice_fade(voice, fade)?;
        }
        Ok(())
    }

    /// Pending generations are canceled before bytes can later start stale audio.
    fn fade_out(
        &mut self,
        engine: &mut SoundEngine<'_>,
        duration: Duration,
    ) -> Result<(), SoundEngineError> {
        self.retiring = true;
        if let Some(load) = self.load.take() {
            engine.cancel_load(load);
        }
        if let Some(voice) = self.voice {
            let mut fade = engine.voice_fade(voice)?;
            fade.retarget(SoundFadeDirection::Out, duration);
            engine.set_voice_fade(voice, fade)?;
        }
        Ok(())
    }

    /// Releases a naturally stopped voice and queues its callback exactly once.
    fn finish(
        self,
        engine: &mut SoundEngine<'_>,
        finished: &mut Vec<ZoneMusicCue>,
    ) -> Result<(), SoundEngineError> {
        if let Some(voice) = self.voice {
            stop_if_owned(engine, voice)?;
        }
        if let Some(cue) = self.cue {
            finished.push(cue);
        }
        Ok(())
    }
}

/// Global fade advancement or voice stealing may already have retired this ID.
fn stop_if_owned(
    engine: &mut SoundEngine<'_>,
    voice: SoundVoiceHandle,
) -> Result<(), SoundEngineError> {
    match engine.stop(voice) {
        Ok(()) | Err(SoundEngineError::UnknownVoice) => Ok(()),
        Err(error) => Err(error),
    }
}
