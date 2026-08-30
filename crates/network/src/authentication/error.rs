//! Stable login failures independent of packet and SRP dependency errors.

use thiserror::Error;

/// A discrete phase of the legacy login-server exchange.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoginStage {
    /// Initial version and account-name challenge.
    Challenge,
    /// SRP public-key and proof verification.
    Proof,
    /// Authenticated realm-directory request.
    RealmList,
}

impl std::fmt::Display for LoginStage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Challenge => "challenge",
            Self::Proof => "proof",
            Self::RealmList => "realm list",
        })
    }
}

/// Stock login result values returned by build-12340 realmd servers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoginFailure {
    /// Unspecified failure value zero.
    Unknown0,
    /// Unspecified failure value one.
    Unknown1,
    /// The account is banned.
    Banned,
    /// The account does not exist.
    UnknownAccount,
    /// The password proof was rejected.
    IncorrectPassword,
    /// The account is already online.
    AlreadyOnline,
    /// The account has no remaining play time.
    NoTime,
    /// The login database is busy.
    DatabaseBusy,
    /// The client build is invalid.
    VersionInvalid,
    /// The server requested a client patch transfer.
    DownloadFile,
    /// The selected login server is invalid.
    InvalidServer,
    /// The account is suspended.
    Suspended,
    /// The account lacks access.
    NoAccess,
    /// Authentication succeeded but a survey was requested.
    Survey,
    /// Parental controls rejected login.
    ParentalControl,
    /// The account lock policy rejected login.
    LockedEnforced,
}

/// A failure during build-12340 Grunt authentication or realm discovery.
#[derive(Debug, Error)]
pub enum LoginError {
    /// The account name violates stock normalization constraints.
    #[error("invalid account name: {message}")]
    InvalidUsername {
        /// Validation context without exposing SRP dependency types.
        message: String,
    },
    /// The password violates stock normalization constraints.
    #[error("invalid account password: {message}")]
    InvalidPassword {
        /// Validation context without retaining raw password text.
        message: String,
    },
    /// Socket I/O failed in a named exchange phase.
    #[error("login {stage} I/O failed: {message}")]
    Io {
        /// Exchange phase in progress.
        stage: LoginStage,
        /// I/O context without exposing transport implementation types.
        message: String,
    },
    /// A packet could not be decoded as the expected protocol message.
    #[error("login {stage} packet decode failed: {message}")]
    Decode {
        /// Exchange phase in progress.
        stage: LoginStage,
        /// Parser context without exposing dependency error types.
        message: String,
    },
    /// A valid packet arrived out of protocol order.
    #[error("expected {expected} during login {stage}, received {received}")]
    UnexpectedMessage {
        /// Exchange phase in progress.
        stage: LoginStage,
        /// Expected stock opcode name.
        expected: &'static str,
        /// Received stock opcode name.
        received: String,
    },
    /// The server returned a stock login failure.
    #[error("login {stage} rejected: {failure:?}")]
    Rejected {
        /// Exchange phase that rejected the account.
        stage: LoginStage,
        /// Stable stock result code.
        failure: LoginFailure,
    },
    /// The server supplied non-standard or invalid SRP group parameters.
    #[error("login server supplied invalid SRP parameters: {message}")]
    InvalidSrpParameters {
        /// Rejected parameter detail.
        message: String,
    },
    /// The server requested an additional security mechanism not yet supplied.
    #[error("login server requested unsupported {mechanism} security")]
    UnsupportedSecurity {
        /// Stock security mechanism name.
        mechanism: &'static str,
    },
    /// The server proof did not match the locally calculated proof.
    #[error("login server SRP proof did not match")]
    ServerProofMismatch,
}
