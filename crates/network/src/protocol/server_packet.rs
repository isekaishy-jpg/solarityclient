//! Owned wrapper around one decrypted server packet.

use super::{
    AddonPolicyError, CharacterDirectory, CharacterDirectoryError, CharacterLoginRejection,
    ObjectUpdateError, WorldAddonManifest, WorldAddonPolicy, WorldEntryPacketError, WorldLocation,
    WorldObjectUpdateBatch,
};

const SMSG_CHAR_ENUM: u16 = 0x003B;
const SMSG_CHARACTER_LOGIN_FAILED: u16 = 0x0041;
const SMSG_LOGIN_VERIFY_WORLD: u16 = 0x0236;
const SMSG_ADDON_INFO: u16 = 0x02EF;

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
            SMSG_CHARACTER_LOGIN_FAILED => Some("SMSG_CHARACTER_LOGIN_FAILED"),
            0x00A9 => Some("SMSG_UPDATE_OBJECT"),
            0x01DD => Some("SMSG_PONG"),
            0x01F6 => Some("SMSG_COMPRESSED_UPDATE_OBJECT"),
            0x01EE => Some("SMSG_AUTH_RESPONSE"),
            SMSG_LOGIN_VERIFY_WORLD => Some("SMSG_LOGIN_VERIFY_WORLD"),
            SMSG_ADDON_INFO => Some("SMSG_ADDON_INFO"),
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

    /// Decodes positional `SMSG_ADDON_INFO` policy against the sent manifest.
    ///
    /// Returns `None` for another retained opcode. The manifest must be the
    /// exact ordered manifest used to authenticate this world session.
    ///
    /// # Errors
    ///
    /// Returns [`AddonPolicyError`] when an add-on policy packet is malformed.
    pub fn addon_policy(
        &self,
        manifest: &WorldAddonManifest,
    ) -> Result<Option<WorldAddonPolicy>, AddonPolicyError> {
        if self.opcode != SMSG_ADDON_INFO {
            return Ok(None);
        }
        WorldAddonPolicy::decode(&self.payload, manifest).map(Some)
    }

    /// Decodes `SMSG_LOGIN_VERIFY_WORLD`, or returns `None` for another opcode.
    ///
    /// # Errors
    ///
    /// Returns [`WorldEntryPacketError`] when the location body is not exactly
    /// the build-12340 20-byte representation.
    pub fn world_location(&self) -> Result<Option<WorldLocation>, WorldEntryPacketError> {
        if self.opcode != SMSG_LOGIN_VERIFY_WORLD {
            return Ok(None);
        }
        WorldLocation::decode(&self.payload).map(Some)
    }

    /// Decodes `SMSG_CHARACTER_LOGIN_FAILED`, or returns `None` for another opcode.
    ///
    /// # Errors
    ///
    /// Returns [`WorldEntryPacketError`] unless the result body is exactly one byte.
    pub fn character_login_rejection(
        &self,
    ) -> Result<Option<CharacterLoginRejection>, WorldEntryPacketError> {
        if self.opcode != SMSG_CHARACTER_LOGIN_FAILED {
            return Ok(None);
        }
        CharacterLoginRejection::decode(&self.payload).map(Some)
    }

    /// Decodes normal or zlib-compressed object updates.
    ///
    /// Returns `None` for another retained opcode.
    ///
    /// # Errors
    ///
    /// Returns [`ObjectUpdateError`] when framing, compression, movement, or
    /// update-mask data is malformed or exceeds the explicit packet bound.
    pub fn object_updates(&self) -> Result<Option<WorldObjectUpdateBatch>, ObjectUpdateError> {
        if !matches!(self.opcode, 0x00A9 | 0x01F6) {
            return Ok(None);
        }
        WorldObjectUpdateBatch::decode(self.opcode, &self.payload).map(Some)
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
