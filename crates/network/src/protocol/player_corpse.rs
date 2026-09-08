//! Original corpse location and absent-transport pose responses.

use thiserror::Error;

/// Independent corpse query state, never the resident corpse object's GUID.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WorldPlayerCorpseUpdate {
    /// Opcode 216 with a zero found byte clears the retained query location.
    Missing,
    /// Opcode 216 with any nonzero found byte.
    Location {
        /// Original map used by the range/minimap owner.
        map: u32,
        /// Corpse position, relative to the transport when its counter is nonzero.
        position: [f32; 3],
        /// Display map distinguishes CORPSE_IN_INSTANCE from CORPSE_IN_RANGE.
        display_map: u32,
        /// Low counter converted to the native 1FC00000 high-GUID family.
        transport: u32,
    },
    /// Opcode 4B7 replaces the retained fallback matrix without a GUID gate.
    Transport {
        /// Translation of the absent transport.
        position: [f32; 3],
        /// Rotation about the world Z axis.
        orientation: f32,
    },
}

/// A corpse location or transport response ended inside a required field.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("malformed player corpse packet")]
pub struct WorldPlayerCorpsePacketError;

impl WorldPlayerCorpseUpdate {
    pub(super) fn decode(
        opcode: u16,
        body: &[u8],
    ) -> Result<Option<Self>, WorldPlayerCorpsePacketError> {
        let word = |offset: usize| {
            body.get(offset..offset + 4)
                .and_then(|bytes| bytes.try_into().ok())
                .map(u32::from_le_bytes)
                .ok_or(WorldPlayerCorpsePacketError)
        };
        let position = |offset| -> Result<_, WorldPlayerCorpsePacketError> {
            Ok([
                f32::from_bits(word(offset)?),
                f32::from_bits(word(offset + 4)?),
                f32::from_bits(word(offset + 8)?),
            ])
        };
        Ok(Some(match opcode {
            0x216 => {
                if *body.first().ok_or(WorldPlayerCorpsePacketError)? == 0 {
                    Self::Missing
                } else {
                    Self::Location {
                        map: word(1)?,
                        position: position(5)?,
                        display_map: word(17)?,
                        transport: word(21)?,
                    }
                }
            }
            0x4b7 => Self::Transport {
                position: position(0)?,
                orientation: f32::from_bits(word(12)?),
            },
            _ => return Ok(None),
        }))
    }
}
