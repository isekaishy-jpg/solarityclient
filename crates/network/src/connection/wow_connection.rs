//! Build-12340 transition from selected realmd realm to encrypted world state.

use tokio::io::{AsyncRead, AsyncWrite};
use wow_srp::wrath_header::{ClientCrypto, ProofSeed};
use wow_world_messages::wrath::opcodes::{ClientOpcodeMessage, ServerOpcodeMessage};
use wow_world_messages::wrath::{CMSG_AUTH_SESSION, Expansion, SMSG_AUTH_RESPONSE};

use crate::authentication::WorldIdentity;
use crate::protocol::WorldAddonManifest;
use crate::realm::RealmEntry;

use super::{
    AccountExpansion, WorldAuthError, WorldAuthFailure, WorldAuthProgress, WorldAuthStage,
    WorldQueue, WorldSession, WorldSessionInfo,
};

const CLIENT_BUILD: u32 = 12_340;

/// Stateless entry point for a selected world-server connection.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorldConnection;

impl WorldConnection {
    /// Proves the realmd session to a selected world and reads its first response.
    ///
    /// The legacy Grunt path sends zero for Battle.net-only routing fields and
    /// sends the selected realmd identifier in the dedicated realm field.
    ///
    /// # Errors
    ///
    /// Returns [`WorldAuthError`] when packet order, transport, decode, or
    /// authentication validation fails.
    pub async fn authenticate<S>(
        mut stream: S,
        identity: WorldIdentity,
        realm: &RealmEntry,
        addons: WorldAddonManifest,
    ) -> Result<WorldAuthProgress<S>, WorldAuthError>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send,
    {
        let message = ServerOpcodeMessage::tokio_read_unencrypted(&mut stream)
            .await
            .map_err(|error| WorldAuthError::Decode {
                stage: WorldAuthStage::Challenge,
                message: error.to_string(),
            })?;
        let challenge = match message {
            ServerOpcodeMessage::SMSG_AUTH_CHALLENGE(challenge) => challenge,
            message => {
                return Err(unexpected(
                    WorldAuthStage::Challenge,
                    "SMSG_AUTH_CHALLENGE",
                    message,
                ));
            }
        };

        let proof_seed = ProofSeed::new();
        let (client_proof, crypto) = proof_seed.into_client_header_crypto(
            &identity.account_name,
            *identity.session_key.as_bytes(),
            challenge.server_seed,
        );
        let account_name = identity.account_name.as_ref().to_owned();
        let realm_id = realm.id();
        let auth_session = CMSG_AUTH_SESSION {
            client_build: CLIENT_BUILD,
            login_server_id: 0,
            username: account_name.clone(),
            login_server_type: 0,
            client_seed: proof_seed.seed(),
            region_id: 0,
            battleground_id: 0,
            realm_id: u32::from(realm_id),
            dos_response: 0,
            client_proof,
            addon_info: addons.encode(),
        };
        ClientOpcodeMessage::CMSG_AUTH_SESSION(Box::new(auth_session))
            .tokio_write_unencrypted_client(&mut stream)
            .await
            .map_err(|error| WorldAuthError::Io {
                stage: WorldAuthStage::SessionProof,
                message: error.to_string(),
            })?;

        WorldHandshake {
            stream,
            crypto,
            account_name,
            realm_id,
            addon_manifest: addons,
        }
        .advance()
        .await
    }
}

pub(crate) struct WorldHandshake<S> {
    stream: S,
    crypto: ClientCrypto,
    account_name: String,
    realm_id: u8,
    addon_manifest: WorldAddonManifest,
}

impl<S> WorldHandshake<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    pub(super) async fn advance(mut self) -> Result<WorldAuthProgress<S>, WorldAuthError> {
        let message =
            ServerOpcodeMessage::tokio_read_encrypted(&mut self.stream, self.crypto.decrypter())
                .await
                .map_err(|error| WorldAuthError::Decode {
                    stage: WorldAuthStage::Response,
                    message: error.to_string(),
                })?;
        let response = match message {
            ServerOpcodeMessage::SMSG_AUTH_RESPONSE(response) => *response,
            message => {
                return Err(unexpected(
                    WorldAuthStage::Response,
                    "SMSG_AUTH_RESPONSE",
                    message,
                ));
            }
        };

