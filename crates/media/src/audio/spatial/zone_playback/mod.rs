//! Zone-owned asynchronous generations and native two-slot crossfades.

mod channel;

use solarity_asset::{AreaSoundReferences, ZoneSoundCatalog};

use super::{ZoneMusicCue, ZoneMusicSelection, ZoneSoundFrame, ZoneSoundLayer, ZoneSoundState};
use crate::audio::backend::SoundVoiceHandle;
use crate::audio::engine::{
    SoundEngine, SoundEngineError, SoundLoadHandle, SoundLoadRequest, SoundPlayback,
};
use channel::{ZoneChannel, ZoneChannelKind};

/// Owns location selection, pending loads, crossfades, and completion callbacks.
/// The application advances engine fades once per frame and delivers every load
/// completion here; this service never reads archives on the render thread.
pub struct ZoneSoundService {
    catalog: ZoneSoundCatalog,
    state: ZoneSoundState,
    music: ZoneChannel,
    ambience: ZoneChannel,
    underwater_changed: bool,
}

impl ZoneSoundService {
    /// Creates the process-owned selector with no active world generations.
    pub fn new(catalog: ZoneSoundCatalog) -> Self {
        Self {
            catalog,
            state: ZoneSoundState::default(),
            music: ZoneChannel::new(ZoneChannelKind::Music),
            ambience: ZoneChannel::new(ZoneChannelKind::Ambience),
            underwater_changed: false,
        }
    }

    /// Changes one authored location layer; call only when its relations change.
    pub fn set_location(
        &mut self,
        layer: ZoneSoundLayer,
        references: Option<AreaSoundReferences>,
        mut frame: ZoneSoundFrame,
    ) {
        frame.active = self.active_music();
        self.state
            .set_location(layer, references, &self.catalog, frame);
    }

    /// Applies WMO inheritance suppression and immediate underwater transitions.
    pub fn set_environment(
        &mut self,
        world_model_only: bool,
        underwater_liquid_id: u32,
        provider_id: u32,
    ) {
        self.state.set_world_model_only(world_model_only);
        self.underwater_changed |= self.state.set_underwater(underwater_liquid_id, provider_id);
    }

    /// Returns the selected DSP provider for the separate effects owner.
    pub fn sound_provider_id(&self) -> u32 {
        self.state.sound_provider_id()
    }

    /// Returns the current generation's original completion metadata.
    pub fn active_music(&self) -> Option<ZoneMusicCue> {
        self.music.active_cue()
    }

    /// Returns the current admitted music voice, absent while loading or silent.
    pub fn music_voice(&self) -> Option<SoundVoiceHandle> {
        self.music.active_voice()
    }

    /// Observes natural finishes, then selects and queues ordered replacements.
    /// `frame.active` is replaced with this owner's current generation. Direct
    /// script-file playback suppresses zone music until that script voice ends.
    ///
    /// # Errors
    /// Returns a selection, admission, or backend lifecycle failure.
    pub fn update(
        &mut self,
        engine: &mut SoundEngine<'_>,
        mut frame: ZoneSoundFrame,
        script_music: bool,
        next_random_word: &mut impl FnMut() -> u32,
    ) -> Result<Vec<SoundLoadRequest>, SoundEngineError> {
        let mut finished = Vec::new();
        self.music.collect_finished(engine, &mut finished)?;
        self.ambience.collect_finished(engine, &mut finished)?;
        for cue in finished {
            frame.active = self.active_music();
            self.state
                .finished(cue, frame, &mut |upper| next_random_word() % upper);
        }
        frame.active = self.active_music();
        let mut loads = Vec::with_capacity(2);
        let music = if script_music {
            ZoneMusicSelection::Silence
        } else {
            self.state.music(frame)
        };
        match music {
            ZoneMusicSelection::Delay => {}
            ZoneMusicSelection::Silence => self.music.silence(engine)?,
            ZoneMusicSelection::Cue(cue) if cue.sound_entry_id() == 0 => {}
            ZoneMusicSelection::Cue(cue) => {
                if let Some(load) = self.music.select(
                    engine,
                    cue.sound_entry_id(),
                    Some(cue),
                    false,
                    next_random_word,
                )? {
                    loads.push(load);
                }
            }
        }
        let ambience = self.state.ambience(frame.time, frame.options);
        match ambience {
            None => self.ambience.silence(engine)?,
            Some(0) => {}
            Some(entry_id) => {
                if let Some(load) = self.ambience.select(
                    engine,
                    entry_id,
                    None,
                    self.underwater_changed,
                    next_random_word,
                )? {
                    loads.push(load);
                }
            }
        }
        self.underwater_changed = false;
        Ok(loads)
    }

    /// Attaches an admitted generation to its still-owned asynchronous request.
    ///
    /// # Errors
    /// Returns a backend error applying the initial fade envelope.
    pub fn complete_load(
        &mut self,
        engine: &mut SoundEngine<'_>,
        load: SoundLoadHandle,
        playback: SoundPlayback,
    ) -> Result<(), SoundEngineError> {
        self.music.complete_load(engine, load, playback)?;
        self.ambience.complete_load(engine, load, playback)
    }

    /// Cancels world loads and stops every owned generation on world teardown.
    /// Intro cooldowns remain process-owned across world transitions.
    ///
    /// # Errors
    /// Returns a backend stop failure.
    pub fn clear(&mut self, engine: &mut SoundEngine<'_>) -> Result<(), SoundEngineError> {
        self.music.clear(engine)?;
        self.ambience.clear(engine)?;
        self.underwater_changed = false;
        Ok(())
    }
}
