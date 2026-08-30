//! Build-12340 world liveness and clock-synchronization packet bodies.

use thiserror::Error;

/// A malformed `SMSG_PONG` or `SMSG_TIME_SYNC_REQ` body.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("malformed {packet} body: expected 4 bytes, found {actual}")]
pub struct WorldLivenessPacketError {
    packet: &'static str,
    actual: usize,
}

impl WorldLivenessPacketError {
    pub(super) const fn new(packet: &'static str, actual: usize) -> Self {
        Self { packet, actual }
    }

    /// Returns the stock packet name.
    #[must_use]
    pub const fn packet(&self) -> &'static str {
        self.packet
    }

    /// Returns the rejected body extent.
    #[must_use]
    pub const fn actual(&self) -> usize {
        self.actual
    }
}

pub(super) fn decode_u32(
    payload: &[u8],
    packet: &'static str,
) -> Result<u32, WorldLivenessPacketError> {
    let bytes: [u8; 4] = payload
        .try_into()
        .map_err(|_| WorldLivenessPacketError::new(packet, payload.len()))?;
    Ok(u32::from_le_bytes(bytes))
}
