//! Ordered build-12340 Grunt login and realm-directory exchange.

use std::fmt;

use tokio::io::{AsyncRead, AsyncWrite};
use wow_login_messages::Message;
use wow_login_messages::version_8::opcodes::ServerOpcodeMessage;
use wow_login_messages::version_8::{
    CMD_AUTH_LOGON_CHALLENGE_Server, CMD_AUTH_LOGON_PROOF_Client,
    CMD_AUTH_LOGON_PROOF_Client_SecurityFlag, CMD_AUTH_LOGON_PROOF_Server, CMD_REALM_LIST_Client,
};
use wow_srp::client::SrpClientChallenge;
use wow_srp::{GENERATOR, LARGE_SAFE_PRIME_LITTLE_ENDIAN, PublicKey};

use crate::realm::RealmDirectory;

use super::{
    GruntCredentials, GruntIntegrity, GruntLoginOptions, LoginError, LoginFailure, LoginStage,
};

/// The authenticated 40-byte key shared with realmd and the selected world server.
#[derive(Clone, Eq, PartialEq)]
pub struct WorldSessionKey([u8; 40]);

impl WorldSessionKey {
    /// Returns the key bytes required to initialize Wrath world-header cryptography.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 40] {
        &self.0
    }
}

impl fmt::Debug for WorldSessionKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WorldSessionKey(<redacted>)")
    }
}

/// Authenticated ownership of the login-server stream and session identity.
pub struct AuthenticatedGrunt<S> {
    stream: S,
    account_name: String,
    session_key: WorldSessionKey,
}

impl<S> AuthenticatedGrunt<S> {
    /// Returns the normalized account name used for world authentication.
    #[must_use]
    pub fn account_name(&self) -> &str {
        &self.account_name
    }

    /// Returns the established world-session key.
    #[must_use]
    pub const fn session_key(&self) -> &WorldSessionKey {
        &self.session_key
    }

    /// Consumes login state and returns the underlying transport.
    #[must_use]
    pub fn into_stream(self) -> S {
        self.stream
    }
}

impl<S> AuthenticatedGrunt<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    /// Requests and owns the current realm directory in exact server order.
    ///
    /// # Errors
    ///
    /// Returns [`LoginError`] for transport, decode, or opcode-order failures.
    pub async fn request_realms(&mut self) -> Result<RealmDirectory, LoginError> {
        CMD_REALM_LIST_Client::default()
            .tokio_write(&mut self.stream)
            .await
            .map_err(|error| LoginError::Io {
                stage: LoginStage::RealmList,
                message: error.to_string(),
            })?;
        let message = read_server_message(&mut self.stream, LoginStage::RealmList).await?;
        match message {
            ServerOpcodeMessage::CMD_REALM_LIST(message) => {
                Ok(RealmDirectory::from_protocol(message.realms))
            }
            message => Err(unexpected(
                LoginStage::RealmList,
                "CMD_REALM_LIST_Server",
                message,
            )),
        }
    }
}

/// Stateless entry point for the legacy authentication state machine.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GruntLogin;

