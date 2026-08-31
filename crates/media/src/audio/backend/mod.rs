//! FMOD-facing stock backend and vendor implementation boundary.

mod dependency;
mod status;
mod types;

pub use dependency::{SoundBackend, SoundOutput};
pub use status::SoundBackendError;
pub use types::{SoundOutputInfo, SoundOutputTarget, SoundVoiceHandle, SoundVoiceState};
