//! Stock network protocol and session boundaries.

#[cfg(not(target_pointer_width = "64"))]
compile_error!("solarity-network supports only 64-bit application targets");

mod account_data;
mod authentication;
mod connection;
mod integrity;
mod protocol;
mod realm;
mod session;
mod transport;

pub use authentication::{
    AuthenticatedGrunt, Build12340WindowsIntegrity, GruntCredentials, GruntIntegrity, GruntLogin,
    GruntLoginOptions, LoginError, LoginFailure, LoginLocale, LoginStage, WorldIdentity,
    WorldSessionKey,
};
pub use connection::{
    AccountExpansion, CharacterLogin, CharacterLoginProgress, InWorldSession, WorldAuthError,
    WorldAuthFailure, WorldAuthProgress, WorldAuthStage, WorldConnection, WorldPacketReader,
    WorldPacketWriter, WorldQueue, WorldSession, WorldSessionInfo,
};
pub use protocol::{
    AddonManifestError, AddonPolicyError, BannedAddon, MonsterMove, MonsterMovePath,
    MonsterMoveTransport, MovementPacketError, RemoteMovement, WorldAddon, WorldAddonManifest,
    WorldAddonPolicy, WorldAddonPolicyEntry,
};
pub use protocol::{
    CharacterAppearance, CharacterClass, CharacterCreation, CharacterCreationError,
    CharacterCreationResult, CharacterDeletionError, CharacterDeletionResult, CharacterDirectory,
    CharacterDirectoryError, CharacterEntry, CharacterEquipment, CharacterGender,
    CharacterLocation, CharacterLoginRejection, CharacterLoginRejectionReason, CharacterNameResult,
    CharacterPet, CharacterRace, CharacterRename, CharacterRenameError, CharacterRenameResult,
    MovementSplineFacing, MovementSplineSnapshot, ObjectFieldUpdate, ObjectMovementContext,
    ObjectMovementFall, ObjectMovementSpeeds, ObjectMovementTransport, ObjectMovementUpdate,
    ObjectPositionTransport, ObjectUpdateError, WORLD_ACTION_BUTTON_COUNT,
    WorldActionButtonPacketError, WorldActionButtonUpdate, WorldActionButtons,
    WorldEntryPacketError, WorldLivenessPacketError, WorldLocation, WorldMovementEncodeError,
    WorldMovementField, WorldMovementKind, WorldMovementMessage, WorldObjectKind,
    WorldObjectUpdate, WorldObjectUpdateBatch, WorldServerPacket, WorldTimePacketError,
    WorldTimeSpeed, WorldTransfer, WorldTransferPacketError, WorldTransferTransport,
};
pub use protocol::{WorldClientControlUpdate, WorldPlayerControlPacketError};
pub use protocol::{WorldStatePacketError, WorldStateUpdate};
pub use realm::{RealmCategory, RealmDirectory, RealmEntry, RealmRecommendation, RealmType};
pub use session::{WorldSessionError, WorldSessionStage};
pub use transport::{TcpEndpoint, TcpTransport, TransportError};
