//! World-transfer packets registered by build 12340 at `0x00405540`.

use thiserror::Error;

use super::WorldLocation;

/// An authoritative loading-card or destination-world transition.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WorldTransfer {
    /// `0x00401480` opens the destination card without replacing the world.
    Pending {
        /// Destination Map.dbc identifier.
        map_id: u32,
        /// Optional ship entry and source map consumed by `0x0040AD50`.
        transport: Option<WorldTransferTransport>,
    },
    /// `0x00403D10` schedules replacement, including for the current map ID.
    NewWorld(WorldLocation),
    /// `0x00403DE0` replaces only a different map, without a worldport ACK.
    VerifyWorld(WorldLocation),
    /// `0x00403910` dismisses the card and preserves the active world.
    Aborted {
        /// Map whose transfer was rejected.
        map_id: u32,
        /// Exact stock rejection code, including unrecognized values.
        reason: u8,
        /// Expansion, difficulty, or unique-message index for reasons 7–9.
        argument: Option<u8>,
    },
}

/// Transport context appended to a pending-transfer packet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldTransferTransport {
    entry: u32,
    source_map_id: u32,
}

impl WorldTransferTransport {
    /// Returns the transport's template entry, not its instance GUID.
    #[must_use]
    pub const fn entry(self) -> u32 {
        self.entry
    }

    /// Returns the map from which the transport is departing.
    #[must_use]
    pub const fn source_map_id(self) -> u32 {
        self.source_map_id
    }
}

impl WorldTransfer {
    /// Decodes only the four opcodes owned by the stock transfer handlers.
    pub(crate) fn decode(
        opcode: u16,
        payload: &[u8],
    ) -> Result<Option<Self>, WorldTransferPacketError> {
        let malformed = || WorldTransferPacketError {
            opcode,
            payload_bytes: payload.len(),
        };
        Ok(Some(match opcode {
            0x003E | 0x0236 => {
                let location = WorldLocation::decode(payload).map_err(|_| malformed())?;
                if opcode == 0x003E {
                    Self::NewWorld(location)
                } else {
                    Self::VerifyWorld(location)
                }
            }
            0x003F => {
                if !matches!(payload.len(), 4 | 12) {
                    return Err(malformed());
                }
                Self::Pending {
                    map_id: read_u32(payload, 0),
                    transport: (payload.len() == 12).then(|| WorldTransferTransport {
                        entry: read_u32(payload, 4),
                        source_map_id: read_u32(payload, 8),
                    }),
                }
            }
            0x0040 => {
                let Some(&reason) = payload.get(4) else {
                    return Err(malformed());
                };
                let has_argument = matches!(reason, 7..=9);
                if payload.len() != 5 + usize::from(has_argument) {
                    return Err(malformed());
                }
                Self::Aborted {
                    map_id: read_u32(payload, 0),
                    reason,
                    argument: payload.get(5).copied(),
                }
            }
            _ => return Ok(None),
        }))
    }
}

/// A transfer packet whose body does not match its stock wire fields.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("malformed world-transfer opcode {opcode:#06x} with {payload_bytes} payload bytes")]
pub struct WorldTransferPacketError {
    opcode: u16,
    payload_bytes: usize,
}

/// Reads a little-endian field after the enclosing shape has been checked.
fn read_u32(payload: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        payload[offset],
        payload[offset + 1],
        payload[offset + 2],
        payload[offset + 3],
    ])
}
