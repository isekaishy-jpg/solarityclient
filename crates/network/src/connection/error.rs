//! Stable world-handshake failures independent of protocol dependency errors.

use thiserror::Error;

/// A discrete phase of the world-server authentication exchange.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldAuthStage {
    /// Waiting for the server's initial seed challenge.
    Challenge,
    /// Sending the account proof and selected realm.
    SessionProof,
    /// Waiting for an encrypted authentication response.
    Response,
}

impl std::fmt::Display for WorldAuthStage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Challenge => "challenge",
            Self::SessionProof => "session proof",
            Self::Response => "response",
        })
    }
}

/// Stock failures returned while authenticating a world session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldAuthFailure {
    /// Generic authentication failure.
    Failed,
    /// The world rejected the connection.
    Rejected,
    /// The session proof did not match the login-server key.
    BadServerProof,
    /// The service is unavailable.
    Unavailable,
    /// The world encountered an authentication-system error.
    SystemError,
    /// Account billing state is invalid.
    BillingError,
    /// Account billing time has expired.
    BillingExpired,
    /// Client build 12340 was rejected.
    VersionMismatch,
    /// The account was not found.
    UnknownAccount,
    /// The account proof was rejected as an incorrect password.
    IncorrectPassword,
    /// The login-server session has expired.
    SessionExpired,
    /// The world is shutting down.
    ServerShuttingDown,
    /// Another login is already in progress.
    AlreadyLoggingIn,
    /// The login server could not be reached.
    LoginServerNotFound,
    /// The account is banned.
    Banned,
    /// The account is already online.
    AlreadyOnline,
    /// The account has no remaining play time.
    NoTime,
    /// The account database is busy.
    DatabaseBusy,
    /// The account is suspended.
    Suspended,
    /// Parental controls rejected login.
    ParentalControl,
    /// The account lock policy rejected login.
    LockedEnforced,
    /// The selected realm identifier does not match this world.
    RealmNotFound,
}

/// A failure during the build-12340 world authentication transition.
#[derive(Debug, Error)]
pub enum WorldAuthError {
    /// Socket I/O failed in a named exchange phase.
    #[error("world authentication {stage} I/O failed: {message}")]
    Io {
        /// Exchange phase in progress.
        stage: WorldAuthStage,
        /// I/O context without exposing transport implementation types.
        message: String,
    },
    /// A packet could not be decoded as the expected build-12340 message.
    #[error("world authentication {stage} packet decode failed: {message}")]
    Decode {
        /// Exchange phase in progress.
        stage: WorldAuthStage,
        /// Parser context without exposing dependency error types.
        message: String,
    },
    /// A valid packet arrived out of stock protocol order.
    #[error("expected {expected} during world authentication {stage}, received {received}")]
    UnexpectedMessage {
        /// Exchange phase in progress.
        stage: WorldAuthStage,
        /// Expected stock opcode or response name.
        expected: &'static str,
        /// Received stock opcode or response name.
        received: String,
    },
    /// The world returned a typed stock authentication failure.
    #[error("world authentication rejected: {failure:?}")]
    Rejected {
        /// Stable stock result code.
        failure: WorldAuthFailure,
    },
}
