//! Sound loading, playback, mixing, spatialization, DSP, and zone ambience.
//!
//! The boundary is evidenced by `SoundEngine.cpp`, `SoundInterface2.cpp`,
//! `SoundInterface2DSP.cpp`, `SoundInterface2Internal.cpp`, and
//! `SoundInterface2ZoneSounds.cpp`. SDL3_mixer supplies playback primitives;
//! this module owns stock-facing policy and asset integration.

mod backend;
mod cache;
mod codec;
mod dsp;
mod emitter;
mod engine;
mod spatial;

mod types;
