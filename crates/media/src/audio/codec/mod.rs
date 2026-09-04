//! Stock audio decoder admission and retained SDL resource ownership.

mod decoder;
mod dependency;
mod loader;
mod status;
mod types;

pub(in crate::audio) use decoder::SoundDecodeAdmission;
pub use decoder::SoundDecoder;
pub(in crate::audio) use loader::SoundDecodeTicket;
pub use status::SoundDecodeError;
pub use types::{DecodedSoundHandle, DecodedSoundInfo, SoundDecodeMode};