impl GruntLogin {
    /// Performs challenge, SRP proof, and mutual server-proof verification.
    ///
    /// # Errors
    ///
    /// Returns [`LoginError`] when packet order, server parameters, credentials,
    /// integrity proof, extra security, transport, or mutual proof validation fails.
    pub async fn authenticate<S, I>(
        mut stream: S,
        credentials: GruntCredentials,
        options: GruntLoginOptions,
        integrity: &I,
    ) -> Result<AuthenticatedGrunt<S>, LoginError>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send,
        I: GruntIntegrity + ?Sized,
    {
        options
            .challenge(credentials.username().to_owned())
            .tokio_write(&mut stream)
            .await
            .map_err(|error| LoginError::Io {
                stage: LoginStage::Challenge,
                message: error.to_string(),
            })?;

        let message = read_server_message(&mut stream, LoginStage::Challenge).await?;
        let (crc_salt, generator, large_safe_prime, salt, security_flag, server_public_key) =
            match message {
                ServerOpcodeMessage::CMD_AUTH_LOGON_CHALLENGE(
                    CMD_AUTH_LOGON_CHALLENGE_Server::Success {
                        crc_salt,
                        generator,
                        large_safe_prime,
                        salt,
                        security_flag,
                        server_public_key,
                    },
                ) => (
                    crc_salt,
                    generator,
                    large_safe_prime,
                    salt,
                    security_flag,
                    server_public_key,
                ),
                ServerOpcodeMessage::CMD_AUTH_LOGON_CHALLENGE(message) => {
                    return Err(LoginError::Rejected {
                        stage: LoginStage::Challenge,
                        failure: challenge_failure(message),
                    });
                }
                message => {
                    return Err(unexpected(
                        LoginStage::Challenge,
                        "CMD_AUTH_LOGON_CHALLENGE_Server",
                        message,
                    ));
                }
            };

        if security_flag.get_pin().is_some() {
            return Err(LoginError::UnsupportedSecurity { mechanism: "PIN" });
        }
        if security_flag.get_matrix_card().is_some() {
            return Err(LoginError::UnsupportedSecurity {
                mechanism: "matrix card",
            });
        }
        if security_flag.get_authenticator().is_some() {
            return Err(LoginError::UnsupportedSecurity {
                mechanism: "authenticator",
            });
        }

        let generator = match generator.as_slice() {
            [generator] if *generator == GENERATOR => *generator,
            _ => {
                return Err(LoginError::InvalidSrpParameters {
                    message: "generator is not the stock single-byte value 7".to_owned(),
                });
            }
        };
        let large_safe_prime: [u8; 32] =
            large_safe_prime.try_into().map_err(|prime: Vec<u8>| {
                LoginError::InvalidSrpParameters {
                    message: format!("large safe prime has {} bytes instead of 32", prime.len()),
                }
            })?;
        if large_safe_prime != LARGE_SAFE_PRIME_LITTLE_ENDIAN {
            return Err(LoginError::InvalidSrpParameters {
                message: "large safe prime does not match the stock SRP group".to_owned(),
            });
        }
        let server_public_key = PublicKey::from_le_bytes(server_public_key).map_err(|error| {
            LoginError::InvalidSrpParameters {
                message: format!("invalid server public key: {error}"),
            }
        })?;
        let challenge = SrpClientChallenge::new(
            credentials.username.clone(),
            credentials.password,
            generator,
            large_safe_prime,
            server_public_key,
            salt,
        );
        let integrity_hash = integrity.proof(crc_salt)?;
        let proof = CMD_AUTH_LOGON_PROOF_Client {
            client_public_key: *challenge.client_public_key(),
            client_proof: *challenge.client_proof(),
            crc_hash: integrity_hash,
            telemetry_keys: Vec::new(),
            security_flag: CMD_AUTH_LOGON_PROOF_Client_SecurityFlag::empty(),
        };
        proof
            .tokio_write(&mut stream)
            .await
            .map_err(|error| LoginError::Io {
                stage: LoginStage::Proof,
                message: error.to_string(),
            })?;

        let message = read_server_message(&mut stream, LoginStage::Proof).await?;
        let server_proof = match message {
            ServerOpcodeMessage::CMD_AUTH_LOGON_PROOF(CMD_AUTH_LOGON_PROOF_Server::Success {
                server_proof,
                ..
            }) => server_proof,
            ServerOpcodeMessage::CMD_AUTH_LOGON_PROOF(message) => {
                return Err(LoginError::Rejected {
                    stage: LoginStage::Proof,
                    failure: proof_failure(message),
                });
            }
            message => {
                return Err(unexpected(
                    LoginStage::Proof,
                    "CMD_AUTH_LOGON_PROOF_Server",
                    message,
                ));
            }
        };
        let client = challenge
            .verify_server_proof(server_proof)
            .map_err(|_| LoginError::ServerProofMismatch)?;
        Ok(AuthenticatedGrunt {
            stream,
            account_name: credentials.username.as_ref().to_owned(),
            session_key: WorldSessionKey(*client.session_key()),
        })
    }
}

async fn read_server_message<S>(
    stream: &mut S,
    stage: LoginStage,
) -> Result<ServerOpcodeMessage, LoginError>
where
    S: AsyncRead + Unpin + Send,
{
    ServerOpcodeMessage::tokio_read(stream)
        .await
        .map_err(|error| LoginError::Decode {
            stage,
            message: error.to_string(),
        })
}

fn unexpected(
    stage: LoginStage,
    expected: &'static str,
    message: ServerOpcodeMessage,
) -> LoginError {
    LoginError::UnexpectedMessage {
        stage,
        expected,
        received: message.to_string(),
    }
}

