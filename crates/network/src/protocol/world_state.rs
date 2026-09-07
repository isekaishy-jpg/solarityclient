//! Server world-state fields consumed by zone audio and world-state UI.

use thiserror::Error;

/// Build-12340 world-state initialization or one-field replacement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorldStateUpdate {
    /// 52693A reads map, zone, area, a u16 count, and ordered field/value pairs.
    Initialize {
        /// Server-selected map, zone, and area identifiers.
        location: [u32; 3],
        /// Ordered replacements; omitted keys retain their preceding values.
        values: Vec<(u32, u32)>,
    },
    /// 5269E5 reads one field and its unmodified 32-bit value.
    Value {
        /// World-state field identifier.
        field: u32,
        /// Exact value, including the bit patterns used as signed values.
        value: u32,
    },
}

/// A world-state packet does not contain its declared native fields.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("malformed world-state packet: expected {expected} bytes, found {actual}")]
pub struct WorldStatePacketError {
    expected: usize,
    actual: usize,
}

impl WorldStateUpdate {
    pub(super) fn decode(
        opcode: u16,
        payload: &[u8],
    ) -> Result<Option<Self>, WorldStatePacketError> {
        let header = match opcode {
            0x2c2 => 14,
            0x2c3 => 8,
            _ => return Ok(None),
        };
        if payload.len() < header {
            return Err(WorldStatePacketError {
                expected: header,
                actual: payload.len(),
            });
        }
        let count = if opcode == 0x2c2 {
            usize::from(u16::from_le_bytes([payload[12], payload[13]]))
        } else {
            0
        };
        let expected = header + count * 8;
        if payload.len() != expected {
            return Err(WorldStatePacketError {
                expected,
                actual: payload.len(),
            });
        }
        let word = |offset| {
            u32::from_le_bytes([
                payload[offset],
                payload[offset + 1],
                payload[offset + 2],
                payload[offset + 3],
            ])
        };
        Ok(Some(if opcode == 0x2c2 {
            Self::Initialize {
                location: [word(0), word(4), word(8)],
                values: (0..count)
                    .map(|index| (word(14 + index * 8), word(18 + index * 8)))
                    .collect(),
            }
        } else {
            Self::Value {
                field: word(0),
                value: word(4),
            }
        }))
    }
}