        match response {
            SMSG_AUTH_RESPONSE::AuthOk {
                billing_flags,
                billing_rested,
                billing_time,
                expansion,
            } => {
                let (encrypter, decrypter) = self.crypto.split();
                Ok(WorldAuthProgress::Authenticated(WorldSession {
                    stream: self.stream,
                    encrypter,
                    decrypter,
                    account_name: self.account_name,
                    realm_id: self.realm_id,
                    addon_manifest: self.addon_manifest,
                    info: WorldSessionInfo {
                        billing_time,
                        billing_flags: billing_flags.as_int(),
                        billing_rested,
                        expansion: account_expansion(expansion),
                    },
                }))
            }
            SMSG_AUTH_RESPONSE::AuthWaitQueue {
                queue_position,
                realm_has_free_character_migration,
            } => Ok(WorldAuthProgress::Queued(WorldQueue {
                handshake: self,
                position: queue_position,
                realm_has_free_character_migration,
            })),
            response => match auth_failure(response) {
                Some(failure) => Err(WorldAuthError::Rejected { failure }),
                None => Err(WorldAuthError::UnexpectedMessage {
                    stage: WorldAuthStage::Response,
                    expected: "AUTH_OK or AUTH_WAIT_QUEUE",
                    received: response.to_string(),
                }),
            },
        }
    }
}

fn unexpected(
    stage: WorldAuthStage,
    expected: &'static str,
    message: ServerOpcodeMessage,
) -> WorldAuthError {
    WorldAuthError::UnexpectedMessage {
        stage,
        expected,
        received: message.to_string(),
    }
}

const fn account_expansion(expansion: Expansion) -> AccountExpansion {
    match expansion {
        Expansion::Vanilla => AccountExpansion::Original,
        Expansion::TheBurningCrusade => AccountExpansion::TheBurningCrusade,
        Expansion::WrathOfTheLichKing => AccountExpansion::WrathOfTheLichKing,
    }
}

const fn auth_failure(response: SMSG_AUTH_RESPONSE) -> Option<WorldAuthFailure> {
    let failure = match response {
        SMSG_AUTH_RESPONSE::AuthFailed => WorldAuthFailure::Failed,
        SMSG_AUTH_RESPONSE::AuthReject => WorldAuthFailure::Rejected,
        SMSG_AUTH_RESPONSE::AuthBadServerProof => WorldAuthFailure::BadServerProof,
        SMSG_AUTH_RESPONSE::AuthUnavailable => WorldAuthFailure::Unavailable,
        SMSG_AUTH_RESPONSE::AuthSystemError => WorldAuthFailure::SystemError,
        SMSG_AUTH_RESPONSE::AuthBillingError => WorldAuthFailure::BillingError,
        SMSG_AUTH_RESPONSE::AuthBillingExpired => WorldAuthFailure::BillingExpired,
        SMSG_AUTH_RESPONSE::AuthVersionMismatch => WorldAuthFailure::VersionMismatch,
        SMSG_AUTH_RESPONSE::AuthUnknownAccount => WorldAuthFailure::UnknownAccount,
        SMSG_AUTH_RESPONSE::AuthIncorrectPassword => WorldAuthFailure::IncorrectPassword,
        SMSG_AUTH_RESPONSE::AuthSessionExpired => WorldAuthFailure::SessionExpired,
        SMSG_AUTH_RESPONSE::AuthServerShuttingDown => WorldAuthFailure::ServerShuttingDown,
        SMSG_AUTH_RESPONSE::AuthAlreadyLoggingIn => WorldAuthFailure::AlreadyLoggingIn,
        SMSG_AUTH_RESPONSE::AuthLoginServerNotFound => WorldAuthFailure::LoginServerNotFound,
        SMSG_AUTH_RESPONSE::AuthBanned => WorldAuthFailure::Banned,
        SMSG_AUTH_RESPONSE::AuthAlreadyOnline => WorldAuthFailure::AlreadyOnline,
        SMSG_AUTH_RESPONSE::AuthNoTime => WorldAuthFailure::NoTime,
        SMSG_AUTH_RESPONSE::AuthDbBusy => WorldAuthFailure::DatabaseBusy,
        SMSG_AUTH_RESPONSE::AuthSuspended => WorldAuthFailure::Suspended,
        SMSG_AUTH_RESPONSE::AuthParentalControl => WorldAuthFailure::ParentalControl,
        SMSG_AUTH_RESPONSE::AuthLockedEnforced => WorldAuthFailure::LockedEnforced,
        SMSG_AUTH_RESPONSE::RealmListRealmNotFound => WorldAuthFailure::RealmNotFound,
        _ => return None,
    };
    Some(failure)
}
