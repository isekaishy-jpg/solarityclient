//! SDL owns serialization of callback replacement and execution under its mixer lock.

#![allow(unsafe_code)]

use std::ffi::{c_int, c_void};
use std::sync::Arc;

use sdl3::mixer::Mixer;
use sdl3::mixer::sys::{MIX_Mixer, MIX_SetPostMixCallback};
use sdl3::sys::audio::SDL_AudioSpec;

use crate::recording::RecordingAudio;

use super::SoundBackendError;

/// Stable callback userdata retained until SDL serializes its removal.
pub(super) struct AudioTap {
    pub(super) sink: Arc<RecordingAudio>,
    next_frame: Option<u64>,
}

impl AudioTap {
    /// Install the callback only after allocating its permanently addressed userdata.
    pub(super) fn attach(
        mixer: &Mixer,
        sink: Arc<RecordingAudio>,
    ) -> Result<Box<Self>, SoundBackendError> {
        let mut tap = Box::new(Self {
            sink,
            next_frame: None,
        });
        // SAFETY: The Box's stable address lives until the owner unregisters this
        // callback. SDL holds its mixer lock during replacement and invocation.
        let installed = unsafe {
            MIX_SetPostMixCallback(mixer.raw(), Some(postmix), (&mut *tap as *mut Self).cast())
        };
        if !installed {
            return Err(SoundBackendError::adapter(
                "capture game audio",
                sdl3::get_error(),
            ));
        }
        Ok(tap)
    }
}

/// Unregister under SDL serialization before the owner releases callback userdata.
pub(super) fn detach(mixer: &Mixer) -> Result<(), SoundBackendError> {
    // SAFETY: A live mixer is owned by the caller; unregister waits for any
    // running callback under SDL's lock before its userdata can be released.
    if !unsafe { MIX_SetPostMixCallback(mixer.raw(), None, std::ptr::null_mut()) } {
        return Err(SoundBackendError::adapter(
            "detach recording audio",
            sdl3::get_error(),
        ));
    }
    Ok(())
}

/// Copy final floating-point samples into bounded recording storage using the device clock.
unsafe extern "C" fn postmix(
    userdata: *mut c_void,
    _mixer: *mut MIX_Mixer,
    spec: *const SDL_AudioSpec,
    pcm: *mut f32,
    samples: c_int,
) {
    if userdata.is_null() || spec.is_null() || pcm.is_null() || samples <= 0 {
        return;
    }
    // SAFETY: SDL supplies this callback's retained Box, live format, and exactly
    // `samples` interleaved floats. Its mixer lock excludes concurrent callbacks.
    let (tap, spec, pcm) = unsafe {
        (
            &mut *userdata.cast::<AudioTap>(),
            &*spec,
            std::slice::from_raw_parts(pcm, samples as usize),
        )
    };
    let first = *tap.next_frame.get_or_insert_with(|| {
        (tap.sink.elapsed().as_nanos() * u128::from(tap.sink.rate()) / 1_000_000_000) as u64
    });
    tap.sink.capture(first, pcm, spec.freq, spec.channels);
    tap.next_frame = Some(first.saturating_add((samples as u64) / (spec.channels.max(1) as u64)));
}
