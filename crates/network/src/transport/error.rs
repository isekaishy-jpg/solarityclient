//! Stable TCP failures independent of Tokio and operating-system error types.

use thiserror::Error;

/// A failure while validating or opening a network transport.
#[derive(Debug, Error)]
pub enum TransportError {
    /// A configured or realm-advertised authority is malformed.
    #[error("invalid TCP endpoint {authority:?}: {reason}")]
    InvalidEndpoint {
        /// Rejected authority text.
        authority: String,
        /// Stable validation reason.
        reason: &'static str,
    },
    /// DNS resolution or the TCP connection attempt failed.
    #[error("could not connect to {endpoint}: {message}")]
    Connect {
        /// Requested endpoint.
        endpoint: String,
        /// Operating-system context without exposing Tokio error types.
        message: String,
    },
    /// Required socket configuration failed after connecting.
    #[error("could not configure connected socket for {endpoint}: {message}")]
    Configure {
        /// Connected endpoint.
        endpoint: String,
        /// Operating-system context without exposing Tokio error types.
        message: String,
    },
}
