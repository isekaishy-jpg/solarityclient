//! Stable ownership of the SDL mixer and its borrowing stock engine.

#![allow(unsafe_code)]

use std::rc::Rc;

use solarity_asset::AssetStore;

use crate::audio::backend::{
    SoundOutput, SoundOutputConfiguration, SoundOutputQuality, SoundOutputTarget,
};

use super::{SoundEngine, SoundEngineError, SoundEngineSettings, SoundSoftwareChannelCount};

/// Movable owner of one output allocation and every track borrowing it.
///
/// SDL's safe wrapper gives each track the lifetime of its parent mixer. The
/// output therefore lives in a stable shared allocation, while the engine is
/// declared first so Rust destroys all tracks before destroying the output.
/// Scoped callbacks keep the internal output lifetime inaccessible to callers.
#[doc = include_str!("../../../tests/compile_fail/owned_sound_engine.md")]
pub struct OwnedSoundEngine {
    engine: SoundEngine<'static>,
    _output: Rc<SoundOutput>,
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
        Self::load_configured(
            store,
            SoundOutputConfiguration {
                target,
                quality: SoundOutputQuality::Medium,
            },
            software_channel_count,
            settings,
        )
    }

    /// Opens the requested device and quality, then loads the stock sound catalog.
    ///
    /// # Errors
    /// Returns output, catalog, decoder, or track allocation failures.
    pub fn load_configured(
        store: &mut AssetStore,
        configuration: SoundOutputConfiguration,
        software_channel_count: SoundSoftwareChannelCount,
        settings: SoundEngineSettings,
    ) -> Result<Self, SoundEngineError> {
        let output = Rc::new(SoundOutput::open_configured(configuration)?);
        let output_pointer = Rc::as_ptr(&output);
        // SAFETY: `output_pointer` points into a stable shared allocation whose
        // contents are never moved or mutably borrowed. The owner retains it; field
        // declaration order drops `engine` and all of its tracks first. No
        // reference carrying this artificial lifetime escapes the private field:
        // callbacks are universally quantified over the output lifetime, with
        // a result type independent of it. They cannot extract/swap an engine
        // between owners or insert an engine borrowing a shorter-lived output.
        let output_reference = unsafe { &*output_pointer };
        let engine = SoundEngine::load(store, output_reference, software_channel_count, settings)?;
        Ok(Self {
            engine,
            _output: output,
        })
    }

    /// Applies an explicit native sound restart without replacing logical owners.
    /// Active tracks retire; pending archive completions cannot restart old sounds.
    ///
    /// # Errors
    /// Returns output creation or track allocation failures before replacing output.
    pub fn restart(
        &mut self,
        configuration: SoundOutputConfiguration,
        software_channel_count: SoundSoftwareChannelCount,
    ) -> Result<(), SoundEngineError> {
        let output = Rc::new(SoundOutput::open_configured(configuration)?);
        let output_pointer = Rc::as_ptr(&output);
        // SAFETY: this stable allocation is retained below before the old output
        // retires. restart_output either fails without retaining it, or replaces
        // and destroys every old track. Scoped engine callbacks cannot expose it.
        let output_reference = unsafe { &*output_pointer };
        self.engine
            .restart_output(output_reference, software_channel_count)?;
        self._output = output;
        Ok(())
    }

    /// Reads sound state while keeping the output lifetime inside this owner.
    ///
    /// The result can contain copied diagnostics or owned values, but cannot
    /// borrow the engine or expose its internally retained output lifetime.
    pub fn with_engine<R>(
        &self,
        operation: impl for<'output> FnOnce(&SoundEngine<'output>) -> R,
    ) -> R {
        operation(&self.engine)
    }

    /// Runs sound operations without allowing engine/output ownership to escape.
    ///
    /// The callback may return an owned result, including an operation error.
    /// It must work for any output lifetime, so it cannot exchange this engine
    /// with one borrowing another owner's or a local output allocation.
    pub fn with_engine_mut<R>(
        &mut self,
        operation: impl for<'output> FnOnce(&mut SoundEngine<'output>) -> R,
    ) -> R {
        operation(&mut self.engine)
    }
}
