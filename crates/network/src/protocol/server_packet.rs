//! Owned wrapper around one decrypted server packet.

use super::{CharacterDirectory, CharacterDirectoryError};

const SMSG_CHAR_ENUM: u16 = 0x003B;

/// A decoded world packet that retains unsupported payloads for later dispatch.
pub struct WorldServerPacket {
    opcode: u16,
    payload: Vec<u8>,
}

impl WorldServerPacket {
    pub(crate) const fn new(opcode: u16, payload: Vec<u8>) -> Self {
        Self { opcode, payload }
    }

    /// Returns the raw build-12340 opcode.
    #[must_use]
    pub const fn opcode(&self) -> u16 {
        self.opcode
    }

    /// Returns the stock name for protocol packets implemented at this boundary.
    #[must_use]
    pub const fn name(&self) -> Option<&'static str> {
        match self.opcode {
            SMSG_CHAR_ENUM => Some("SMSG_CHAR_ENUM"),
            0x01DD => Some("SMSG_PONG"),
            0x01EE => Some("SMSG_AUTH_RESPONSE"),
            0x02EF => Some("SMSG_ADDON_INFO"),
            _ => None,
        }
    }

    /// Returns the decrypted packet body exactly as received.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Decodes `SMSG_CHAR_ENUM`, or returns `None` for another retained opcode.
    ///
    /// # Errors
    ///
    /// Returns [`CharacterDirectoryError`] when a character packet is malformed.
    pub fn character_directory(
        &self,
    ) -> Result<Option<CharacterDirectory>, CharacterDirectoryError> {
        if self.opcode != SMSG_CHAR_ENUM {
            return Ok(None);
        }
        CharacterDirectory::decode(&self.payload).map(Some)
    }
}

impl std::fmt::Debug for WorldServerPacket {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WorldServerPacket")
            .field("opcode", &format_args!("{:#06X}", self.opcode))
            .field("name", &self.name())
            .field("payload_bytes", &self.payload.len())
            .finish_non_exhaustive()
    }
}
