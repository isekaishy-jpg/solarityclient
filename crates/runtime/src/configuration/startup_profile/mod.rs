//! Persistent startup CVars stored in the stock `WTF\\Config.wtf` location.

mod character_profile;
mod config_wtf;

pub(crate) use character_profile::CharacterProfile;
pub use config_wtf::StartupProfile;
