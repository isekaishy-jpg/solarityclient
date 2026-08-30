//! Typed Wrath login and world packet encoding and decoding.
//!
//! This module adapts the pinned `wow_login_messages` and
//! `wow_world_messages` crates into stable Solarity types. Opcode dispatch and
//! malformed-packet behavior must remain build-12340 specific.

mod addon_manifest;
mod character_directory;
mod server_packet;
mod wow_svcs_client_services;

pub use addon_manifest::{AddonManifestError, WorldAddon, WorldAddonManifest};
pub use character_directory::{
    CharacterAppearance, CharacterClass, CharacterDirectory, CharacterDirectoryError,
    CharacterEntry, CharacterEquipment, CharacterGender, CharacterLocation, CharacterPet,
    CharacterRace,
};
pub use server_packet::WorldServerPacket;