fn challenge_failure(message: CMD_AUTH_LOGON_CHALLENGE_Server) -> LoginFailure {
    match message {
        CMD_AUTH_LOGON_CHALLENGE_Server::FailUnknown0 => LoginFailure::Unknown0,
        CMD_AUTH_LOGON_CHALLENGE_Server::FailUnknown1 => LoginFailure::Unknown1,
        CMD_AUTH_LOGON_CHALLENGE_Server::FailBanned => LoginFailure::Banned,
        CMD_AUTH_LOGON_CHALLENGE_Server::FailUnknownAccount => LoginFailure::UnknownAccount,
        CMD_AUTH_LOGON_CHALLENGE_Server::FailIncorrectPassword => LoginFailure::IncorrectPassword,
        CMD_AUTH_LOGON_CHALLENGE_Server::FailAlreadyOnline => LoginFailure::AlreadyOnline,
        CMD_AUTH_LOGON_CHALLENGE_Server::FailNoTime => LoginFailure::NoTime,
        CMD_AUTH_LOGON_CHALLENGE_Server::FailDbBusy => LoginFailure::DatabaseBusy,
        CMD_AUTH_LOGON_CHALLENGE_Server::FailVersionInvalid => LoginFailure::VersionInvalid,
        CMD_AUTH_LOGON_CHALLENGE_Server::LoginDownloadFile => LoginFailure::DownloadFile,
        CMD_AUTH_LOGON_CHALLENGE_Server::FailInvalidServer => LoginFailure::InvalidServer,
        CMD_AUTH_LOGON_CHALLENGE_Server::FailSuspended => LoginFailure::Suspended,
        CMD_AUTH_LOGON_CHALLENGE_Server::FailNoAccess => LoginFailure::NoAccess,
        CMD_AUTH_LOGON_CHALLENGE_Server::SuccessSurvey => LoginFailure::Survey,
        CMD_AUTH_LOGON_CHALLENGE_Server::FailParentalcontrol => LoginFailure::ParentalControl,
        CMD_AUTH_LOGON_CHALLENGE_Server::FailLockedEnforced => LoginFailure::LockedEnforced,
        CMD_AUTH_LOGON_CHALLENGE_Server::Success { .. } => unreachable!(),
    }
}

const fn proof_failure(message: CMD_AUTH_LOGON_PROOF_Server) -> LoginFailure {
    match message {
        CMD_AUTH_LOGON_PROOF_Server::FailUnknown0 => LoginFailure::Unknown0,
        CMD_AUTH_LOGON_PROOF_Server::FailUnknown1 => LoginFailure::Unknown1,
        CMD_AUTH_LOGON_PROOF_Server::FailBanned => LoginFailure::Banned,
        CMD_AUTH_LOGON_PROOF_Server::FailUnknownAccount => LoginFailure::UnknownAccount,
        CMD_AUTH_LOGON_PROOF_Server::FailIncorrectPassword => LoginFailure::IncorrectPassword,
        CMD_AUTH_LOGON_PROOF_Server::FailAlreadyOnline => LoginFailure::AlreadyOnline,
        CMD_AUTH_LOGON_PROOF_Server::FailNoTime => LoginFailure::NoTime,
        CMD_AUTH_LOGON_PROOF_Server::FailDbBusy => LoginFailure::DatabaseBusy,
        CMD_AUTH_LOGON_PROOF_Server::FailVersionInvalid => LoginFailure::VersionInvalid,
        CMD_AUTH_LOGON_PROOF_Server::LoginDownloadFile => LoginFailure::DownloadFile,
        CMD_AUTH_LOGON_PROOF_Server::FailInvalidServer => LoginFailure::InvalidServer,
        CMD_AUTH_LOGON_PROOF_Server::FailSuspended => LoginFailure::Suspended,
        CMD_AUTH_LOGON_PROOF_Server::FailNoAccess => LoginFailure::NoAccess,
        CMD_AUTH_LOGON_PROOF_Server::SuccessSurvey => LoginFailure::Survey,
        CMD_AUTH_LOGON_PROOF_Server::FailParentalcontrol => LoginFailure::ParentalControl,
        CMD_AUTH_LOGON_PROOF_Server::FailLockedEnforced => LoginFailure::LockedEnforced,
        CMD_AUTH_LOGON_PROOF_Server::Success { .. } => unreachable!(),
    }
}
