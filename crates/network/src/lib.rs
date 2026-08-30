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
    AuthenticatedGrunt, GruntCredentials, GruntIntegrity, GruntLogin, GruntLoginOptions,
    LoginError, LoginFailure, LoginLocale, LoginStage, WorldIdentity, WorldSessionKey,
};
pub use connection::{
    AccountExpansion, CharacterLogin, WorldAuthError, WorldAuthFailure, WorldAuthProgress,
    WorldAuthStage, WorldConnection, WorldQueue, WorldSession, WorldSessionInfo,
};
pub use protocol::{
    AddonManifestError, AddonPolicyError, BannedAddon, WorldAddon, WorldAddonManifest,
    WorldAddonPolicy, WorldAddonPolicyEntry,
};
pub use protocol::{
    CharacterAppearance, CharacterClass, CharacterDirectory, CharacterDirectoryError,
    CharacterEntry, CharacterEquipment, CharacterGender, CharacterLocation, CharacterPet,
    CharacterRace, WorldServerPacket,
};
pub use realm::{RealmCategory, RealmDirectory, RealmEntry, RealmRecommendation, RealmType};
pub use session::{WorldSessionError, WorldSessionStage};
pub use transport::{TcpEndpoint, TcpTransport, TransportError};
