//! FMOD-facing stock backend and vendor implementation boundary.

mod capture;
mod dependency;
mod status;
mod types;

pub use dependency::{SoundBackend, SoundOutput};
pub use status::SoundBackendError;
pub(in crate::audio) use types::SoundVoiceStart;
pub use types::{
    SoundBackendPlayback, SoundOutputConfiguration, SoundOutputDevice, SoundOutputDeviceId,
    SoundOutputInfo, SoundOutputQuality, SoundOutputTarget, SoundSpatialPosition, SoundVoiceHandle,
    SoundVoicePriority, SoundVoiceState,
};
