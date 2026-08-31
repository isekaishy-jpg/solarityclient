//! Ogg and Vorbis decoding boundary used by stock audio and cinematic streams.

mod decoder;
mod dependency;
mod status;
mod types;

pub use decoder::SoundDecoder;
pub use status::SoundDecodeError;
pub use types::{DecodedSoundHandle, DecodedSoundInfo, SoundDecodeMode};
