//! Stable failures after world authentication has enabled header cryptography.

use thiserror::Error;

/// Direction of a failed encrypted world packet operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldSessionStage {
    /// Sending a client packet.
    Send,
    /// Receiving and decoding a server packet.
    Receive,
}

impl std::fmt::Display for WorldSessionStage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Send => "send",
            Self::Receive => "receive",
        })
    }
}

/// A failure while exchanging encrypted build-12340 world packets.
#[derive(Debug, Error)]
pub enum WorldSessionError {
    /// Borrowed duplex halves were replaced with unrelated transports.
    #[error("world session transport halves belong to different connections")]
    MismatchedTransport,
    /// A client packet could not be written to the transport.
    #[error("world session {stage} I/O failed: {message}")]
    Io {
        /// Packet direction in progress.
        stage: WorldSessionStage,
        /// I/O context without exposing transport implementation types.
        message: String,
    },
    /// A server packet could not be decoded after decrypting its header.
    #[error("world session {stage} decode failed: {message}")]
    Decode {
        /// Packet direction in progress.
        stage: WorldSessionStage,
        /// Parser context without exposing dependency error types.
        message: String,
    },
}
