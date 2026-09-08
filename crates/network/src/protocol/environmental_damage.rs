//! Build-12340 environmental damage notifications received by 756800.

use thiserror::Error;

/// One server-authored environmental impact on a unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldEnvironmentalDamage {
    /// Full, unpacked target GUID.
    pub guid: u64,
    /// Native environmental category byte, retained without reinterpretation.
    pub kind: u8,
    /// Signed native damage amount.
    pub amount: i32,
    /// Signed amount absorbed.
    pub absorbed: i32,
    /// Signed amount resisted.
    pub resisted: i32,
}

/// A notification does not have the native 21-byte body.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("malformed environmental damage packet: expected 21 bytes, found {actual}")]
pub struct WorldEnvironmentalDamagePacketError {
    actual: usize,
}

impl WorldEnvironmentalDamage {
    pub(super) fn decode(
        opcode: u16,
        body: &[u8],
    ) -> Result<Option<Self>, WorldEnvironmentalDamagePacketError> {
        if opcode != 0x1fc {
            return Ok(None);
        }
        if body.len() != 21 {
            return Err(WorldEnvironmentalDamagePacketError { actual: body.len() });
        }
        let word = |offset| {
            i32::from_le_bytes([
                body[offset],
                body[offset + 1],
                body[offset + 2],
                body[offset + 3],
            ])
        };
        Ok(Some(Self {
            guid: u64::from_le_bytes([
                body[0], body[1], body[2], body[3], body[4], body[5], body[6], body[7],
            ]),
            kind: body[8],
            amount: word(9),
            absorbed: word(13),
            resisted: word(17),
        }))
    }
}
