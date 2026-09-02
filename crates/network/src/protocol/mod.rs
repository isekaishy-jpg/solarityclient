//! Typed Wrath login and world packet encoding and decoding.
//!
//! This module adapts the pinned `wow_login_messages` and
//! `wow_world_messages` crates into stable Solarity types. Opcode dispatch and
//! malformed-packet behavior must remain build-12340 specific.

mod action_buttons;
mod addon_manifest;
mod addon_policy;
mod character_creation;
mod character_deletion;
mod character_directory;
mod character_rename;
mod liveness;
mod object_update;
mod server_packet;
mod world_entry;
mod world_time;
mod wow_svcs_client_services;

pub use action_buttons::{
    WORLD_ACTION_BUTTON_COUNT, WorldActionButtonPacketError, WorldActionButtonUpdate,
    WorldActionButtons,
};
pub use addon_manifest::{AddonManifestError, WorldAddon, WorldAddonManifest};
pub use addon_policy::{AddonPolicyError, BannedAddon, WorldAddonPolicy, WorldAddonPolicyEntry};
pub use character_creation::{CharacterCreation, CharacterCreationError, CharacterCreationResult};
pub use character_deletion::{CharacterDeletionError, CharacterDeletionResult};
pub use character_directory::{
    CharacterAppearance, CharacterClass, CharacterDirectory, CharacterDirectoryError,
    CharacterEntry, CharacterEquipment, CharacterGender, CharacterLocation, CharacterPet,
    CharacterRace,
};
pub use character_rename::{
    CharacterNameResult, CharacterRename, CharacterRenameError, CharacterRenameResult,
};
pub use liveness::WorldLivenessPacketError;
pub use object_update::{
    ObjectFieldUpdate, ObjectMovementSpeeds, ObjectMovementUpdate, ObjectUpdateError,
    WorldObjectKind, WorldObjectUpdate, WorldObjectUpdateBatch,
};
pub use server_packet::WorldServerPacket;
pub use world_entry::{
    CharacterLoginRejection, CharacterLoginRejectionReason, WorldEntryPacketError, WorldLocation,
};
pub use world_time::{WorldTimePacketError, WorldTimeSpeed};
