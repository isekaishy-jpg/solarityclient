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
mod game_object_query;
mod liveness;
mod movement;
mod movement_message;
mod movement_spline;
mod remote_movement;

pub use remote_movement::{
    MonsterMove, MonsterMovePath, MonsterMoveTransport, MovementPacketError, RemoteMovement,
};
mod object_update;
mod player_control;
mod server_packet;
mod world_entry;
mod world_state;
mod world_time;
mod world_transfer;
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
pub use game_object_query::{
    GameObjectQueryPacketError, GameObjectQueryResponse, GameObjectTemplate,
};
pub use liveness::WorldLivenessPacketError;
pub use movement::{ObjectMovementContext, ObjectMovementFall, ObjectMovementTransport};
pub use movement_message::{
    WorldMovementEncodeError, WorldMovementField, WorldMovementKind, WorldMovementMessage,
};
pub use movement_spline::{MovementSplineFacing, MovementSplineSnapshot};
pub use object_update::{
    ObjectFieldUpdate, ObjectMovementSpeeds, ObjectMovementUpdate, ObjectPositionTransport,
    ObjectUpdateError, WorldObjectKind, WorldObjectUpdate, WorldObjectUpdateBatch,
};
pub use player_control::{WorldClientControlUpdate, WorldPlayerControlPacketError};
pub use server_packet::WorldServerPacket;
pub use world_entry::{
    CharacterLoginRejection, CharacterLoginRejectionReason, WorldEntryPacketError, WorldLocation,
};
pub use world_state::{WorldStatePacketError, WorldStateUpdate};
pub use world_time::{WorldTimePacketError, WorldTimeSpeed};
pub use world_transfer::{WorldTransfer, WorldTransferPacketError, WorldTransferTransport};
