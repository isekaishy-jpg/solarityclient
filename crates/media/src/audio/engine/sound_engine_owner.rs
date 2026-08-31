//! Stable ownership of the SDL mixer and its borrowing stock engine.

#![allow(unsafe_code)]

use std::ops::{Deref, DerefMut};

use solarity_asset::AssetStore;

use crate::audio::backend::{SoundOutput, SoundOutputTarget};

use super::{SoundEngine, SoundEngineError, SoundEngineSettings, SoundSoftwareChannelCount};

/// Movable owner of one output allocation and every track borrowing it.
///
/// SDL's safe wrapper gives each track the lifetime of its parent mixer. The
/// mixer therefore lives in a stable `Box`, while the engine is declared first
/// so Rust destroys all tracks before destroying that mixer allocation.
pub struct OwnedSoundEngine {
    engine: SoundEngine<'static>,
    _output: Box<SoundOutput>,
}

impl OwnedSoundEngine {
    /// Opens an explicit output and loads the complete stock sound engine.
    ///
    /// # Errors
    ///
    /// Returns [`SoundEngineError`] when output creation, DBC loading, decoder
    /// initialization, or exact track-pool allocation fails.
    pub fn load(
        store: &mut AssetStore,
        target: SoundOutputTarget,
        software_channel_count: SoundSoftwareChannelCount,
        settings: SoundEngineSettings,
    ) -> Result<Self, SoundEngineError> {
        let output = Box::new(SoundOutput::open(target)?);
        let output_pointer = &raw const *output;
        // SAFETY: `output_pointer` points into a Box allocation whose address
        // never changes. `output` is retained by the returned owner, and field
        // declaration order drops `engine` and all of its tracks first. No
        // reference carrying this artificial lifetime can be moved out of the
        // private engine field.
        let output_reference = unsafe { &*output_pointer };
        let engine = SoundEngine::load(store, output_reference, software_channel_count, settings)?;
        Ok(Self {
            engine,
            _output: output,
        })
    }
}

impl Deref for OwnedSoundEngine {
    type Target = SoundEngine<'static>;

    fn deref(&self) -> &Self::Target {
        &self.engine
    }
}

impl DerefMut for OwnedSoundEngine {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.engine
    }
}
